//! Bounded, asynchronous diagnostics. Never accept track URLs, titles, or raw errors.
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::mpsc::{self, SyncSender},
    time::Instant,
};
const LOG_LIMIT: u64 = 1024 * 1024;

pub(crate) struct Metrics {
    tx: SyncSender<Value>,
}
impl Metrics {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::sync_channel::<Value>(256);
        std::thread::spawn(move || {
            let path = crate::db::data_dir().join("playback.jsonl");
            for mut value in rx {
                value["timestamp"] = json!(chrono::Utc::now().to_rfc3339());
                value["process_id"] = json!(std::process::id());
                let _ = append(&path, &value, LOG_LIMIT);
            }
        });
        Self { tx }
    }
    #[cfg(test)]
    pub fn silent() -> Self {
        let (tx, _) = mpsc::sync_channel(1);
        Self { tx }
    }
    pub fn emit(&self, value: Value) {
        let _ = self.tx.try_send(value);
    }
}
fn append(path: &Path, value: &Value, limit: u64) -> io::Result<()> {
    let line = serde_json::to_vec(value)?;
    if fs::metadata(path).map(|m| m.len()).unwrap_or(0) + line.len() as u64 + 1 > limit {
        fs::rename(path, path.with_extension("jsonl.1"))?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(&line)?;
    file.write_all(b"\n")
}

pub(crate) struct Playback {
    id: u64,
    start: Instant,
    origin: &'static str,
    preparation: &'static str,
    loaded_ms: Option<u128>,
    restart_ms: Option<u128>,
    progressed: bool,
    last_position: Option<f64>,
    interrupted: bool,
    cache_start: Option<Instant>,
    cache_is_stall: bool,
    startup_cache_ms: u128,
    stall_ms: u128,
    stall_count: u32,
}
impl Playback {
    pub fn new(
        id: u64,
        origin: &'static str,
        preparation: &'static str,
        metrics: &Metrics,
    ) -> Self {
        metrics.emit(json!({"event":"load_started", "playback_id":id,"origin":origin}));
        Self {
            id,
            start: Instant::now(),
            origin,
            preparation,
            loaded_ms: None,
            restart_ms: None,
            progressed: false,
            last_position: None,
            interrupted: false,
            cache_start: None,
            cache_is_stall: false,
            startup_cache_ms: 0,
            stall_ms: 0,
            stall_count: 0,
        }
    }
    pub fn preparation_status(&mut self, status: &'static str) {
        self.preparation = status;
    }
    pub fn loaded(&mut self, metrics: &Metrics) {
        self.loaded_ms = Some(self.start.elapsed().as_millis());
        metrics
            .emit(json!({"event":"file_loaded","playback_id":self.id,"elapsed_ms":self.loaded_ms}));
    }
    pub fn has_progressed(&self) -> bool {
        self.progressed
    }
    pub fn timeout(
        &self,
        file_loaded: bool,
        engine_paused: bool,
        cache_wait: bool,
        metrics: &Metrics,
    ) {
        metrics.emit(json!({"event":"startup_timeout","playback_id":self.id,
            "preparation":self.preparation,"file_loaded":file_loaded,"engine_paused":engine_paused,
            "cache_wait":cache_wait,"elapsed_ms":self.start.elapsed().as_millis()}));
    }
    pub fn restarted(&mut self) {
        if self.restart_ms.is_none() {
            self.restart_ms = Some(self.start.elapsed().as_millis());
        }
    }
    pub fn interrupt(&mut self) {
        self.interrupted = true;
        self.last_position = None;
    }
    pub fn fallback(&mut self, reason: &'static str, metrics: &Metrics) {
        metrics
            .emit(json!({"event":"prepared_url_fallback","playback_id":self.id,"reason":reason}));
        self.preparation = "fallback";
        // A retry after playback began is a recovery, not a clean startup sample.
        if self.progressed {
            self.interrupted = true;
        }
        self.progressed = false;
        self.loaded_ms = None;
        self.restart_ms = None;
        self.last_position = None;
    }
    fn end_cache(&mut self, metrics: &Metrics) {
        if let Some(start) = self.cache_start.take() {
            let elapsed = start.elapsed().as_millis();
            if self.cache_is_stall {
                self.stall_ms += elapsed;
            } else {
                self.startup_cache_ms += elapsed;
            }
            metrics.emit(json!({"event":"buffering_finished","playback_id":self.id,
                "phase":if self.cache_is_stall { "playback" } else { "startup" },"duration_ms":elapsed}));
        }
    }
    pub fn sample(
        &mut self,
        position: Option<f64>,
        paused: bool,
        seeking: bool,
        cache_wait: bool,
        buffered_seconds: Option<f64>,
        metrics: &Metrics,
    ) {
        if cache_wait && !paused && !seeking {
            if self.cache_start.is_none() {
                self.cache_start = Some(Instant::now());
                self.cache_is_stall = self.progressed;
                if self.cache_is_stall {
                    self.stall_count += 1;
                }
            }
        } else {
            self.end_cache(metrics);
        }
        if paused || seeking {
            if paused {
                self.interrupt();
            } else {
                self.last_position = None;
            }
            return;
        }
        if cache_wait {
            return;
        }
        if let Some(position) = position {
            if !self.progressed
                && self.restart_ms.is_some()
                && self
                    .last_position
                    .is_some_and(|last| position > last + 0.001)
            {
                self.progressed = true;
                metrics.emit(json!({"event":"playback_started","playback_id":self.id,
                    "origin":self.origin,"preparation":self.preparation,
                    "first_progress_ms":self.start.elapsed().as_millis(),"file_loaded_ms":self.loaded_ms,
                    "playback_restart_ms":self.restart_ms,"startup_buffering_ms":self.startup_cache_ms,
                    "buffered_seconds":buffered_seconds,"interrupted":self.interrupted}));
            }
            self.last_position = Some(position);
        }
    }
    pub fn finish(mut self, reason: &'static str, metrics: &Metrics) {
        self.end_cache(metrics);
        metrics.emit(json!({"event":"playback_summary","playback_id":self.id,"reason":reason,
            "origin":self.origin,"preparation":self.preparation,"started":self.progressed,
            "stall_count":self.stall_count,"stall_ms":self.stall_ms,"startup_buffering_ms":self.startup_cache_ms}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn capture() -> (Metrics, mpsc::Receiver<Value>) {
        let (tx, rx) = mpsc::sync_channel(50);
        (Metrics { tx }, rx)
    }
    #[test]
    fn load_is_not_playback_and_seeks_do_not_count_as_progress() {
        let (m, rx) = capture();
        let mut p = Playback::new(1, "automatic", "ready", &m);
        p.loaded(&m);
        p.restarted();
        p.sample(Some(0.), false, false, false, Some(5.), &m);
        assert!(!p.progressed);
        p.interrupt(); // Explicit user seek, rather than mpv's initial seek while loading.
        p.sample(Some(40.), false, true, false, Some(5.), &m);
        assert!(!p.progressed);
        p.sample(Some(40.), false, false, false, Some(5.), &m);
        p.sample(Some(40.1), false, false, false, Some(5.), &m);
        assert!(p.progressed);
        let events: Vec<_> = rx.try_iter().collect();
        let started = events
            .iter()
            .find(|e| e["event"] == "playback_started")
            .unwrap();
        assert_eq!(started["interrupted"], true);
    }
    #[test]
    fn startup_cache_and_playback_stalls_are_separate() {
        let (m, rx) = capture();
        let mut p = Playback::new(1, "automatic", "ready", &m);
        p.sample(None, false, false, true, None, &m);
        p.loaded(&m);
        p.restarted();
        p.sample(Some(0.), false, false, false, None, &m);
        p.sample(Some(0.1), false, false, false, None, &m);
        p.sample(Some(0.1), false, false, true, None, &m);
        p.sample(Some(0.1), true, false, true, None, &m);
        p.finish("end", &m);
        let events: Vec<_> = rx.try_iter().collect();
        assert_eq!(events.last().unwrap()["stall_count"], 1);
        assert!(events.iter().any(|v| v["phase"] == "startup"));
        assert!(events.iter().any(|v| v["phase"] == "playback"));
    }
    #[test]
    fn rotation_retains_only_one_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("playback.jsonl");
        for i in 0..20 {
            append(&path, &json!({"n":i}), 30).unwrap();
        }
        assert!(fs::metadata(&path).unwrap().len() <= 30);
        assert!(fs::metadata(path.with_extension("jsonl.1")).unwrap().len() <= 30);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
