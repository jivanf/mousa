//! Native Windows media-key and system playback-panel integration.
use crate::player::{Command, Player, Snapshot};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::time::Duration;
use windows::{
    Foundation::{TimeSpan, TypedEventHandler},
    Media::{
        MediaPlaybackStatus, MediaPlaybackType, PlaybackPositionChangeRequestedEventArgs,
        SystemMediaTransportControls, SystemMediaTransportControlsButton,
        SystemMediaTransportControlsButtonPressedEventArgs,
        SystemMediaTransportControlsTimelineProperties,
    },
    Win32::{
        Foundation::HWND,
        System::WinRT::{
            ISystemMediaTransportControlsInterop, RO_INIT_MULTITHREADED, RoInitialize,
            RoUninitialize,
        },
        UI::WindowsAndMessaging::IsWindow,
    },
};

pub fn start(window: &gpui::Window, player: Player) {
    let handle = HasWindowHandle::window_handle(window)
        .ok()
        .and_then(|h| match h.as_raw() {
            RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
            _ => None,
        });
    let Some(handle) = handle else {
        player.state.lock().unwrap().error =
            "Could not attach Windows media controls to the window".into();
        return;
    };
    std::thread::spawn(move || {
        let result = (|| -> windows::core::Result<()> {
            unsafe {
                RoInitialize(RO_INIT_MULTITHREADED)?;
            }
            // Keep the apartment alive until all event handlers and COM objects are released.
            struct Apartment;
            impl Drop for Apartment {
                fn drop(&mut self) {
                    unsafe {
                        RoUninitialize();
                    }
                }
            }
            let _apartment = Apartment;
            let hwnd = HWND(handle as *mut _);
            let controls = Controls::new(hwnd, &player)?;
            let mut metadata = None;
            while unsafe { IsWindow(Some(hwnd)) }.as_bool() {
                let snapshot = player.state.lock().unwrap().clone();
                controls.update(&snapshot, &mut metadata)?;
                std::thread::sleep(Duration::from_millis(250));
            }
            Ok(())
        })();
        if let Err(error) = result {
            player.state.lock().unwrap().error = format!("Windows media controls failed: {error}");
        }
    });
}

fn command(button: SystemMediaTransportControlsButton) -> Option<Command> {
    match button {
        SystemMediaTransportControlsButton::Play => Some(Command::Pause(false)),
        SystemMediaTransportControlsButton::Pause => Some(Command::Pause(true)),
        SystemMediaTransportControlsButton::Stop => Some(Command::Stop),
        SystemMediaTransportControlsButton::Next => Some(Command::Next),
        SystemMediaTransportControlsButton::Previous => Some(Command::Previous),
        _ => None,
    }
}

fn status(snapshot: &Snapshot) -> MediaPlaybackStatus {
    if snapshot.current().is_none() || snapshot.stopped {
        MediaPlaybackStatus::Stopped
    } else if snapshot.paused {
        MediaPlaybackStatus::Paused
    } else {
        MediaPlaybackStatus::Playing
    }
}

struct Controls {
    smtc: SystemMediaTransportControls,
    button_token: i64,
    seek_token: Option<i64>,
}

impl Controls {
    fn new(hwnd: HWND, player: &Player) -> windows::core::Result<Self> {
        let interop = windows::core::factory::<
            SystemMediaTransportControls,
            ISystemMediaTransportControlsInterop,
        >()?;
        let smtc: SystemMediaTransportControls = unsafe { interop.GetForWindow(hwnd)? };
        let button_player = player.clone();
        let button_token = smtc.ButtonPressed(&TypedEventHandler::<
            SystemMediaTransportControls,
            SystemMediaTransportControlsButtonPressedEventArgs,
        >::new(move |_, args| {
            if let Some(command) = command(
                args.as_ref()
                    .ok_or_else(windows::core::Error::empty)?
                    .Button()?,
            ) {
                button_player.send(command);
            }
            Ok(())
        }))?;
        let mut controls = Self {
            smtc,
            button_token,
            seek_token: None,
        };
        let seek_player = player.clone();
        controls.seek_token = Some(controls.smtc.PlaybackPositionChangeRequested(
            &TypedEventHandler::<
                SystemMediaTransportControls,
                PlaybackPositionChangeRequestedEventArgs,
            >::new(move |_, args| {
                let position = args
                    .as_ref()
                    .ok_or_else(windows::core::Error::empty)?
                    .RequestedPlaybackPosition()?;
                seek_player.send(Command::Seek(position.Duration as f64 / 10_000_000.));
                Ok(())
            }),
        )?);
        controls.smtc.SetIsPlayEnabled(true)?;
        controls.smtc.SetIsPauseEnabled(true)?;
        controls.smtc.SetIsStopEnabled(true)?;
        controls.smtc.SetIsNextEnabled(true)?;
        controls.smtc.SetIsPreviousEnabled(true)?;
        controls.smtc.SetIsEnabled(true)?;
        Ok(controls)
    }

    fn update(
        &self,
        snapshot: &Snapshot,
        previous: &mut Option<(String, String)>,
    ) -> windows::core::Result<()> {
        self.smtc.SetPlaybackStatus(status(snapshot))?;
        let metadata = snapshot
            .current()
            .map(|t| (t.title.clone(), t.artist.clone()));
        if metadata != *previous {
            let display = self.smtc.DisplayUpdater()?;
            display.ClearAll()?;
            display.SetType(MediaPlaybackType::Music)?;
            if let Some((title, artist)) = &metadata {
                let music = display.MusicProperties()?;
                music.SetTitle(&title.into())?;
                music.SetArtist(&artist.into())?;
            }
            display.Update()?;
            *previous = metadata;
        }
        let timeline = SystemMediaTransportControlsTimelineProperties::new()?;
        let duration = snapshot.duration.max(0.);
        let span = |seconds: f64| TimeSpan {
            Duration: (seconds * 10_000_000.) as i64,
        };
        timeline.SetStartTime(span(0.))?;
        timeline.SetMinSeekTime(span(0.))?;
        timeline.SetEndTime(span(duration))?;
        timeline.SetMaxSeekTime(span(duration))?;
        timeline.SetPosition(span(snapshot.position.clamp(0., duration)))?;
        self.smtc.UpdateTimelineProperties(&timeline)
    }
}

impl Drop for Controls {
    fn drop(&mut self) {
        let _ = self.smtc.SetIsEnabled(false);
        let _ = self.smtc.RemoveButtonPressed(self.button_token);
        if let Some(token) = self.seek_token {
            let _ = self.smtc.RemovePlaybackPositionChangeRequested(token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex, mpsc};
    use windows::{
        Media::Control::{
            GlobalSystemMediaTransportControlsSessionManager,
            GlobalSystemMediaTransportControlsSessionPlaybackStatus,
        },
        Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
        },
        core::w,
    };

    // Exercises the real Windows session broker and its callbacks without playing
    // audio, modifying the user's library, or sending keys to another application.
    #[test]
    #[ignore = "requires an interactive Windows desktop and the system media session broker"]
    fn native_session_routes_commands_and_publishes_state() -> windows::core::Result<()> {
        unsafe {
            RoInitialize(RO_INIT_MULTITHREADED)?;
        }
        struct TestWindow(HWND);
        impl Drop for TestWindow {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.0);
                    RoUninitialize();
                }
            }
        }
        let window = TestWindow(unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("Mousa media integration test"),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                100,
                100,
                None,
                None,
                None,
                None,
            )?
        });
        let (tx, rx) = mpsc::channel();
        let title = format!("Mousa media test {}", std::process::id());
        let mut snapshot = Snapshot {
            paused: true,
            duration: 120.,
            position: 5.,
            ..Default::default()
        };
        snapshot.queue = Arc::new(vec![crate::db::Track {
            id: 1,
            playlist_index: 1,
            url: String::new(),
            title: title.clone(),
            artist: "Test artist".into(),
            duration: 120.,
            unavailable: None,
        }]);
        let player = Player {
            tx,
            state: Arc::new(Mutex::new(snapshot.clone())),
        };
        let controls = Controls::new(window.0, &player)?;
        let mut metadata = None;
        controls.update(&snapshot, &mut metadata)?;
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.get()?;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let session = loop {
            let mut found = None;
            for session in manager.GetSessions()? {
                if session
                    .TryGetMediaPropertiesAsync()?
                    .get()?
                    .Title()?
                    .to_string()
                    == title
                {
                    found = Some(session);
                    break;
                }
            }
            if let Some(session) = found {
                break session;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "test media session was not published"
            );
            std::thread::sleep(Duration::from_millis(100));
        };
        assert_eq!(
            session.GetPlaybackInfo()?.PlaybackStatus()?,
            GlobalSystemMediaTransportControlsSessionPlaybackStatus::Paused
        );
        assert_eq!(
            session
                .TryGetMediaPropertiesAsync()?
                .get()?
                .Artist()?
                .to_string(),
            "Test artist"
        );
        assert!(session.TryPlayAsync()?.get()?);
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(3)).unwrap(),
            Command::Pause(false)
        ));
        snapshot.paused = false;
        controls.update(&snapshot, &mut metadata)?;
        assert!(session.TryPauseAsync()?.get()?);
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(3)).unwrap(),
            Command::Pause(true)
        ));
        assert!(session.TrySkipNextAsync()?.get()?);
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(3)).unwrap(),
            Command::Next
        ));
        assert!(session.TrySkipPreviousAsync()?.get()?);
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(3)).unwrap(),
            Command::Previous
        ));
        assert!(session.TryChangePlaybackPositionAsync(300_000_000)?.get()?);
        assert!(
            matches!(rx.recv_timeout(Duration::from_secs(3)).unwrap(), Command::Seek(p) if p == 30.)
        );
        assert!(session.TryStopAsync()?.get()?);
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(3)).unwrap(),
            Command::Stop
        ));
        snapshot.stopped = true;
        assert_eq!(status(&snapshot), MediaPlaybackStatus::Stopped);
        drop(controls);
        Ok(())
    }
}
