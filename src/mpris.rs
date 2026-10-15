use crate::player::{Command, Player};
use std::collections::HashMap;
use zbus::zvariant::{ObjectPath, OwnedValue};
struct Root;
#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {}
    fn quit(&self) {}
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> &str {
        "Mousa"
    }
    #[zbus(property)]
    fn desktop_entry(&self) -> &str {
        "mousa"
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<&str> {
        vec!["https", "http"]
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<&str> {
        vec![]
    }
}
struct Controls(Player);
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Controls {
    fn open_uri(&self, uri: &str) -> zbus::fdo::Result<()> {
        let u = url::Url::parse(uri).map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        if !matches!(u.scheme(), "https" | "http")
            || !matches!(
                u.host_str(),
                Some("youtube.com" | "www.youtube.com" | "music.youtube.com" | "youtu.be")
            )
        {
            return Err(zbus::fdo::Error::NotSupported(
                "Only YouTube URLs are supported".into(),
            ));
        }
        let uri = uri.to_owned();
        let player = self.0.clone();
        std::thread::spawn(move || match crate::integrations::import_url(&uri) {
            Ok(tracks) => player.send(Command::Queue(tracks, 0)),
            Err(e) => player.state.lock().unwrap().error = format!("{e:#}"),
        });
        Ok(())
    }
    fn next(&self) {
        self.0.send(Command::Next)
    }
    fn previous(&self) {
        self.0.send(Command::Previous)
    }
    fn pause(&self) {
        self.0.send(Command::Pause(true))
    }
    fn play_pause(&self) {
        self.0.send(Command::Toggle)
    }
    fn stop(&self) {
        self.0.send(Command::Stop)
    }
    fn play(&self) {
        self.0.send(Command::Pause(false))
    }
    fn seek(&self, offset: i64) {
        let p = self.0.state.lock().unwrap().position;
        self.0.send(Command::Seek(p + offset as f64 / 1_000_000.));
    }
    fn set_position(&self, track_id: ObjectPath<'_>, position: i64) {
        let s = self.0.state.lock().unwrap();
        if let Some(t) = s.current()
            && track_id.as_str() == format!("/app/mousa/track/{}", t.id)
        {
            self.0.send(Command::Seek(position as f64 / 1_000_000.));
        }
    }
    #[zbus(property)]
    fn playback_status(&self) -> String {
        let s = self.0.state.lock().unwrap();
        if s.queue.is_empty() || s.stopped {
            "Stopped"
        } else if s.paused {
            "Paused"
        } else {
            "Playing"
        }
        .into()
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let s = self.0.state.lock().unwrap();
        let mut m = HashMap::new();
        if let Some(t) = s.current() {
            m.insert(
                "mpris:trackid".into(),
                ObjectPath::try_from(format!("/app/mousa/track/{}", t.id))
                    .unwrap()
                    .into(),
            );
            m.insert(
                "xesam:title".into(),
                zbus::zvariant::Str::from(t.title.as_str()).into(),
            );
            m.insert(
                "xesam:artist".into(),
                OwnedValue::try_from(zbus::zvariant::Value::from(vec![t.artist.clone()])).unwrap(),
            );
            m.insert(
                "mpris:length".into(),
                ((s.duration * 1_000_000.) as i64).into(),
            );
        }
        m
    }
    #[zbus(property)]
    fn position(&self) -> i64 {
        (self.0.state.lock().unwrap().position * 1_000_000.) as i64
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.0.state.lock().unwrap().volume / 100.
    }
    #[zbus(property)]
    fn set_volume(&self, v: f64) {
        self.0.send(Command::Volume(v * 100.));
    }
    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.0.state.lock().unwrap().shuffle
    }
    #[zbus(property)]
    fn set_shuffle(&self, value: bool) {
        self.0.send(Command::SetShuffle(value));
    }
    #[zbus(property)]
    fn loop_status(&self) -> &str {
        if self.0.state.lock().unwrap().repeat {
            "Playlist"
        } else {
            "None"
        }
    }
    #[zbus(property)]
    fn set_loop_status(&self, value: &str) -> zbus::fdo::Result<()> {
        match value {
            "Playlist" => self.0.send(Command::SetRepeat(true)),
            "None" => self.0.send(Command::SetRepeat(false)),
            _ => {
                return Err(zbus::fdo::Error::NotSupported(
                    "Only playlist repeat is supported".into(),
                ));
            }
        };
        Ok(())
    }
    #[zbus(property)]
    fn set_rate(&self, value: f64) -> zbus::fdo::Result<()> {
        if value == 1. {
            Ok(())
        } else {
            Err(zbus::fdo::Error::NotSupported(
                "Only normal speed is supported".into(),
            ))
        }
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.
    }
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        let s = self.0.state.lock().unwrap();
        !s.queue.is_empty() && (s.index + 1 < s.queue.len() || s.repeat || s.shuffle)
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        !self.0.state.lock().unwrap().queue.is_empty()
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        !self.0.state.lock().unwrap().queue.is_empty()
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.0.state.lock().unwrap().duration > 0.
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}
pub fn start(player: Player) {
    std::thread::spawn(move || {
        let result = (|| -> anyhow::Result<()> {
            let conn = zbus::blocking::connection::Builder::session()?
                .name("org.mpris.MediaPlayer2.mousa")?
                .serve_at("/org/mpris/MediaPlayer2", Root)?
                .serve_at("/org/mpris/MediaPlayer2", Controls(player.clone()))?
                .build()?;
            let mut last = String::new();
            let mut last_seek = 0;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let (seek, position) = {
                    let s = player.state.lock().unwrap();
                    (s.seek_serial, (s.position * 1_000_000.) as i64)
                };
                if seek != last_seek {
                    conn.emit_signal(
                        None::<&str>,
                        "/org/mpris/MediaPlayer2",
                        "org.mpris.MediaPlayer2.Player",
                        "Seeked",
                        &(position,),
                    )?;
                    last_seek = seek;
                }
                let control = Controls(player.clone());
                let current = {
                    let s = player.state.lock().unwrap();
                    format!(
                        "{:?}",
                        (
                            s.paused,
                            s.stopped,
                            s.current().map(|t| t.id),
                            s.volume,
                            s.shuffle,
                            s.repeat,
                            s.duration
                        )
                    )
                };
                if current != last {
                    let changed: HashMap<&str, OwnedValue> = HashMap::from([
                        ("Volume", control.volume().into()),
                        ("CanPlay", control.can_play().into()),
                        ("CanGoNext", control.can_go_next().into()),
                        ("CanGoPrevious", control.can_go_previous().into()),
                        ("CanSeek", control.can_seek().into()),
                        ("Shuffle", control.shuffle().into()),
                        (
                            "LoopStatus",
                            zbus::zvariant::Str::from(control.loop_status()).into(),
                        ),
                        (
                            "PlaybackStatus",
                            zbus::zvariant::Str::from(control.playback_status()).into(),
                        ),
                        (
                            "Metadata",
                            OwnedValue::try_from(zbus::zvariant::Value::from(control.metadata()))?,
                        ),
                    ]);
                    conn.emit_signal(
                        None::<&str>,
                        "/org/mpris/MediaPlayer2",
                        "org.freedesktop.DBus.Properties",
                        "PropertiesChanged",
                        &(
                            "org.mpris.MediaPlayer2.Player",
                            changed,
                            Vec::<String>::new(),
                        ),
                    )?;
                    last = current;
                }
            }
        })();
        if let Err(e) = result {
            eprintln!("MPRIS unavailable: {e}");
        }
    });
}
