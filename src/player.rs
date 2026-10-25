use crate::{
    db::{self, Track},
    integrations,
    playback_metrics::{Metrics, Playback},
    playback_start::StartupDeadline,
    stream_prepare::{self, Preparation},
};
use libmpv2::{Mpv, events::Event};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
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
// Keep seeks made before FileLoaded out of mpv: it rejects them while idle/loading.
fn seek_to(
    s: &mut Snapshot,
    ready: bool,
    pending: &mut Option<f64>,
    position: f64,
    seek: impl FnOnce(f64) -> Result<(), String>,
) {
    if !position.is_finite() || s.current().is_none() {
        return;
    }
    let position = if s.duration > 0. {
        position.clamp(0., s.duration)
    } else {
        position.max(0.)
    };
    if ready {
        if let Err(error) = seek(position) {
            s.error = format!("Seek failed: {error}");
            return;
        }
    } else {
        *pending = Some(position);
    }
    s.position = position;
    s.seek_serial += 1;
}

pub enum Command {
    Queue(Vec<Track>, usize),
    Toggle,
    Pause(bool),
    Next,
    Previous,
    Seek(f64),
    SeekAcknowledged(f64, Sender<()>),
    Volume(f64),
    Shuffle,
    SetShuffle(bool),
    SetRepeat(bool),
    Repeat,
    Append(Track),
    Remove(usize),
    Jump(usize),
    Stop,
    Metadata(Vec<(Track, Track)>),
}
/// Holds the requested position until the playback thread has handled that seek.
pub struct PendingSeek {
    position: f64,
    completed: Receiver<()>,
}
impl PendingSeek {
    pub fn position(&self) -> Option<f64> {
        match self.completed.try_recv() {
            Err(mpsc::TryRecvError::Empty) => Some(self.position),
            _ => None,
        }
    }
}

// A seek command returns before mpv has necessarily updated time-pos.
fn can_sample_position(previous_seek: u64, current_seek: u64, seeking: bool) -> bool {
    previous_seek == current_seek && !seeking
}

// Retain the random choice until it is consumed or a relevant command invalidates it.
fn planned_next(s: &Snapshot, preparation: Option<&Preparation>) -> Option<usize> {
    preparation
        .map(|p| p.index)
        .or_else(|| stream_prepare::next_index(s.index, s.queue.len(), s.shuffle, s.repeat))
}
fn invalidates_preparation(cmd: &Command, s: &Snapshot) -> bool {
    match cmd {
        Command::Queue(..)
        | Command::Jump(_)
        | Command::Previous
        | Command::Append(_)
        | Command::Remove(_)
        | Command::Stop
        | Command::Shuffle
        | Command::Repeat => true,
        Command::SetShuffle(v) => *v != s.shuffle,
        Command::SetRepeat(v) => *v != s.repeat,
        _ => false,
    }
}
// Startup preparation belongs to the saved current entry, never to the next-track plan.
fn startup_track(s: &Snapshot, signed_in: bool) -> Option<(usize, Track)> {
    if !signed_in || s.stopped {
        return None;
    }
    s.current().cloned().map(|track| (s.index, track))
}
fn invalidates_startup_preparation(cmd: &Command, s: &Snapshot) -> bool {
    matches!(
        cmd,
        Command::Queue(..) | Command::Jump(_) | Command::Next | Command::Remove(_) | Command::Stop
    ) || matches!(cmd, Command::Previous) && s.position <= 3.
}
fn preparation_for_load(
    resuming: bool,
    startup: &mut Option<Preparation>,
    next: &mut Option<Preparation>,
) -> Option<Preparation> {
    if resuming {
        startup.take()
    } else {
        next.take()
    }
}
fn is_youtube_track(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .is_some_and(|host| {
            host == "youtu.be" || host == "youtube.com" || host.ends_with(".youtube.com")
        })
}

fn load_track(
    mpv: &Mpv,
    track: &Track,
    index: usize,
    preparation: Option<Preparation>,
    metrics: &Metrics,
) -> (Result<(), String>, bool, &'static str) {
    if let Some(reason) = &track.unavailable {
        let _ = mpv.command("stop", &[]);
        return (
            Err(format!("Track unavailable: {reason}")),
            false,
            "unavailable",
        );
    }
    let (stream, status) = preparation
        .map(|p| p.take(index, &track.url, metrics))
        .unwrap_or((None, "not_requested"));
    metrics
        .emit(serde_json::json!({"event":"prepare_consumed","queue_index":index,"outcome":status}));
    if status == "unavailable" {
        let _ = mpv.command("stop", &[]);
        return (
            Err("This track is unavailable on YouTube. Choose another track.".into()),
            false,
            status,
        );
    }
    if let Some(stream) = stream {
        let options = stream.options();
        if mpv
            .command("loadfile", &[&stream.url, "replace", "-1", &options])
            .is_ok()
        {
            return (Ok(()), true, status);
        }
        return (
            mpv.command("loadfile", &[&track.url, "replace"])
                .map_err(|e| e.to_string()),
            false,
            "fallback",
        );
    }
    (
        mpv.command("loadfile", &[&track.url, "replace"])
            .map_err(|e| e.to_string()),
        false,
        status,
    )
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
                // Metadata can advertise streams that the CDN rejects. The fallback
                // must test the selected format and try another before handing it to mpv.
                m.set_property("ytdl-raw-options", "check-formats=")?;
                m.set_property("idle", "yes")?;
                #[cfg(target_os = "windows")]
                m.set_property(
                    "script-opts",
                    format!(
                        "ytdl_hook-ytdl_path={}",
                        integrations::ytdlp_path().display()
                    )
                    .as_str(),
                )?;
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
            let metrics = Metrics::start();
            let mut preparation: Option<Preparation> = None;
            let saved = output.lock().unwrap().clone();
            // Credential checks and resolution happen on the worker, without holding UI state.
            let signed_in = !saved.stopped
                && saved.current().is_some()
                && integrations::get_secret("google-refresh").is_ok()
                && integrations::get_secret("google-name").is_ok();
            let mut startup_preparation = startup_track(&saved, signed_in)
                .map(|(index, track)| Preparation::start(index, &track, "startup", &metrics));
            let mut active_preparation: Option<Preparation> = None;
            let mut measurement: Option<Playback> = None;
            let mut startup_deadline = StartupDeadline::default();
            let mut playback_id = 0;
            let mut using_prepared = false;
            let mut now_playing_sent = false;
            let mut loaded = false;
            let mut ready = false;
            let mut resume_at = None;
            let mut started = 0;
            let mut listened = 0.;
            let mut scrobbled = false;
            let mut last = Instant::now();
            let mut persisted = Instant::now();
            let mut history = Vec::new();
            let mut saved_revision = u64::MAX;
            loop {
                let cmd = rx.recv_timeout(Duration::from_millis(50));
                if matches!(cmd, Err(mpsc::RecvTimeoutError::Disconnected)) {
                    stream_prepare::invalidate(&mut preparation, &metrics, "shutdown");
                    stream_prepare::invalidate(&mut startup_preparation, &metrics, "shutdown");
                    if let Some(m) = measurement.take() {
                        m.finish("shutdown", &metrics);
                    }
                    break;
                }
                if let Some(p) = active_preparation.as_mut() {
                    p.poll(&metrics);
                }
                if let Some(p) = preparation.as_mut() {
                    p.poll(&metrics);
                }
                if let Some(p) = startup_preparation.as_mut() {
                    p.poll(&metrics);
                }
                let mut s = output.lock().unwrap();
                let previous_seek = s.seek_serial;
                let mut load_origin = "manual";
                let mut load = false;
                let mut resuming = false;
                while let Some(event) = events.wait_event(0.) {
                    match event {
                        Ok(Event::EndFile(0)) if loaded => {
                            resume_at = None;
                            loaded = false;
                            ready = false;
                            startup_deadline.cancel();
                            if let Some(m) = measurement.take() {
                                m.finish("end", &metrics);
                            }
                            if let Some(index) = planned_next(&s, preparation.as_ref()) {
                                history.push(s.index);
                                s.index = index;
                                load = true;
                                load_origin = "automatic";
                            } else {
                                stream_prepare::invalidate(
                                    &mut preparation,
                                    &metrics,
                                    "queue_ended",
                                );
                                s.paused = true;
                            }
                        }
                        // libmpv2 returns failed EndFile events as Err, not EndFile(4).
                        // Handle both so a rejected stream retries promptly instead of
                        // leaving the UI playing until the startup watchdog expires.
                        Ok(Event::EndFile(4)) | Err(_) if loaded => {
                            if using_prepared {
                                // Resolve again through mpv once if a prepared URL is rejected/expired.
                                using_prepared = false;
                                ready = false;
                                resume_at = Some(s.position);
                                if let Some(m) = measurement.as_mut() {
                                    m.fallback("playback_error", &metrics);
                                }
                                if let Some(t) = s.current()
                                    && mpv.command("loadfile", &[&t.url, "replace"]).is_ok()
                                {
                                    loaded = true;
                                    startup_deadline.arm(Instant::now(), false);
                                    continue;
                                }
                            }
                            s.error =
                                "This track could not be played from YouTube. It may be unavailable; try another track."
                                    .into();
                            s.paused = true;
                            loaded = false;
                            ready = false;
                            startup_deadline.cancel();
                            if let Some(m) = measurement.take() {
                                m.finish("playback_error", &metrics);
                            }
                        }
                        Ok(Event::PlaybackRestart) if loaded => {
                            if let Some(m) = measurement.as_mut() {
                                m.restarted();
                            }
                        }
                        Ok(Event::FileLoaded) if loaded => {
                            ready = true;
                            if let Some(m) = measurement.as_mut() {
                                m.loaded(&metrics);
                            }
                            if let Some(position) = resume_at.take() {
                                seek_to(&mut s, true, &mut resume_at, position, |position| {
                                    mpv.command("seek", &[&position.to_string(), "absolute"])
                                        .map_err(|e| e.to_string())
                                });
                            }
                            if !now_playing_sent && let Some(t) = s.current().cloned() {
                                now_playing_sent = true;
                                std::thread::spawn(move || {
                                    let _ = integrations::lastfm_track(&t, None);
                                });
                            }
                        }
                        _ => {}
                    }
                }
                if let Ok(cmd) = cmd {
                    // Keep the acknowledgement alive through validation and execution.
                    // Dropping it also releases the preview if the request is rejected.
                    let (cmd, _acknowledgement) = match cmd {
                        Command::SeekAcknowledged(position, ack) => {
                            (Command::Seek(position), Some(ack))
                        }
                        cmd => (cmd, None),
                    };
                    if !matches!(cmd, Command::Metadata(_)) {
                        s.error.clear();
                    }
                    if (integrations::get_secret("google-refresh").is_err()
                        || integrations::get_secret("google-name").is_err())
                        && !matches!(
                            cmd,
                            Command::Stop
                                | Command::Volume(_)
                                | Command::Pause(true)
                                | Command::Metadata(_)
                        )
                    {
                        continue;
                    }
                    if invalidates_startup_preparation(&cmd, &s) {
                        stream_prepare::invalidate(
                            &mut startup_preparation,
                            &metrics,
                            "current_track_changed",
                        );
                    }
                    if invalidates_preparation(&cmd, &s) {
                        stream_prepare::invalidate(
                            &mut preparation,
                            &metrics,
                            "queue_or_mode_changed",
                        );
                    }
                    if matches!(cmd, Command::Seek(_))
                        && let Some(m) = measurement.as_mut()
                    {
                        m.interrupt();
                    }
                    match cmd {
                        Command::SeekAcknowledged(..) => unreachable!(),
                        Command::Metadata(updates) => {
                            crate::queue_metadata::apply(&mut s, updates);
                        }
                        Command::Queue(q, i) => {
                            resume_at = None;
                            if i < q.len() {
                                s.queue = Arc::new(q);
                                s.index = i;
                                history.clear();
                                load = true;
                            }
                        }
                        Command::Toggle => {
                            if !loaded && active_preparation.is_none() && !s.queue.is_empty() {
                                resume_at = Some(s.position);
                                resuming = true;
                                load = true;
                            } else {
                                s.paused = !s.paused;
                                let _ = mpv.set_property("pause", s.paused);
                            }
                        }
                        Command::Pause(p) => {
                            s.paused = p;
                            let _ = mpv.set_property("pause", p);
                            if !p && !loaded && active_preparation.is_none() && !s.queue.is_empty()
                            {
                                resume_at = Some(s.position);
                                resuming = true;
                                load = true;
                            }
                        }
                        Command::Next => {
                            resume_at = None;
                            if !s.queue.is_empty() {
                                history.push(s.index);
                                if let Some(index) = planned_next(&s, preparation.as_ref()) {
                                    s.index = index;
                                    load = true;
                                } else {
                                    s.paused = true;
                                    let _ = mpv.set_property("pause", true);
                                }
                            }
                        }
                        Command::Previous => {
                            if s.position > 3. {
                                seek_to(&mut s, loaded && ready, &mut resume_at, 0., |position| {
                                    mpv.command("seek", &[&position.to_string(), "absolute"])
                                        .map_err(|e| e.to_string())
                                });
                            } else if !s.queue.is_empty() {
                                resume_at = None;
                                s.index = history.pop().unwrap_or(s.index.saturating_sub(1));
                                load = true;
                            }
                        }
                        Command::Seek(p) => {
                            seek_to(&mut s, loaded && ready, &mut resume_at, p, |position| {
                                mpv.command("seek", &[&position.to_string(), "absolute"])
                                    .map_err(|e| e.to_string())
                            });
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
                            resume_at = None;
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
                                    if active_preparation.is_some() {
                                        load = true;
                                    }
                                } else if i == s.index {
                                    resume_at = None;
                                    if s.queue.is_empty() {
                                        active_preparation = None;
                                        let _ = mpv.command("stop", &[]);
                                        loaded = false;
                                        ready = false;
                                        startup_deadline.cancel();
                                        if let Some(m) = measurement.take() {
                                            m.finish("queue_empty", &metrics);
                                        }
                                        s.paused = true;
                                    } else {
                                        s.index = s.index.min(s.queue.len() - 1);
                                        load = true;
                                    }
                                }
                            }
                        }
                        Command::Stop => {
                            load = false;
                            active_preparation = None;
                            resume_at = None;
                            s.stopped = true;
                            s.position = 0.;
                            let _ = mpv.command("stop", &[]);
                            s.paused = true;
                            loaded = false;
                            ready = false;
                            startup_deadline.cancel();
                            if let Some(m) = measurement.take() {
                                m.finish("stop", &metrics);
                            }
                        }
                    }
                    s.revision += 1;
                }
                // Keep an in-flight resolver alive when Play is pressed immediately.
                // Manual loads use the same bounded worker as speculative preparation.
                if load && let Some(t) = s.current().cloned() {
                    active_preparation =
                        preparation_for_load(resuming, &mut startup_preparation, &mut preparation)
                            .filter(|p| p.matches(s.index, &t));
                    if active_preparation.is_none() && is_youtube_track(&t.url) {
                        active_preparation =
                            Some(Preparation::start(s.index, &t, "current", &metrics));
                    }
                    if let Some(m) = measurement.take() {
                        m.finish("replaced", &metrics);
                    }
                    playback_id += 1;
                    measurement = Some(Playback::new(
                        playback_id,
                        if resuming { "resume" } else { load_origin },
                        "pending",
                        &metrics,
                    ));
                    let _ = mpv.command("stop", &[]);
                    loaded = false;
                    ready = false;
                    startup_deadline.cancel();
                    s.stopped = false;
                    s.paused = false;
                    s.position = resume_at.unwrap_or(0.);
                    s.duration = t.duration;
                    listened = 0.;
                    scrobbled = false;
                    s.revision += 1;
                }
                let resolved = active_preparation
                    .as_ref()
                    .is_some_and(Preparation::finished);
                if (resolved || load && active_preparation.is_none())
                    && let Some(t) = s.current().cloned()
                {
                    let (result, prepared, status) =
                        load_track(&mpv, &t, s.index, active_preparation.take(), &metrics);
                    if let Some(timing) = measurement.as_mut() {
                        timing.preparation_status(status);
                    }
                    s.revision += 1;
                    using_prepared = prepared;
                    now_playing_sent = false;
                    match result {
                        Ok(_) => {
                            loaded = true;
                            ready = false;
                            startup_deadline.arm(Instant::now(), prepared);
                            s.stopped = false;
                            s.position = resume_at.unwrap_or(0.);
                            s.duration = t.duration;
                            let _ = mpv.set_property("pause", s.paused);
                            started = integrations::now();
                            listened = 0.;
                            scrobbled = false;
                        }
                        Err(e) => {
                            s.error = e.to_string();
                            loaded = false;
                            ready = false;
                            s.paused = true;
                            if let Some(m) = measurement.take() {
                                m.finish("load_error", &metrics);
                            }
                        }
                    }
                }
                let elapsed = last.elapsed().as_secs_f64();
                last = Instant::now();
                if loaded && ready {
                    if can_sample_position(
                        previous_seek,
                        s.seek_serial,
                        mpv.get_property::<bool>("seeking").unwrap_or(true),
                    ) {
                        s.position = mpv.get_property::<f64>("time-pos").unwrap_or(s.position);
                    }
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
                if loaded {
                    if let Some(m) = measurement.as_mut() {
                        m.sample(
                            if ready {
                                mpv.get_property::<f64>("time-pos").ok()
                            } else {
                                None
                            },
                            s.paused,
                            mpv.get_property::<bool>("seeking").unwrap_or(false),
                            mpv.get_property::<bool>("paused-for-cache")
                                .unwrap_or(false),
                            mpv.get_property::<f64>("demuxer-cache-duration").ok(),
                            &metrics,
                        );
                    }
                    if startup_deadline.expired(
                        Instant::now(),
                        s.paused,
                        measurement.as_ref().is_some_and(|m| m.has_progressed()),
                    ) {
                        if let Some(m) = measurement.as_ref() {
                            m.timeout(
                                ready,
                                mpv.get_property::<bool>("pause").unwrap_or(false),
                                mpv.get_property::<bool>("paused-for-cache")
                                    .unwrap_or(false),
                                &metrics,
                            );
                        }
                        if using_prepared {
                            using_prepared = false;
                            ready = false;
                            resume_at = Some(s.position);
                            if let Some(m) = measurement.as_mut() {
                                m.fallback("startup_timeout", &metrics);
                            }
                            if let Some(t) = s.current()
                                && mpv.command("loadfile", &[&t.url, "replace"]).is_ok()
                            {
                                let _ = mpv.set_property("pause", s.paused);
                                startup_deadline.arm(Instant::now(), false);
                            } else {
                                loaded = false;
                            }
                        } else {
                            loaded = false;
                        }
                        if !loaded {
                            let _ = mpv.command("stop", &[]);
                            ready = false;
                            s.paused = true;
                            s.error = "Playback could not start. Press Play to retry or choose another track.".into();
                            stream_prepare::invalidate(
                                &mut preparation,
                                &metrics,
                                "startup_timeout",
                            );
                            if let Some(m) = measurement.take() {
                                m.finish("startup_timeout", &metrics);
                            }
                        }
                    }
                    if ready
                        && stream_prepare::due(s.position, s.duration, s.paused)
                        && (preparation.is_none()
                            || preparation.as_ref().is_some_and(|p| p.needs_refresh()))
                        && let Some(index) = planned_next(&s, preparation.as_ref())
                        && let Some(track) = s.queue.get(index)
                    {
                        stream_prepare::invalidate(&mut preparation, &metrics, "expired");
                        preparation = Some(Preparation::start(index, track, "next", &metrics));
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
        let player = Self { tx, state };
        player.refresh_metadata();
        player
    }
    /// Resolve cached metadata off the UI and playback threads.
    pub fn refresh_metadata(&self) {
        let queue = self.state.lock().unwrap().queue.clone();
        if queue.is_empty() {
            return;
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            if let Ok(updates) = crate::queue_metadata::read(&queue)
                && !updates.is_empty()
            {
                let _ = tx.send(Command::Metadata(updates));
            }
        });
    }
    pub fn seek_with_preview(&self, position: f64) -> PendingSeek {
        let (ack, completed) = mpsc::channel();
        self.send(Command::SeekAcknowledged(position, ack));
        PendingSeek {
            position,
            completed,
        }
    }
    pub fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        Snapshot {
            queue: Arc::new(vec![Track {
                id: 1,
                playlist_index: 1,
                unavailable: None,
                url: "test.wav".into(),
                title: "Test".into(),
                artist: "Test".into(),
                duration: 120.,
            }]),
            paused: true,
            duration: 120.,
            ..Default::default()
        }
    }

    #[test]
    fn preview_waits_for_its_own_seek_and_releases_on_rejection() {
        let (tx, rx) = mpsc::channel();
        let player = Player {
            tx,
            state: Arc::new(Mutex::new(snapshot())),
        };
        let first = player.seek_with_preview(45.);
        let latest = player.seek_with_preview(80.);
        assert_eq!(latest.position(), Some(80.));
        // An earlier request finishing must not release a newer preview.
        drop(rx.recv().unwrap());
        assert_eq!(first.position(), None);
        assert_eq!(latest.position(), Some(80.));
        // Rejected requests drop their acknowledgement too, restoring real state.
        drop(rx.recv().unwrap());
        assert_eq!(latest.position(), None);
        drop(rx);
        assert_eq!(player.seek_with_preview(20.).position(), None);
    }

    #[test]
    fn stale_playback_samples_cannot_overwrite_a_seek() {
        assert!(!can_sample_position(0, 1, false));
        assert!(!can_sample_position(1, 1, true));
        assert!(can_sample_position(1, 1, false));
        assert!(!can_sample_position(1, 2, true));
    }

    #[test]
    fn seeks_before_loading_are_deferred_without_starting_playback() {
        let mut s = snapshot();
        let mut pending = None;
        for position in [45., 80., 0.] {
            seek_to(&mut s, false, &mut pending, position, |_| {
                panic!("must not seek an idle or loading player")
            });
            assert_eq!(pending, Some(position));
            assert_eq!(s.position, position);
            assert!(s.paused);
            assert!(s.error.is_empty());
        }
        let position = pending.take().unwrap();
        seek_to(&mut s, true, &mut pending, position, |target| {
            assert_eq!(target, 0.);
            Ok(())
        });
        assert!(pending.is_none());
    }

    #[test]
    fn ready_seeks_clamp_positions_and_preserve_position_on_failure() {
        let mut s = snapshot();
        let mut pending = None;
        seek_to(&mut s, true, &mut pending, 200., |target| {
            assert_eq!(target, 120.);
            Ok(())
        });
        assert_eq!(s.position, 120.);
        assert_eq!(s.seek_serial, 1);
        seek_to(&mut s, true, &mut pending, 20., |_| {
            Err("unavailable".into())
        });
        assert_eq!(s.position, 120.);
        assert_eq!(s.seek_serial, 1);
        assert_eq!(s.error, "Seek failed: unavailable");
        seek_to(&mut s, true, &mut pending, f64::NAN, |_| {
            panic!("invalid seek")
        });
        s.queue = Arc::new(vec![]);
        seek_to(&mut s, false, &mut pending, 10., |_| panic!("empty queue"));
        assert!(pending.is_none());
    }
    #[test]
    fn shuffle_plan_is_reused_and_queue_edits_invalidate_it() {
        let mut s = snapshot();
        s.queue = Arc::new(vec![s.queue[0].clone(); 4]);
        s.shuffle = true;
        let p = Preparation::pending(3, &s.queue[3].url);
        for _ in 0..100 {
            assert_eq!(planned_next(&s, Some(&p)), Some(3));
        }
        for cmd in [
            Command::Queue(vec![], 0),
            Command::Jump(2),
            Command::Previous,
            Command::Append(s.queue[0].clone()),
            Command::Remove(1),
            Command::Stop,
            Command::Shuffle,
            Command::Repeat,
            Command::SetShuffle(false),
            Command::SetRepeat(true),
        ] {
            assert!(invalidates_preparation(&cmd, &s));
        }
        for cmd in [
            Command::Next,
            Command::Seek(2.),
            Command::Pause(true),
            Command::Toggle,
            Command::Volume(20.),
            Command::SetShuffle(true),
            Command::SetRepeat(false),
            Command::Metadata(vec![]),
        ] {
            assert!(!invalidates_preparation(&cmd, &s));
        }
    }
    #[test]
    fn startup_prepares_only_a_signed_in_nonstopped_current_entry() {
        let mut s = snapshot();
        s.position = 42.;
        let (index, track) = startup_track(&s, true).unwrap();
        assert_eq!(index, s.index);
        assert_eq!(track, s.current().unwrap().clone());
        assert!(s.paused);
        assert_eq!(s.position, 42.);
        assert!(startup_track(&s, false).is_none());
        s.stopped = true;
        assert!(startup_track(&s, true).is_none());
        s.stopped = false;
        s.index = 5;
        assert!(startup_track(&s, true).is_none());
        s.queue = Arc::new(vec![]);
        assert!(startup_track(&s, true).is_none());
    }
    #[test]
    fn startup_preparation_survives_controls_that_leave_current_track_selected() {
        let mut s = snapshot();
        s.position = 42.;
        for cmd in [
            Command::Toggle,
            Command::Pause(false),
            Command::Pause(true),
            Command::Volume(10.),
            Command::Seek(10.),
            Command::Previous,
            Command::Shuffle,
            Command::Repeat,
            Command::Append(s.queue[0].clone()),
            Command::Metadata(vec![]),
        ] {
            assert!(!invalidates_startup_preparation(&cmd, &s));
        }
        for cmd in [
            Command::Next,
            Command::Queue(vec![], 0),
            Command::Jump(0),
            Command::Remove(0),
            Command::Stop,
        ] {
            assert!(invalidates_startup_preparation(&cmd, &s));
        }
        s.position = 0.;
        assert!(invalidates_startup_preparation(&Command::Previous, &s));
    }
    #[test]
    fn startup_preparation_is_consumed_only_by_resume_never_next_or_shuffle() {
        let s = snapshot();
        let mut startup = Some(Preparation::pending(0, "current"));
        let mut next = Some(Preparation::pending(1, "next"));
        assert_eq!(
            preparation_for_load(false, &mut startup, &mut next)
                .unwrap()
                .index,
            1
        );
        assert!(startup.is_some());
        assert!(next.is_none());
        // No next plan: ordinary Next still follows the queue, never the startup index.
        assert_eq!(planned_next(&s, next.as_ref()), None);
        assert_eq!(
            preparation_for_load(true, &mut startup, &mut next)
                .unwrap()
                .index,
            0
        );
        assert!(startup.is_none());
        assert!(preparation_for_load(true, &mut startup, &mut next).is_none());
    }
}
