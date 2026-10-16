use crate::{
    db::{self, Track},
    integrations,
};
use libmpv2::{Mpv, events::Event};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, Sender},
    },
    time::{Duration, Instant},
};
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub queue: Arc<Vec<Track>>,
    pub index: usize,
    pub paused: bool,
    #[serde(default)]
    pub stopped: bool,
    pub position: f64,
    pub duration: f64,
    pub volume: f64,
    pub shuffle: bool,
    pub repeat: bool,
    pub error: String,
    pub revision: u64,
    #[serde(skip)]
    pub seek_serial: u64,
}
impl Snapshot {
    pub fn current(&self) -> Option<&Track> {
        self.queue.get(self.index)
    }
}
pub enum Command {
    Queue(Vec<Track>, usize),
    Toggle,
    Pause(bool),
    Next,
    Previous,
    Seek(f64),
    Volume(f64),
    Shuffle,
    SetShuffle(bool),
    SetRepeat(bool),
    Repeat,
    Append(Track),
    Remove(usize),
    Jump(usize),
    Stop,
}
#[derive(Clone)]
pub struct Player {
    pub tx: Sender<Command>,
    pub state: Arc<Mutex<Snapshot>>,
}
impl Player {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let mut initial = Snapshot {
            volume: 70.,
            paused: true,
            ..Default::default()
        };
        if let Ok(db) = db::library()
            && let Some(s) = db.load("session")
            && let Ok(saved) = serde_json::from_str::<Snapshot>(&s)
        {
            initial = saved;
            initial.paused = true;
            initial.error.clear();
        }
        std::thread::spawn(|| {
            loop {
                let _ = integrations::flush_scrobbles();
                std::thread::sleep(Duration::from_secs(60));
            }
        });
        let state = Arc::new(Mutex::new(initial));
        let output = state.clone();
        std::thread::spawn(move || {
            let mpv = match Mpv::with_initializer(|m| {
                m.set_property("vo", "null")?;
                m.set_property("vid", "no")?;
                m.set_property("ytdl-format", "bestaudio/best")?;
                m.set_property("idle", "yes")?;
                Ok(())
            }) {
                Ok(m) => m,
                Err(e) => {
                    output.lock().unwrap().error = format!("Playback initialization failed: {e}");
                    return;
                }
            };
            let events = mpv.create_client(None).expect("mpv event client");
            let _ = mpv.set_property("volume", output.lock().unwrap().volume);
            let mut loaded = false;
            let mut ready = false;
            let mut resume_at = 0.;
            let mut started = 0;
            let mut listened = 0.;
            let mut scrobbled = false;
            let mut last = Instant::now();
            let mut persisted = Instant::now();
            let mut history = Vec::new();
            let mut saved_revision = u64::MAX;
            loop {
                let cmd = rx.recv_timeout(Duration::from_millis(200));
                if matches!(cmd, Err(mpsc::RecvTimeoutError::Disconnected)) {
                    break;
                }
                let mut s = output.lock().unwrap();
                let mut load = false;
                if let Ok(cmd) = cmd {
                    s.error.clear();
                    if (integrations::get_secret("google-refresh").is_err()
                        || integrations::get_secret("google-name").is_err())
                        && !matches!(
                            cmd,
                            Command::Stop | Command::Volume(_) | Command::Pause(true)
                        )
                    {
                        continue;
                    }
                    match cmd {
                        Command::Queue(q, i) => {
                            resume_at = 0.;
                            if i < q.len() {
                                s.queue = Arc::new(q);
                                s.index = i;
                                history.clear();
                                load = true;
                            }
                        }
                        Command::Toggle => {
                            if !loaded && !s.queue.is_empty() {
                                resume_at = s.position;
                                load = true;
                            } else {
                                s.paused = !s.paused;
                                let _ = mpv.set_property("pause", s.paused);
                            }
                        }
                        Command::Pause(p) => {
                            s.paused = p;
                            let _ = mpv.set_property("pause", p);
                            if !p && !loaded && !s.queue.is_empty() {
                                resume_at = s.position;
                                load = true;
                            }
                        }
                        Command::Next => {
                            resume_at = 0.;
                            if !s.queue.is_empty() {
                                history.push(s.index);
                                if s.shuffle && s.queue.len() > 1 {
                                    let offset = rand::random_range(1..s.queue.len());
                                    s.index = (s.index + offset) % s.queue.len();
                                    load = true;
                                } else if s.index + 1 < s.queue.len() {
                                    s.index += 1;
                                    load = true;
                                } else if s.repeat {
                                    s.index = 0;
                                    load = true;
                                } else {
                                    s.paused = true;
                                    let _ = mpv.set_property("pause", true);
                                }
                            }
                        }
                        Command::Previous => {
                            if s.position > 3. {
                                let _ = mpv.command("seek", &["0", "absolute"]);
                            } else if !s.queue.is_empty() {
                                s.index = history.pop().unwrap_or(s.index.saturating_sub(1));
                                load = true;
                            }
                        }
                        Command::Seek(p) => {
                            if p.is_finite() {
                                let p = if s.duration > 0. {
                                    p.clamp(0., s.duration)
                                } else {
                                    p.max(0.)
                                };
                                match mpv.command("seek", &[&p.to_string(), "absolute"]) {
                                    Ok(_) => {
                                        s.position = p;
                                        s.seek_serial += 1;
                                    }
                                    Err(e) => s.error = format!("Seek failed: {e}"),
                                }
                            }
                        }
                        Command::Volume(v) => {
                            s.volume = if v.is_finite() {
                                v.clamp(0., 100.)
                            } else {
                                s.volume
                            };
                            let _ = mpv.set_property("volume", s.volume);
                        }
                        Command::SetShuffle(v) => s.shuffle = v,
                        Command::SetRepeat(v) => s.repeat = v,
                        Command::Shuffle => s.shuffle = !s.shuffle,
                        Command::Repeat => s.repeat = !s.repeat,
                        Command::Append(t) => Arc::make_mut(&mut s.queue).push(t),
                        Command::Jump(i) => {
                            resume_at = 0.;
                            if i < s.queue.len() {
                                history.push(s.index);
                                s.index = i;
                                load = true;
                            }
                        }
                        Command::Remove(i) => {
                            history.clear();
                            if i < s.queue.len() {
                                Arc::make_mut(&mut s.queue).remove(i);
                                if i < s.index {
                                    s.index -= 1;
                                } else if i == s.index {
                                    if s.queue.is_empty() {
                                        let _ = mpv.command("stop", &[]);
                                        loaded = false;
                                        s.paused = true;
                                    } else {
                                        s.index = s.index.min(s.queue.len() - 1);
                                        load = true;
                                    }
                                }
                            }
                        }
                        Command::Stop => {
                            s.stopped = true;
                            s.position = 0.;
                            let _ = mpv.command("stop", &[]);
                            s.paused = true;
                            loaded = false;
                        }
                    }
                    s.revision += 1;
                }
                if load && let Some(t) = s.current().cloned() {
                    match mpv.command("loadfile", &[&t.url, "replace"]) {
                        Ok(_) => {
                            loaded = true;
                            ready = false;
                            s.stopped = false;
                            s.paused = false;
                            s.position = 0.;
                            s.duration = t.duration;
                            let _ = mpv.set_property("pause", false);
                            started = integrations::now();
                            listened = 0.;
                            scrobbled = false;
                        }
                        Err(e) => s.error = e.to_string(),
                    }
                }
                while let Some(event) = events.wait_event(0.) {
                    match event {
                        Ok(Event::EndFile(0)) => {
                            loaded = false;
                            if !s.queue.is_empty()
                                && (s.index + 1 < s.queue.len() || s.repeat || s.shuffle)
                            {
                                drop(s);
                                let mut s2 = output.lock().unwrap();
                                history.push(s2.index);
                                s2.index = if s2.shuffle && s2.queue.len() > 1 {
                                    (s2.index + rand::random_range(1..s2.queue.len()))
                                        % s2.queue.len()
                                } else {
                                    (s2.index + 1) % s2.queue.len()
                                };
                                if let Some(t) = s2.current().cloned() {
                                    let _ = mpv.command("loadfile", &[&t.url, "replace"]);
                                    loaded = true;
                                    ready = false;
                                    s2.position = 0.;
                                    s2.duration = t.duration;
                                    s2.paused = false;
                                    started = integrations::now();
                                    listened = 0.;
                                    scrobbled = false;
                                    s2.revision += 1;
                                }
                                drop(s2);
                                s = output.lock().unwrap();
                            } else {
                                s.paused = true;
                            }
                        }
                        Ok(Event::EndFile(4)) => {
                            s.error =
                                "YouTube playback failed. Update yt-dlp or try another track."
                                    .into();
                            s.paused = true;
                            loaded = false;
                        }
                        Ok(Event::FileLoaded) => {
                            ready = true;
                            if resume_at > 0. {
                                let _ = mpv.command("seek", &[&resume_at.to_string(), "absolute"]);
                                resume_at = 0.;
                            }
                            if let Some(t) = s.current().cloned() {
                                std::thread::spawn(move || {
                                    let _ = integrations::lastfm_track(&t, None);
                                });
                            }
                        }
                        _ => {}
                    }
                }
                let elapsed = last.elapsed().as_secs_f64();
                last = Instant::now();
                if loaded {
                    s.position = mpv.get_property::<f64>("time-pos").unwrap_or(s.position);
                    s.duration = mpv.get_property::<f64>("duration").unwrap_or(s.duration);
                    if ready
                        && !s.paused
                        && !mpv
                            .get_property::<bool>("paused-for-cache")
                            .unwrap_or(false)
                    {
                        listened += elapsed;
                    }
                }
                if !scrobbled
                    && s.duration > 30.
                    && listened >= (s.duration / 2.).min(240.)
                    && let Some(t) = s.current().cloned()
                {
                    scrobbled = true;
                    std::thread::spawn(move || {
                        let _ = integrations::queue_scrobble(&t, started);
                        let _ = integrations::flush_scrobbles();
                    });
                }
                if persisted.elapsed() > Duration::from_secs(3) && saved_revision != s.revision {
                    let snapshot = s.clone();
                    drop(s);
                    if let (Ok(json), Ok(db)) = (serde_json::to_string(&snapshot), db::library()) {
                        // A cache refresh must not stall the audio command loop.
                        // If a writer owns the database, retry this revision later.
                        if db.conn.busy_timeout(Duration::ZERO).is_ok()
                            && db.save("session", &json).is_ok()
                        {
                            saved_revision = snapshot.revision;
                        }
                    }
                    persisted = Instant::now();
                }
            }
        });
        Self { tx, state }
    }
    pub fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }
}
