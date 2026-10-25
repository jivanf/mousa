//! Bound startup without treating a deliberate pause as a loading failure.
use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct StartupDeadline {
    deadline: Option<Instant>,
    limit: Duration,
}
impl StartupDeadline {
    pub fn arm(&mut self, now: Instant, prepared: bool) {
        self.limit = Duration::from_secs(if prepared { 15 } else { 45 });
        self.deadline = Some(now + self.limit);
    }
    pub fn cancel(&mut self) {
        self.deadline = None;
    }
    pub fn expired(&mut self, now: Instant, paused: bool, progressed: bool) -> bool {
        if progressed {
            self.cancel();
        }
        let Some(deadline) = self.deadline else {
            return false;
        };
        if paused {
            self.deadline = Some(now + self.limit);
            return false;
        }
        if now < deadline {
            return false;
        }
        self.cancel();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hung_prepared_load_gets_one_retry_then_a_bounded_failure() {
        let now = Instant::now();
        let mut guard = StartupDeadline::default();
        guard.arm(now, true);
        assert!(!guard.expired(now + Duration::from_secs(14), false, false));
        assert!(guard.expired(now + Duration::from_secs(15), false, false));
        assert!(!guard.expired(now + Duration::from_secs(16), false, false));
        guard.arm(now + Duration::from_secs(15), false);
        assert!(!guard.expired(now + Duration::from_secs(59), false, false));
        assert!(guard.expired(now + Duration::from_secs(60), false, false));
    }
    #[test]
    fn pause_progress_and_replacement_do_not_trigger_stale_timeouts() {
        let now = Instant::now();
        let mut guard = StartupDeadline::default();
        guard.arm(now, true);
        assert!(!guard.expired(now + Duration::from_secs(120), true, false));
        assert!(!guard.expired(now + Duration::from_secs(121), false, false));
        assert!(!guard.expired(now + Duration::from_secs(122), false, true));
        assert!(!guard.expired(now + Duration::from_secs(200), false, false));
        guard.arm(now, true);
        guard.arm(now + Duration::from_secs(14), true);
        assert!(!guard.expired(now + Duration::from_secs(16), false, false));
        guard.cancel();
        assert!(!guard.expired(now + Duration::from_secs(60), false, false));
    }
    #[test]
    fn hung_http_load_can_be_replaced_and_playback_resumes() {
        use libmpv2::Mpv;
        use std::{io::Read, net::TcpListener, sync::mpsc};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/hang.wav", listener.local_addr().unwrap());
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(5) {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    let mut request = [0; 1024];
                    let _ = stream.read(&mut request);
                    let _ = accepted_tx.send(());
                    // Hold the response open: no file-loaded event and no network error.
                    let _ = release_rx.recv_timeout(Duration::from_secs(5));
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("retry.wav");
        let mut wav = Vec::from(&b"RIFF"[..]);
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
        std::fs::write(&path, wav).unwrap();
        let mpv = Mpv::with_initializer(|m| {
            m.set_property("ao", "null")?;
            m.set_property("vo", "null")?;
            Ok(())
        })
        .unwrap();
        mpv.command("loadfile", &[&url, "replace"]).unwrap();
        accepted_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let now = Instant::now();
        let mut guard = StartupDeadline::default();
        guard.arm(now, true);
        assert!(guard.expired(now + Duration::from_secs(15), false, false));
        // The production timeout path replaces the hung request with the original source.
        mpv.command("loadfile", &[path.to_str().unwrap(), "replace"])
            .unwrap();
        guard.arm(Instant::now(), false);
        let started = Instant::now();
        loop {
            if mpv.get_property::<f64>("time-pos").unwrap_or(0.) > 0.05 {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "replacement playback failed"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!guard.expired(Instant::now(), false, true));
        let _ = release_tx.send(());
        server.join().unwrap();
    }
}
