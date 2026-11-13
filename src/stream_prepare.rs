//! One speculative URL, never audio bytes. Results live only in memory.
use crate::{db::Track, playback_metrics::Metrics};
use serde_json::{Value, json};
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

// Leave another full preparation window after our bounded resolver times out so
// mpv can still prefetch the YouTube fallback before the current track ends.
pub(crate) const LEAD_SECONDS: f64 = 60.;
const MAX_AGE: Duration = Duration::from_secs(120);

pub(crate) fn next_index(index: usize, len: usize, shuffle: bool, repeat: bool) -> Option<usize> {
    if len == 0 {
        None
    } else if shuffle && len > 1 {
        Some((index + rand::random_range(1..len)) % len)
    } else if index + 1 < len {
        Some(index + 1)
    } else if repeat || shuffle {
        Some(0)
    } else {
        None
    }
}

pub(crate) fn due(position: f64, duration: f64, paused: bool) -> bool {
    !paused
        && position.is_finite()
        && duration.is_finite()
        && duration > 0.
        && position >= 0.
        && duration - position <= LEAD_SECONDS
}

pub(crate) struct Stream {
    pub url: String,
    headers: Vec<String>,
    resolved: Instant,
}
impl Stream {
    fn fresh(&self) -> bool {
        // A successful resolver result is fresh in this process regardless of
        // wall-clock skew between the device and YouTube's signing service.
        self.resolved.elapsed() < MAX_AGE
    }
    // Two nested mpv lists: header fields and loadfile's per-file options.
    pub fn options(&self) -> String {
        let headers = self
            .headers
            .iter()
            .map(|s| s.replace('\\', "\\\\").replace(',', "\\,"))
            .collect::<Vec<_>>()
            .join(",");
        // Keep mpv's resolver script alive. Toggling ytdl per file asynchronously
        // unloads/reloads it while its end/load hooks may still be running.
        format!("http-header-fields={}", quote(&headers))
    }
}
fn quote(s: &str) -> String {
    format!("%{}%{}", s.len(), s)
}

struct Outcome {
    stream: Result<Stream, &'static str>,
    elapsed_ms: u128,
}
pub(crate) struct Preparation {
    pub index: usize,
    url: String,
    purpose: &'static str,
    cancel: Arc<AtomicBool>,
    result: Receiver<Outcome>,
    outcome: Option<Outcome>,
}
impl Preparation {
    pub fn start(index: usize, track: &Track, purpose: &'static str, metrics: &Metrics) -> Self {
        let url = track.url.clone();
        let worker_track = track.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, result) = mpsc::channel();
        let worker_cancel = cancel.clone();
        std::thread::spawn(move || {
            let start = Instant::now();
            let stream = resolve(&worker_track, &worker_cancel);
            let _ = tx.send(Outcome {
                stream,
                elapsed_ms: start.elapsed().as_millis(),
            });
        });
        metrics.emit(json!({"event":"prepare_started","queue_index":index,"purpose":purpose}));
        Self {
            index,
            url,
            purpose,
            cancel,
            result,
            outcome: None,
        }
    }
    pub fn finished(&self) -> bool {
        self.outcome.is_some()
    }
    pub fn unavailable(&self) -> bool {
        matches!(
            &self.outcome,
            Some(Outcome {
                stream: Err("unavailable"),
                ..
            })
        )
    }
    pub fn matches(&self, index: usize, track: &Track) -> bool {
        self.index == index && self.url == track.url
    }
    pub fn needs_refresh(&self) -> bool {
        matches!(&self.outcome, Some(Outcome { stream: Ok(stream), .. }) if stream.resolved.elapsed() >= MAX_AGE)
    }
    #[cfg(test)]
    pub fn pending(index: usize, url: &str) -> Self {
        let (_, result) = mpsc::channel();
        Self {
            index,
            url: url.into(),
            purpose: "test",
            cancel: Arc::new(AtomicBool::new(false)),
            result,
            outcome: None,
        }
    }
    pub fn poll(&mut self, metrics: &Metrics) {
        if self.outcome.is_some() {
            return;
        }
        if let Ok(outcome) = self.result.try_recv() {
            metrics.emit(
                json!({"event":"prepare_finished","queue_index":self.index,"purpose":self.purpose,
                "resolve_ms":outcome.elapsed_ms,
                "outcome":outcome.stream.as_ref().map(|_| "ready").unwrap_or_else(|e| *e)}),
            );
            self.outcome = Some(outcome);
        }
    }
    pub fn take(
        mut self,
        index: usize,
        url: &str,
        metrics: &Metrics,
    ) -> (Option<Stream>, &'static str) {
        self.poll(metrics);
        if self.index != index || self.url != url {
            return (None, "mismatch");
        }
        match self.outcome.take() {
            Some(Outcome { stream: Ok(s), .. }) if s.fresh() => (Some(s), "ready"),
            Some(Outcome { stream: Ok(_), .. }) => (None, "expired"),
            Some(Outcome {
                stream: Err("unavailable"),
                ..
            }) => (None, "unavailable"),
            Some(_) => (None, "failed"),
            None => (None, "pending"),
        }
    }
}
impl Drop for Preparation {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
pub(crate) fn invalidate(
    preparation: &mut Option<Preparation>,
    metrics: &Metrics,
    reason: &'static str,
) {
    if let Some(p) = preparation.take() {
        metrics.emit(json!({"event":"prepare_discarded","queue_index":p.index,"purpose":p.purpose,"reason":reason}));
    }
}

fn parse(bytes: &[u8]) -> Result<Stream, &'static str> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "invalid_json")?;
    // Unsupported multi-resource formats and live streams use mpv's normal resolver.
    if value["is_live"].as_bool() == Some(true) {
        return Err("live_stream");
    }
    let url = value["url"].as_str().ok_or("unsupported_format")?;
    let parsed = url::Url::parse(url).map_err(|_| "invalid_url")?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("unsupported_protocol");
    }
    let mut headers = Vec::new();
    if let Some(fields) = value["http_headers"].as_object() {
        for (name, value) in fields {
            let value = value.as_str().ok_or("invalid_headers")?;
            if name.contains(['\r', '\n', '\0', ':']) || value.contains(['\r', '\n', '\0']) {
                return Err("invalid_headers");
            }
            headers.push(format!("{name}: {value}"));
        }
    }
    Ok(Stream {
        url: url.into(),
        headers,
        resolved: Instant::now(),
    })
}

fn resolve(track: &Track, cancel: &AtomicBool) -> Result<Stream, &'static str> {
    let result = resolve_url(&track.url, cancel);
    let reason = match result.as_ref().err().copied() {
        Some("private") => Some("PRIVATE"),
        Some("removed") => Some("REMOVED"),
        Some("region_blocked") => Some("REGION BLOCKED"),
        Some("unavailable") => Some("UNAVAILABLE"),
        _ => None,
    };
    let Some(reason) = reason else {
        return result;
    };
    if let Some((replacement, stream)) = replacement(track, cancel) {
        if track.id > 0
            && let Ok(db) = crate::db::library()
        {
            let _ = db.conn.execute(
                "UPDATE entries SET url=?,unavailable=NULL WHERE id=? AND url=?",
                rusqlite::params![replacement, track.id, track.url],
            );
        }
        return Ok(stream);
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled");
    }
    if let Ok(db) = crate::db::library() {
        let _ = db.conn.execute(
            "UPDATE entries SET unavailable=? WHERE url=?",
            rusqlite::params![reason, track.url],
        );
    }
    Err("unavailable")
}

fn replacement(track: &Track, cancel: &AtomicBool) -> Option<(String, Stream)> {
    if track.duration <= 0. || track.title.trim().is_empty() || track.artist.trim().is_empty() {
        return None;
    }
    let artist = track
        .artist
        .split('·')
        .next()
        .unwrap_or(&track.artist)
        .trim();
    // Long classical titles often repeat the work and movement around a colon;
    // YouTube search is substantially more accurate with the movement plus one credit.
    let movement = track
        .title
        .rsplit(':')
        .next()
        .unwrap_or(&track.title)
        .trim();
    let query = format!("{movement} {artist}");
    let mut command = Command::new(crate::integrations::ytdlp_path());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.args([
        "--ignore-config",
        "--flat-playlist",
        "--dump-single-json",
        "--no-warnings",
        "--socket-timeout",
        "10",
        "--retries",
        "1",
        "--playlist-end",
        "20",
        "--",
        &format!("ytsearch20:{query}"),
    ]);
    let bytes = command_output(command, cancel, Duration::from_secs(25)).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    value["entries"]
        .as_array()?
        .iter()
        .filter(|item| replacement_matches(track, item))
        .find_map(|item| {
            let url = format!("https://www.youtube.com/watch?v={}", item["id"].as_str()?);
            (url != track.url)
                .then(|| resolve_url(&url, cancel).ok().map(|stream| (url, stream)))?
        })
}

fn replacement_matches(track: &Track, item: &Value) -> bool {
    let Some(title) = item["title"].as_str() else {
        return false;
    };
    let Some(duration) = item["duration"].as_f64() else {
        return false;
    };
    let channel = item["channel"]
        .as_str()
        .or_else(|| item["uploader"].as_str())
        .unwrap_or("")
        .trim_end_matches(" - Topic");
    item["id"].as_str().is_some()
        && replacement_title_similarity(title, &track.title) >= 0.8
        && (duration - track.duration).abs() <= 3.
        && artist_parts(channel)
            .iter()
            .any(|artist| artist_parts(&track.artist).contains(artist))
}

fn replacement_title_similarity(candidate: &str, stored: &str) -> f64 {
    let movement = stored
        .rsplit(" - ")
        .next()
        .unwrap_or(stored)
        .rsplit(':')
        .next()
        .unwrap_or(stored)
        .trim();
    title_similarity(candidate, stored).max(title_similarity(candidate, movement))
}

fn title_similarity(left: &str, right: &str) -> f64 {
    let left = words(left);
    let right = words(right);
    let shorter = left.len().min(right.len());
    if shorter == 0 {
        return 0.;
    }
    left.iter().filter(|word| right.contains(*word)).count() as f64 / shorter as f64
}

fn words(value: &str) -> std::collections::HashSet<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .map(str::to_ascii_lowercase)
        .filter(|word| !word.is_empty())
        .collect()
}

fn artist_parts(value: &str) -> Vec<String> {
    value
        .split(['·', ',', '&'])
        .map(normalize)
        .filter(|part| !part.is_empty())
        .collect()
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn command_output(
    mut command: Command,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<Vec<u8>, &'static str> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "resolver_unavailable")?;
    let mut stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .read_to_end(&mut bytes)
            .map_err(|_| "resolver_read_failed")?;
        (bytes.len() <= 8 * 1024 * 1024)
            .then_some(bytes)
            .ok_or("resolver_output_limit")
    });
    let start = Instant::now();
    let success = loop {
        if cancel.load(Ordering::Relaxed) || start.elapsed() > timeout {
            let _ = child.kill();
            break false;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => break false,
        }
    };
    let _ = child.wait();
    let bytes = reader.join().map_err(|_| "resolver_read_failed")??;
    success.then_some(bytes).ok_or("resolver_failed")
}

fn resolve_url(url: &str, cancel: &AtomicBool) -> Result<Stream, &'static str> {
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled");
    }
    let mut command = Command::new(crate::integrations::ytdlp_path());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command.args([
        "--ignore-config",
        "--no-playlist",
        "--skip-download",
        "--dump-single-json",
        "--no-warnings",
        "--socket-timeout",
        "10",
        "--retries",
        "1",
        "-f",
        "bestaudio/best",
        "--",
        url,
    ]);
    resolve_command(command, cancel, Duration::from_secs(25))
}
fn resolve_command(
    mut command: Command,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<Stream, &'static str> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "resolver_unavailable")?;
    let stderr = child.stderr.take().unwrap();
    // Inspect a bounded diagnostic in memory; never log URLs or raw errors.
    let error_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut stderr = stderr;
        let mut chunk = [0; 4096];
        while let Ok(count) = stderr.read(&mut chunk) {
            if count == 0 {
                break;
            }
            let remaining = 65536usize.saturating_sub(bytes.len());
            bytes.extend_from_slice(&chunk[..count.min(remaining)]);
        }
        bytes
    });
    let stdout = child.stdout.take().unwrap();
    // Drain concurrently, retain at most 8 MiB, and never persist resolver output.
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut stdout = stdout;
        let mut chunk = [0; 8192];
        let mut overflow = false;
        loop {
            let count = stdout
                .read(&mut chunk)
                .map_err(|_| "resolver_read_failed")?;
            if count == 0 {
                break;
            }
            if bytes.len() + count <= 8 * 1024 * 1024 {
                bytes.extend_from_slice(&chunk[..count]);
            } else {
                overflow = true;
            }
        }
        if overflow {
            Err("resolver_output_limit")
        } else {
            Ok(bytes)
        }
    });
    let start = Instant::now();
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            break Err("cancelled");
        }
        if start.elapsed() > timeout {
            break Err("timeout");
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                break if status.success() {
                    Ok(())
                } else {
                    Err("resolver_failed")
                };
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => break Err("resolver_failed"),
        }
    };
    if status.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    let bytes = reader.join().map_err(|_| "resolver_read_failed")?;
    let diagnostic = error_reader.join().unwrap_or_default();
    if status == Err("resolver_failed") {
        if let Some(reason) = unavailable_reason(&diagnostic) {
            return Err(reason);
        }
    }
    status?;
    parse(&bytes?)
}

fn unavailable_reason(diagnostic: &[u8]) -> Option<&'static str> {
    let text = String::from_utf8_lossy(diagnostic).to_ascii_lowercase();
    if text.contains("not available in your country")
        || text.contains("not made this video available in your country")
    {
        Some("region_blocked")
    } else if text.contains("private video") {
        Some("private")
    } else if text.contains("this video has been removed")
        || text.contains("video has been deleted")
    {
        Some("removed")
    } else if text.contains("video unavailable") {
        Some("unavailable")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell_command(unix: &str, windows: &str) -> Command {
        if cfg!(windows) {
            let mut command = Command::new("powershell.exe");
            command.args(["-NoProfile", "-NonInteractive", "-Command", windows]);
            command
        } else {
            let mut command = Command::new("sh");
            command.args(["-c", unix]);
            command
        }
    }
    #[test]
    fn unavailable_video_is_distinguished_from_transient_failure() {
        assert_eq!(
            unavailable_reason(b"Video unavailable: not available in your country"),
            Some("region_blocked")
        );
        assert_eq!(unavailable_reason(b"Private video"), Some("private"));
        assert_eq!(
            unavailable_reason(b"This video has been removed"),
            Some("removed")
        );
        assert_eq!(
            unavailable_reason(b"Sign in to confirm you are not a bot"),
            None
        );
        let command = shell_command(
            "printf 'ERROR: Video unavailable. This video is not available' >&2; exit 1",
            "[Console]::Error.WriteLine('ERROR: Video unavailable. This video is not available'); exit 1",
        );
        // A cold PowerShell start on Windows CI can take several seconds.
        assert_eq!(
            resolve_command(command, &AtomicBool::new(false), Duration::from_secs(30)).err(),
            Some("unavailable")
        );
        assert!(unavailable_reason(b"HTTP Error 503: Service Unavailable").is_none());
        let mut preparation = Preparation::pending(1, "original");
        preparation.outcome = Some(Outcome {
            stream: Err("unavailable"),
            elapsed_ms: 1,
        });
        assert_eq!(
            preparation.take(1, "original", &Metrics::silent()).1,
            "unavailable"
        );
    }
    #[test]
    fn replacements_require_matching_title_artist_and_duration() {
        let track = Track {
            id: 1,
            playlist_index: 1,
            url: "https://www.youtube.com/watch?v=old".into(),
            title: "Liebster Immanuel (Chorus)".into(),
            artist: "Montréal Baroque · Matthew White".into(),
            duration: 283.,
            thumbnail: None,
            unavailable: Some("UNAVAILABLE".into()),
        };
        let candidate = json!({
            "id": "replacement",
            "title": "Liebster Immanuel - Chorus",
            "channel": "Matthew White - Topic",
            "duration": 284.
        });
        assert!(replacement_matches(&track, &candidate));
        assert!(!replacement_matches(
            &track,
            &json!({"id":"replacement","title":"Liebster Immanuel - Chorus","channel":"Matthew White - Topic","duration":300.})
        ));
        assert!(!replacement_matches(
            &track,
            &json!({"id":"replacement","title":"Liebster Immanuel - Chorus","channel":"Different orchestra","duration":284.})
        ));
    }
    #[test]
    fn known_bwv_157_replacement_matches_saved_metadata() {
        let track = Track {
            id: 1,
            playlist_index: 1,
            url: "https://www.youtube.com/watch?v=7v49Kaz31Cs".into(),
            title: "BWV 157 - Ich lasse dich nicht, du segnest mich denn: 1. Duett (Tenor, Bass) - Ich lasse dich nicht, du segnest mich denn".into(),
            artist: "Orchester der J.S. Bach-Stiftung · Rudolf Lutz · Georg Poplutz · Stephan MacLeod".into(),
            duration: 278.,
            thumbnail: None,
            unavailable: Some("UNAVAILABLE".into()),
        };
        let candidate = json!({
            "id": "yWiJEH8qqDk",
            "title": "Ich lasse dich nicht, du segnest mich denn!, BWV 157: No. 1, Duet, Aria. Ich lasse dich nicht,...",
            "channel": "Georg Poplutz - Topic",
            "duration": 278.
        });

        assert!(replacement_matches(&track, &candidate));
    }
    #[test]
    #[ignore = "read-only live YouTube replacement compatibility probe"]
    fn live_unavailable_track_finds_a_playable_replacement() {
        let track = Track {
            id: 0,
            playlist_index: 1,
            url: "https://www.youtube.com/watch?v=QgXAFUtasVE".into(),
            title: "J.S. Bach: Cantata Liebster Immanuel, herzog der frommen, BWV 123: Liebster Immanuel, Herzog der Frommen (Chorus)".into(),
            artist: "Montréal Baroque · Matthew White · Monika Mauch · Charles Daniels · Eric Milnes · Harry van der Kamp".into(),
            duration: 283.,
            thumbnail: None,
            unavailable: Some("UNAVAILABLE".into()),
        };
        let (url, _) = replacement(&track, &AtomicBool::new(false)).unwrap();
        assert_eq!(url, "https://www.youtube.com/watch?v=5FPa8stLD8k");
    }
    #[test]
    fn selection_and_trigger_respect_playback_modes() {
        assert_eq!(next_index(0, 0, false, true), None);
        assert_eq!(next_index(1, 3, false, false), Some(2));
        assert_eq!(next_index(2, 3, false, false), None);
        assert_eq!(next_index(2, 3, false, true), Some(0));
        assert_eq!(next_index(0, 1, true, false), Some(0));
        for _ in 0..100 {
            assert_ne!(next_index(1, 3, true, false), Some(1));
        }
        assert!(!due(39.9, 100., false));
        assert!(due(40., 100., false));
        assert!(due(0., 15., false));
        assert!(!due(90., 100., true));
        assert!(!due(0., 0., false));
        assert!(!due(f64::NAN, 100., false));
    }
    #[test]
    fn parses_headers_without_trusting_wall_clock_expiration() {
        let s = parse(br#"{"url":"https://example.org/audio?expire=1","http_headers":{"User-Agent":"a,b","Referer":"https://example.org"}}"#).unwrap();
        assert!(s.fresh());
        assert!(s.options().contains("User-Agent: a\\,b"));
        assert!(parse(br#"{"url":"file:///tmp/file"}"#).is_err());
        assert!(
            parse(br#"{"url":"https://example.org","http_headers":{"X":"bad\r\nheader"}}"#)
                .is_err()
        );
        assert!(parse(br#"{"requested_formats":[]}"#).is_err());
        assert!(parse(br#"{"url":"https://example.org","is_live":true}"#).is_err());
    }
    #[test]
    fn consuming_pending_mismatched_and_monotonically_stale_results_never_uses_the_url() {
        let m = Metrics::silent();
        let pending = Preparation::pending(1, "original");
        let cancel = pending.cancel.clone();
        assert_eq!(pending.take(1, "original", &m).1, "pending");
        assert!(cancel.load(Ordering::Relaxed));
        let mut ready = Preparation::pending(1, "original");
        ready.outcome = Some(Outcome {
            stream: parse(br#"{"url":"https://example.org/audio"}"#),
            elapsed_ms: 1,
        });
        assert_eq!(ready.take(2, "different", &m).1, "mismatch");
        let mut stale = Preparation::pending(1, "original");
        let mut stream = parse(br#"{"url":"https://example.org/audio?expire=1"}"#).unwrap();
        stream.resolved = Instant::now() - MAX_AGE;
        stale.outcome = Some(Outcome {
            stream: Ok(stream),
            elapsed_ms: 1,
        });
        assert!(stale.needs_refresh());
        assert_eq!(stale.take(1, "original", &m).1, "expired");
        let mut ready = Preparation::pending(1, "original");
        ready.outcome = Some(Outcome {
            stream: parse(br#"{"url":"https://example.org/audio"}"#),
            elapsed_ms: 1,
        });
        assert!(ready.take(1, "original", &m).0.is_some());
    }
    #[test]
    fn cancelled_late_result_cannot_replace_a_new_plan() {
        let m = Metrics::silent();
        let mut old = Preparation::pending(1, "old");
        let (tx, rx) = mpsc::channel();
        old.result = rx;
        let cancel = old.cancel.clone();
        let mut plan = Some(old);
        invalidate(&mut plan, &m, "queue_changed");
        plan = Some(Preparation::pending(2, "new"));
        assert!(cancel.load(Ordering::Relaxed));
        assert!(
            tx.send(Outcome {
                stream: Err("cancelled"),
                elapsed_ms: 1
            })
            .is_err()
        );
        assert_eq!(plan.unwrap().index, 2);
    }
    #[test]
    fn direct_stream_headers_survive_mpv_option_parsing_and_are_file_local() {
        use libmpv2::{Mpv, events::Event};
        use std::{io::Write, net::TcpListener};
        const TRANSITIONS: usize = 13;
        const TRANSITION_TIMEOUT: Duration = Duration::from_secs(15);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/audio.wav", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let start = Instant::now();
            let mut served = 0;
            while served < TRANSITIONS && start.elapsed() < TRANSITION_TIMEOUT * TRANSITIONS as u32
            {
                let Ok((mut socket, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while socket.read_exact(&mut byte).is_ok() {
                    request.push(byte[0]);
                    if request.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                let _ = tx.send(String::from_utf8_lossy(&request).into_owned());
                // One second of PCM silence. No device or remote network involved.
                let mut wav = Vec::new();
                wav.extend_from_slice(b"RIFF");
                wav.extend_from_slice(&16036u32.to_le_bytes());
                wav.extend_from_slice(b"WAVEfmt ");
                wav.extend_from_slice(&16u32.to_le_bytes());
                wav.extend_from_slice(&1u16.to_le_bytes());
                wav.extend_from_slice(&1u16.to_le_bytes());
                wav.extend_from_slice(&8000u32.to_le_bytes());
                wav.extend_from_slice(&16000u32.to_le_bytes());
                wav.extend_from_slice(&2u16.to_le_bytes());
                wav.extend_from_slice(&16u16.to_le_bytes());
                wav.extend_from_slice(b"data");
                wav.extend_from_slice(&16000u32.to_le_bytes());
                wav.resize(16044, 0);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    wav.len()
                );
                let _ = socket.write_all(header.as_bytes());
                let _ = socket.write_all(&wav);
                served += 1;
            }
        });
        let mpv = Mpv::with_initializer(|m| {
            m.set_property("ao", "null")?;
            m.set_property("vo", "null")?;
            m.set_property("speed", 20.)?;
            Ok(())
        })
        .unwrap();
        let events = mpv.create_client(None).unwrap();
        let stream = parse(
            &serde_json::to_vec(
                &json!({"url":url,"http_headers":{"X-Mousa-Test":"a,b=quoted%value"}}),
            )
            .unwrap(),
        )
        .unwrap();
        // Exercise repeated prepared-to-prepared transitions, then ordinary loading.
        for attempt in 0..TRANSITIONS {
            let prepared = attempt < 12;
            if prepared {
                mpv.command("loadfile", &[&url, "replace", "-1", &stream.options()])
                    .unwrap();
            } else {
                mpv.command("loadfile", &[&url, "replace"]).unwrap();
            }
            let start = Instant::now();
            loop {
                assert!(
                    start.elapsed() < TRANSITION_TIMEOUT,
                    "transition {attempt} hung"
                );
                if matches!(events.wait_event(0.1), Some(Ok(Event::FileLoaded))) {
                    assert!(
                        mpv.get_property::<bool>("ytdl").unwrap(),
                        "resolver must stay enabled across transitions"
                    );
                    break;
                }
            }
            let request = rx.recv_timeout(Duration::from_secs(1)).unwrap();
            assert_eq!(request.contains("X-Mousa-Test: a,b=quoted%value"), prepared);
        }
        mpv.command("stop", &[]).unwrap();
        server.join().unwrap();
    }
    #[test]
    fn resolver_process_failure_timeout_and_cancellation_are_bounded() {
        let cancel = AtomicBool::new(false);
        let fail = shell_command("exit 1", "exit 1");
        assert_eq!(
            resolve_command(fail, &cancel, Duration::from_secs(30)).err(),
            Some("resolver_failed")
        );
        let sleep = shell_command("exec sleep 10", "Start-Sleep -Seconds 10");
        assert!(matches!(
            resolve_command(sleep, &cancel, Duration::from_millis(100)),
            Err("timeout")
        ));
        let sleep = shell_command("exec sleep 10", "Start-Sleep -Seconds 10");
        cancel.store(true, Ordering::Relaxed);
        assert!(matches!(
            resolve_command(sleep, &cancel, Duration::from_secs(10)),
            Err("cancelled")
        ));
    }
    #[test]
    #[ignore = "live YouTube URL preparation and muted playback; requires network and yt-dlp"]
    fn youtube_prepared_stream_plays() {
        use libmpv2::Mpv;
        let stream = resolve_url(
            &std::env::var("MOUSA_TEST_YOUTUBE_URL")
                .unwrap_or_else(|_| "https://www.youtube.com/watch?v=jNQXAC9IVRw".into()),
            &AtomicBool::new(false),
        )
        .expect("URL preparation");
        assert!(stream.fresh());
        let mpv = Mpv::with_initializer(|m| {
            m.set_property("ao", "null")?;
            m.set_property("vo", "null")?;
            m.set_property("vid", "no")?;
            Ok(())
        })
        .unwrap();
        mpv.command(
            "loadfile",
            &[&stream.url, "replace", "-1", &stream.options()],
        )
        .unwrap();
        let start = Instant::now();
        loop {
            if mpv.get_property::<f64>("time-pos").unwrap_or(0.) > 0.1 {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "Prepared stream did not advance"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
