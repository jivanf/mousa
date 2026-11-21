//! Collect allowlisted mpv diagnostics without writing its raw, potentially sensitive messages.
use libmpv2::{Mpv, events::Event};
use serde::Serialize;

const MAX_CAUSES: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Cause {
    component: &'static str,
    reason: &'static str,
}

fn classify(prefix: &str, message: &str) -> Option<Cause> {
    let component = match prefix {
        "curl" | "ffmpeg" | "ffmpeg/demuxer" | "stream" | "lavf" => "stream",
        "ytdl_hook" => "mpv_resolver",
        "cplayer" => "player",
        _ if prefix.starts_with("ao/") || prefix == "ao" => "audio_output",
        _ if prefix.starts_with("ad/") || prefix == "ad" => "audio_decoder",
        _ => return None,
    };
    let message = message.to_ascii_lowercase();
    let reason = if message.contains("403 forbidden") || message.contains("http error 403") {
        "http_403"
    } else if message.contains("404 not found") || message.contains("http error 404") {
        "http_404"
    } else if message.contains("429 too many requests") || message.contains("http error 429") {
        "http_429"
    } else if message.contains("http error 4") || message.contains("server returned 4") {
        "http_4xx"
    } else if message.contains("http error 5") || message.contains("server returned 5") {
        "http_5xx"
    } else if message.contains("failed to resolve hostname")
        || message.contains("name or service not known")
        || message.contains("temporary failure in name resolution")
        || message.contains("getaddrinfo failed")
        || message.contains("could not resolve host")
    {
        "dns_failed"
    } else if message.contains("certificate verification failed")
        || message.contains("ssl certificate problem")
        || message.contains("tls handshake")
    {
        "tls_failed"
    } else if message.contains("timed out") || message.contains("connection timeout") {
        "network_timeout"
    } else if message.contains("connection refused")
        || message.contains("connection reset")
        || message.contains("failed to connect")
    {
        "connection_failed"
    } else if message.contains("invalid data found when processing input") {
        "invalid_media_data"
    } else if message.contains("failed to initialize audio")
        || component == "audio_output" && message.contains("failed to")
    {
        "audio_output_failed"
    } else if component == "audio_decoder" && message.contains("failed to") {
        "audio_decoder_failed"
    } else if component == "mpv_resolver" && message.contains("sign in to confirm") {
        "youtube_sign_in_required"
    } else if component == "mpv_resolver" && message.contains("requested format is not available") {
        "youtube_format_unavailable"
    } else if component == "mpv_resolver" && message.contains("video unavailable") {
        "youtube_video_unavailable"
    } else {
        return None;
    };
    Some(Cause { component, reason })
}

pub(crate) struct EngineDiagnostics {
    client: Option<Mpv>,
    causes: Vec<Cause>,
}

impl EngineDiagnostics {
    pub fn new(mpv: &Mpv) -> Self {
        // Use a separate client because malformed UTF-8 in a log message surfaces
        // as an error in libmpv2's event wrapper, just like a failed EndFile.
        let client = mpv.create_client(None).ok().and_then(|client| {
            // SAFETY: the client owns a live handle and the level is a C string.
            let result = unsafe {
                libmpv2_sys::mpv_request_log_messages(client.ctx.as_ptr(), c"warn".as_ptr())
            };
            (result >= 0).then_some(client)
        });
        Self {
            client,
            causes: Vec::new(),
        }
    }

    pub fn available(&self) -> bool {
        self.client.is_some()
    }

    pub fn poll(&mut self) {
        let Some(client) = &self.client else {
            return;
        };
        for _ in 0..64 {
            match client.wait_event(0.) {
                Some(Ok(Event::LogMessage { prefix, text, .. })) => {
                    if let Some(cause) = classify(prefix, text)
                        && self.causes.len() < MAX_CAUSES
                        && !self.causes.contains(&cause)
                    {
                        self.causes.push(cause);
                    }
                }
                None => break,
                _ => {}
            }
        }
    }

    pub fn clear(&mut self) {
        self.causes.clear();
    }

    pub fn take(&mut self) -> Vec<Cause> {
        self.poll();
        std::mem::take(&mut self.causes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_drops_urls_paths_headers_and_unknown_text() {
        let cause = classify(
            "ffmpeg",
            "HTTP error 403 Forbidden for https://example.invalid/?signature=secret",
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(cause).unwrap(),
            serde_json::json!({"component":"stream","reason":"http_403"})
        );
        assert!(classify("ffmpeg", "Authorization: Bearer secret").is_none());
        assert!(classify("/private/path", "HTTP error 403 Forbidden").is_none());
    }

    #[test]
    fn captures_a_real_mpv_http_failure() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            time::{Duration, Instant},
        };

        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/?signature=secret", server.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut socket, _) = server.accept().unwrap();
            let mut request = [0; 4096];
            socket.read(&mut request).unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let mpv = Mpv::with_initializer(|mpv| {
            mpv.set_property("ytdl", "no")?;
            mpv.set_property("idle", "yes")
        })
        .unwrap();
        let events = mpv.create_client(None).unwrap();
        let mut diagnostics = EngineDiagnostics::new(&mpv);
        assert!(diagnostics.available());
        mpv.command("loadfile", &[&url]).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(
                Instant::now() < deadline,
                "expected mpv to reject the stream"
            );
            if matches!(
                events.wait_event(0.1),
                Some(Err(libmpv2::Error::Raw(libmpv2::mpv_error::LoadingFailed)))
                    | Some(Ok(Event::EndFile(4)))
            ) {
                break;
            }
            diagnostics.poll();
        }
        assert!(diagnostics.take().contains(&Cause {
            component: "stream",
            reason: "http_403",
        }));
        assert!(diagnostics.take().is_empty());
    }
}
