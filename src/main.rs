use gpui::{prelude::*, *};
use gpui_component::{
    Root,
    input::{InputEvent, InputState},
};
use mousa::{
    db::{self, Playlist, Sort, Track},
    integrations, mpris,
    player::{Command, Player},
    sync,
};
use mousa::{
    theme::{border, color, font, layout, shadow, size as control_size, space, text},
    ui::{self, Bar, BarChange, button, icon_button, step_button},
};
use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    time::Duration,
};

actions!(
    mousa,
    [
        TogglePlayback,
        NextTrack,
        PreviousTrack,
        FocusSearch,
        SeekBack,
        SeekForward,
        ToggleShuffle,
        ToggleRepeat,
        ToggleMute
    ]
);

enum Reply {
    Library(Vec<Playlist>),
    Tracks(u64, Vec<Track>),
    Done(String),
    Status(String),
    Created(i64),
    Error(String),
}
#[derive(Clone, Copy, PartialEq)]
enum Form {
    None,
    Create,
    Rename,
    Import,
    Delete,
    Settings,
}
struct Mousa {
    player: Player,
    playlists: Vec<Playlist>,
    tracks: Arc<Vec<Track>>,
    selected: Option<i64>,
    seek: Entity<Bar>,
    volume_slider: Entity<Bar>,
    track_list: ListState,
    queue_list: ListState,
    search: Entity<InputState>,
    playlist_search: Entity<InputState>,
    field: Entity<InputState>,
    secret: Entity<InputState>,
    form: Form,
    create_google: bool,
    last_sync: std::time::Instant,
    queue_view: bool,
    status: String,
    error: Option<String>,
    tx: Sender<Reply>,
    rx: Receiver<Reply>,
    generation: u64,
    subscriptions: Vec<Subscription>,
    search_task: Option<Task<()>>,
    focus: FocusHandle,
    current_title: String,
    current_artist: String,
    position: f64,
    duration: f64,
    volume: f64,
    audible_volume: f64,
    paused: bool,
    shuffle: bool,
    repeat: bool,
    playing_id: Option<i64>,
    revision: u64,
    queue: Arc<Vec<Track>>,
    queue_index: usize,
}
fn time(n: f64) -> String {
    let s = n.max(0.) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}
impl Mousa {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let player = Player::start();
        mpris::start(player.clone());
        let (tx, rx) = mpsc::channel();
        let volume_slider =
            cx.new(|cx| Bar::new("Volume · Arrow keys adjust; Home mutes", 0.05, cx));
        let seek = cx.new(|cx| Bar::new("Seek · Arrow keys adjust; Home / End jump", 0.01, cx));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search this playlist…"));
        let playlist_search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Find a playlist…"));
        let field =
            cx.new(|cx| InputState::new(window, cx).placeholder("Playlist name or YouTube URL"));
        let secret = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Last.fm API secret")
                .masked(true)
        });
        let mut app = Self {
            player,
            playlists: vec![],
            tracks: Arc::new(vec![]),
            selected: None,
            seek: seek.clone(),
            volume_slider: volume_slider.clone(),
            track_list: ListState::new(0, ListAlignment::Top, layout::list_overscan()),
            queue_list: ListState::new(0, ListAlignment::Top, layout::list_overscan()),
            search: search.clone(),
            playlist_search: playlist_search.clone(),
            field,
            secret,
            form: Form::None,
            create_google: true,
            last_sync: std::time::Instant::now(),
            queue_view: false,
            status: "Your music. Your rules.".into(),
            error: None,
            tx,
            rx,
            generation: 0,
            subscriptions: vec![],
            search_task: None,
            focus: cx.focus_handle(),
            current_title: "Nothing playing yet".into(),
            current_artist: "Choose a playlist and press play".into(),
            position: 0.,
            duration: 0.,
            volume: 70.,
            audible_volume: 70.,
            paused: true,
            shuffle: false,
            repeat: false,
            playing_id: None,
            revision: u64::MAX,
            queue: Arc::new(vec![]),
            queue_index: 0,
        };
        app.subscriptions
            .push(cx.subscribe(&volume_slider, |this, _, event, _| {
                let BarChange(value) = event;
                this.player.send(Command::Volume(*value as f64 * 100.));
            }));
        app.subscriptions
            .push(cx.subscribe(&seek, |this, _, event, _| {
                let BarChange(value) = event;
                this.player
                    .send(Command::Seek(*value as f64 * this.duration));
            }));
        app.subscriptions
            .push(cx.subscribe(&search, |this, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    this.schedule_search(cx)
                }
            }));
        app.subscriptions
            .push(cx.subscribe(&playlist_search, |_, _, _: &InputEvent, cx| cx.notify()));
        app.refresh_library();
        app.sync_google();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        app
    }
    fn job(&self, f: impl FnOnce() -> anyhow::Result<Reply> + Send + 'static) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = f().unwrap_or_else(|e| Reply::Error(format!("{e:#}")));
            let _ = tx.send(result);
        });
    }
    fn refresh_library(&self) {
        self.job(|| Ok(Reply::Library(db::library()?.playlists()?)));
    }
    fn sync_google(&mut self) {
        self.last_sync = std::time::Instant::now();
        let tx = self.tx.clone();
        self.job(move || {
            sync::refresh(|| {
                let _ = tx.send(Reply::Done("Refreshing YouTube playlists…".into()));
            })?;
            Ok(Reply::Done("Playlists up to date".into()))
        });
    }
    fn schedule_search(&mut self, cx: &mut Context<Self>) {
        self.generation += 1; // Invalidate an in-flight result as soon as input changes.
        self.search_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(80))
                .await;
            let _ = this.update(cx, |this, cx| this.refresh_tracks(cx));
        }));
    }
    fn refresh_tracks(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let query = self.search.read(cx).value().to_string();
        if let Some(id) = self.selected {
            self.job(move || {
                Ok(Reply::Tracks(
                    generation,
                    db::library()?.tracks(id, &query, Sort::Position)?,
                ))
            });
        } else {
            self.track_list.reset(0);
            self.tracks = Arc::new(vec![]);
        }
        cx.notify();
    }
    fn tick(&mut self, cx: &mut Context<Self>) {
        if self.last_sync.elapsed() >= Duration::from_secs(600) {
            self.sync_google();
        }
        let mut changed = false;
        while let Ok(reply) = self.rx.try_recv() {
            changed = true;
            match reply {
                Reply::Library(p) => {
                    self.playlists = p;
                    if self.selected.is_none()
                        || !self.playlists.iter().any(|p| Some(p.id) == self.selected)
                    {
                        self.selected = self.playlists.first().map(|p| p.id);
                        self.refresh_tracks(cx);
                    }
                }
                Reply::Tracks(g, t) => {
                    if g == self.generation {
                        self.track_list.reset(t.len());
                        self.tracks = Arc::new(t);
                    }
                }
                Reply::Done(s) => {
                    self.status = s;
                    self.refresh_library();
                    self.refresh_tracks(cx);
                }
                Reply::Created(id) => {
                    self.selected = Some(id);
                    self.status = "Playlist created".into();
                    self.refresh_library();
                    self.refresh_tracks(cx);
                }
                Reply::Status(s) => self.status = s,
                Reply::Error(e) => {
                    self.error = Some(e.clone());
                    self.status = e;
                    self.refresh_library();
                    self.refresh_tracks(cx);
                }
            }
        }
        let s = self.player.state.lock().unwrap();
        let position = s.position.floor();
        let title = s
            .current()
            .map(|t| t.title.clone())
            .unwrap_or_else(|| "Nothing playing yet".into());
        if self.revision != s.revision {
            self.revision = s.revision;
            if !Arc::ptr_eq(&self.queue, &s.queue) {
                self.queue_list.reset(s.queue.len());
            }
            self.queue = s.queue.clone();
            changed = true;
        }
        if self.position != position
            || self.paused != s.paused
            || self.current_title != title
            || self.duration != s.duration
            || self.volume != s.volume
        {
            changed = true;
        }
        self.current_title = title;
        self.current_artist = s.current().map(|t| t.artist.clone()).unwrap_or_default();
        self.playing_id = s.current().map(|t| t.id);
        self.position = position;
        self.duration = s.duration;
        self.volume = s.volume;
        self.paused = s.paused;
        self.shuffle = s.shuffle;
        self.repeat = s.repeat;
        self.queue_index = s.index;
        if !s.error.is_empty() && self.status != s.error {
            self.error = Some(s.error.clone());
            self.status = s.error.clone();
            changed = true;
        }
        drop(s);
        if changed {
            cx.notify();
        }
    }
    fn toggle_mute(&mut self) {
        if self.volume > 0. {
            self.audible_volume = self.volume;
            self.player.send(Command::Volume(0.));
        } else {
            self.player
                .send(Command::Volume(self.audible_volume.max(1.)));
        }
    }
    fn show_form(&mut self, form: Form, window: &mut Window, cx: &mut Context<Self>) {
        self.form = form;
        let value = if form == Form::Rename {
            self.playlists
                .iter()
                .find(|p| Some(p.id) == self.selected)
                .map(|p| p.name.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
        self.field.update(cx, |s, cx| {
            s.set_placeholder(
                match form {
                    Form::Settings => "Last.fm API key",
                    Form::Import => "YouTube video or playlist URL",
                    _ => "Playlist name",
                },
                window,
                cx,
            );
            s.set_value(value, window, cx);
        });
        cx.notify();
    }
    fn submit(&mut self, cx: &mut Context<Self>) {
        let text = self.field.read(cx).value().to_string();
        let selected = self.selected;
        let form = self.form;
        let create_google = self.create_google;
        let secret = self.secret.read(cx).value().to_string();
        self.form = Form::None;
        self.status = "Working…".into();
        self.job(move || match form {
            Form::Create => Ok(Reply::Created(sync::create(&text, create_google)?)),
            Form::Rename => {
                sync::rename(
                    selected.ok_or_else(|| anyhow::anyhow!("Select a playlist"))?,
                    &text,
                )?;
                Ok(Reply::Done("Playlist renamed".into()))
            }
            Form::Delete => {
                sync::delete(selected.ok_or_else(|| anyhow::anyhow!("Select a playlist"))?)?;
                Ok(Reply::Done(
                    "Playlist deleted; the current queue is preserved".into(),
                ))
            }
            Form::Import => {
                let id = selected.ok_or_else(|| anyhow::anyhow!("Create a playlist first"))?;
                let tracks = integrations::import_url(&text)?;
                let n = tracks.len();
                sync::add(id, &tracks)?;
                Ok(Reply::Done(format!("Added {n} tracks")))
            }
            Form::Settings => {
                anyhow::ensure!(
                    !text.trim().is_empty() && !secret.trim().is_empty(),
                    "Enter both Last.fm credentials"
                );
                integrations::store_secret("lastfm-key", text.trim())?;
                integrations::store_secret("lastfm-secret", secret.trim())?;
                Ok(Reply::Done(
                    "Last.fm credentials saved in the system keyring".into(),
                ))
            }
            _ => Ok(Reply::Done(String::new())),
        });
        cx.notify();
    }
    fn play(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Some(playlist) = self.selected {
            let player = self.player.clone();
            self.job(move || {
                let tracks = db::library()?.tracks(playlist, "", Sort::Position)?;
                let index = tracks
                    .iter()
                    .position(|t| t.id == id)
                    .ok_or_else(|| anyhow::anyhow!("Track was removed"))?;
                player.send(Command::Queue(tracks, index));
                Ok(Reply::Status("Playing from the full playlist".into()))
            });
        }
        cx.notify();
    }
    fn rows(&self, queue: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let tracks = if queue {
            self.queue.clone()
        } else {
            self.tracks.clone()
        };

        let can_move = !queue && self.search.read(cx).value().is_empty();
        let active = self.playing_id;
        let active_queue_index = self.queue_index;
        list(
            if queue {
                self.queue_list.clone()
            } else {
                self.track_list.clone()
            },
            cx.processor(move |_this, i: usize, _, cx| {
                let t = &tracks[i];
                let id = t.id;
                let title = t.title.clone();
                let artist = t.artist.clone();
                let duration = if t.duration.is_finite() && t.duration > 0. {
                    time(t.duration)
                } else {
                    "–:–".into()
                };
                let track = t.clone();
                let is_active = if queue {
                    i == active_queue_index
                } else {
                    Some(id) == active
                };
                ui::track_row(i, is_active)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if queue {
                            this.player.send(Command::Jump(i));
                        } else {
                            this.play(id, cx);
                        }
                    }))
                    .child(
                        div()
                            .w(layout::row_index())
                            .font_family(font::MONO)
                            .text_size(text::secondary())
                            .child(format!(
                                "{:02}",
                                if queue { i + 1 } else { t.playlist_index }
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .whitespace_normal()
                                    .font_weight(FontWeight::BOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(text::secondary())
                                    .text_color(color::muted())
                                    .child(artist),
                            ),
                    )
                    .when(can_move, |d| {
                        d.child(
                            icon_button(("up", i), "arrow-up", "Move up", false, false, true)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.job(move || {
                                        sync::move_entry(id, -1)?;
                                        Ok(Reply::Done("Track moved".into()))
                                    });
                                })),
                        )
                        .child(
                            icon_button(("down", i), "arrow-down", "Move down", false, false, true)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.job(move || {
                                        sync::move_entry(id, 1)?;
                                        Ok(Reply::Done("Track moved".into()))
                                    });
                                })),
                        )
                    })
                    .child(
                        icon_button(
                            ("enqueue", i),
                            if queue { "x" } else { "playlist-add" },
                            if queue {
                                "Remove from queue"
                            } else {
                                "Add to queue"
                            },
                            false,
                            false,
                            true,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if queue {
                                this.player.send(Command::Remove(i));
                            } else {
                                this.player.send(Command::Append(track.clone()));
                            }
                            cx.notify();
                        })),
                    )
                    .when(!queue, |d| {
                        d.child(
                            icon_button(
                                ("remove", i),
                                "x",
                                "Remove from playlist",
                                false,
                                false,
                                true,
                            )
                            .bg(color::pink())
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.job(move || {
                                        sync::remove(id)?;
                                        Ok(Reply::Done("Track removed".into()))
                                    });
                                    cx.notify();
                                },
                            )),
                        )
                    })
                    .child(
                        div()
                            .min_w(layout::time_label())
                            .flex_shrink_0()
                            .text_right()
                            .font_family(font::MONO)
                            .text_size(text::label())
                            .child(duration),
                    )
                    .into_any_element()
            }),
        )
        .h_full()
        .w_full()
    }
}
impl Render for Mousa {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let progress = if self.duration > 0. {
            (self.position / self.duration).clamp(0., 1.) as f32
        } else {
            0.
        };
        self.seek
            .update(cx, |s, cx| s.sync(progress, self.duration <= 0., cx));
        self.volume_slider
            .update(cx, |s, cx| s.sync(self.volume as f32 / 100., false, cx));
        let filter = self.playlist_search.read(cx).value().to_lowercase();
        let title = if self.queue_view {
            "Up next".into()
        } else {
            self.playlists
                .iter()
                .find(|p| Some(p.id) == self.selected)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Make room for music.".into())
        };
        let n = if self.queue_view {
            self.queue.len()
        } else {
            self.tracks.len()
        };
        let mut sidebar = div()
            .w(control_size::sidebar())
            .flex_shrink_0()
            .h_full()
            .border_r(border::strong())
            .border_color(color::ink())
            .bg(color::canvas())
            .p(space::lg())
            .flex()
            .flex_col()
            .gap(space::lg())
            .child(
                button("library", "YOUR LIBRARY", color::surface()).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.queue_view = false;
                        cx.notify();
                    },
                )),
            )
            .child(
                button(
                    "queue",
                    format!("QUEUE  /  {}", self.queue.len()),
                    if self.queue_view {
                        color::ink()
                    } else {
                        color::surface()
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.queue_view = true;
                    cx.notify();
                })),
            )
            .child(ui::input(&self.playlist_search, cx))
            .child(
                button("new", "+ NEW PLAYLIST", color::surface())
                    .on_click(cx.listener(|this, _, w, cx| this.show_form(Form::Create, w, cx))),
            );
        let mut list = div()
            .id("playlists")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(space::sm());
        for p in self
            .playlists
            .iter()
            .filter(|p| p.name.to_lowercase().contains(&filter))
        {
            let id = p.id;
            list = list.child(
                ui::playlist_item(
                    id,
                    p.name.clone(),
                    p.count,
                    if p.google { "YouTube" } else { "Local" },
                    self.selected == Some(id) && !self.queue_view,
                )
                .on_click(cx.listener(move |this, _, w, cx| {
                    this.selected = Some(id);
                    this.queue_view = false;
                    this.search.update(cx, |s, cx| s.set_value("", w, cx));
                    this.refresh_tracks(cx);
                })),
            );
        }
        sidebar = sidebar
            .child(list)
            .child(
                button("google", "CONNECT GOOGLE", color::surface()).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.status = "Complete Google sign-in in your browser…".into();
                        let tx = this.tx.clone();
                        this.job(move || {
                            integrations::google_login()?;
                            sync::refresh(|| {
                                let _ =
                                    tx.send(Reply::Done("Refreshing YouTube playlists…".into()));
                            })?;
                            Ok(Reply::Done(
                                "Google connected. Playlists up to date.".into(),
                            ))
                        });
                        cx.notify();
                    },
                )),
            )
            .child(
                button("sync", "REFRESH PLAYLISTS", color::surface()).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.status = "Refreshing YouTube playlists…".into();
                        this.sync_google();
                        cx.notify();
                    },
                )),
            )
            .child(
                button("settings", "LAST.FM / SETTINGS", color::surface())
                    .on_click(cx.listener(|this, _, w, cx| this.show_form(Form::Settings, w, cx))),
            );
        let toolbar = div()
            .flex()
            .items_center()
            .gap(space::sm())
            .child(div().flex_1().child(ui::input(&self.search, cx)))
            .child(
                button("import", "ADD YOUTUBE", color::surface())
                    .on_click(cx.listener(|this, _, w, cx| this.show_form(Form::Import, w, cx))),
            );
        let mut content = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .p(space::xl())
            .flex()
            .flex_col()
            .gap(space::lg())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::md())
                    .child(
                        div()
                            .flex_1()
                            .text_size(text::page_title())
                            .font_weight(FontWeight::MEDIUM)
                            .child(title),
                    )
                    .when(!self.queue_view && self.selected.is_some(), |d| {
                        d.child(button("rename", "RENAME", color::surface()).on_click(
                            cx.listener(|this, _, w, cx| this.show_form(Form::Rename, w, cx)),
                        ))
                        .child(
                            button("delete", "DELETE", color::pink()).on_click(
                                cx.listener(|this, _, w, cx| this.show_form(Form::Delete, w, cx)),
                            ),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(text::secondary())
                    .child(format!("{n} tracks")),
            );
        if let Some(error) = self.error.clone() {
            content = content.child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::md())
                    .p(space::md())
                    .border(border::strong())
                    .border_color(color::ink())
                    .bg(color::surface())
                    .child(div().flex_1().child(error))
                    .child(
                        button("dismiss-error", "DISMISS", color::surface()).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.error = None;
                                cx.notify();
                            },
                        )),
                    ),
            );
        }
        if !self.queue_view {
            content = content.child(toolbar);
        }
        if self.form != Form::None {
            let label = match self.form {
                Form::Create => "NEW PLAYLIST",
                Form::Rename => "RENAME PLAYLIST",
                Form::Import => "ADD A VIDEO OR AN ENTIRE PLAYLIST",
                Form::Delete => {
                    if self
                        .playlists
                        .iter()
                        .any(|p| Some(p.id) == self.selected && p.google)
                    {
                        "DELETE FROM YOUTUBE? This also deletes the playlist on YouTube and cannot be undone."
                    } else {
                        "DELETE LOCAL PLAYLIST? This cannot be undone."
                    }
                }
                Form::Settings => "LAST.FM — API KEY & SECRET",
                Form::None => "",
            };
            let mut form = div()
                .p(space::lg())
                .border(border::strong())
                .border_color(color::ink())
                .bg(color::surface())
                .flex()
                .flex_col()
                .gap(space::md())
                .child(label);
            if self.form != Form::Delete {
                form = form.child(ui::input(&self.field, cx));
            }
            if self.form == Form::Create {
                form = form.child(
                    button(
                        "playlist-source",
                        if self.create_google {
                            "SAVE TO YOUTUBE (PRIVATE) · Switch to local"
                        } else {
                            "SAVE LOCALLY · Switch to YouTube"
                        },
                        color::surface(),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.create_google = !this.create_google;
                        cx.notify();
                    })),
                );
            }
            if self.form == Form::Settings {
                form = form.child(ui::input(&self.secret, cx)).child(
                    div()
                        .flex()
                        .gap(space::sm())
                        .child(
                            button("lf-auth", "1. AUTHORIZE", color::surface()).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.job(|| {
                                        integrations::lastfm_begin()?;
                                        Ok(Reply::Done(
                                            "Approve Mousa on Last.fm, then click Finish".into(),
                                        ))
                                    });
                                    cx.notify();
                                }),
                            ),
                        )
                        .child(button("lf-finish", "2. FINISH", color::surface()).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.job(|| {
                                    integrations::lastfm_finish()?;
                                    Ok(Reply::Done("Last.fm connected".into()))
                                });
                                cx.notify();
                            }),
                        )),
                );
            }
            form = form.child(
                div()
                    .flex()
                    .gap(space::sm())
                    .child(
                        button(
                            "save",
                            if self.form == Form::Delete {
                                "DELETE"
                            } else {
                                "SAVE"
                            },
                            if self.form == Form::Delete {
                                color::pink()
                            } else {
                                color::surface()
                            },
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                    )
                    .child(
                        button("cancel", "CANCEL", color::surface()).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.form = Form::None;
                                cx.notify();
                            },
                        )),
                    ),
            );
            content = content.child(form);
        }
        content = content.child(
            div()
                .flex_1()
                .min_h_0()
                .border(border::strong())
                .border_color(color::ink())
                .when(n == 0, |d| {
                    d.flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(space::lg())
                        .child(
                            div()
                                .text_size(text::track_title())
                                .font_weight(FontWeight::BOLD)
                                .child(if self.queue_view {
                                    "The queue is yours to fill."
                                } else if !self.search.read(cx).value().is_empty() {
                                    "No matching tracks."
                                } else {
                                    "Start with something you love."
                                }),
                        )
                        .child(if !self.search.read(cx).value().is_empty() {
                            "Try a different title or artist."
                        } else {
                            "Create a playlist, then add a YouTube link."
                        })
                })
                .when(n > 0, |d| d.child(self.rows(self.queue_view, cx))),
        );
        let controls = div()
            .flex()
            .items_center()
            .justify_center()
            .gap(space::md())
            .child(
                icon_button(
                    "shuffle",
                    "arrows-shuffle",
                    "Shuffle (Ctrl+S)",
                    self.shuffle,
                    false,
                    false,
                )
                .on_click(cx.listener(|this, _, _, _| this.player.send(Command::Shuffle))),
            )
            .child(
                icon_button(
                    "prev",
                    "player-skip-back",
                    "Previous track (Ctrl+Left)",
                    false,
                    false,
                    false,
                )
                .on_click(cx.listener(|this, _, _, _| this.player.send(Command::Previous))),
            )
            .child(step_button("back10", "−10").on_click(
                cx.listener(|this, _, _, _| this.player.send(Command::Seek(this.position - 10.))),
            ))
            .child(
                icon_button(
                    "play",
                    if self.paused {
                        "player-play"
                    } else {
                        "player-pause"
                    },
                    "Play / pause (Ctrl+Space)",
                    false,
                    true,
                    false,
                )
                .on_click(cx.listener(|this, _, _, _| this.player.send(Command::Toggle))),
            )
            .child(step_button("forward10", "+10").on_click(
                cx.listener(|this, _, _, _| this.player.send(Command::Seek(this.position + 10.))),
            ))
            .child(
                icon_button(
                    "next",
                    "player-skip-forward",
                    "Next track (Ctrl+Right)",
                    false,
                    false,
                    false,
                )
                .on_click(cx.listener(|this, _, _, _| this.player.send(Command::Next))),
            )
            .child(
                icon_button(
                    "repeat",
                    "repeat",
                    "Repeat playlist (Ctrl+R)",
                    self.repeat,
                    false,
                    false,
                )
                .on_click(cx.listener(|this, _, _, _| this.player.send(Command::Repeat))),
            );
        let info = div()
            .id("now-playing-open-queue")
            .tab_index(0)
            .focus(|s| s.bg(color::canvas()).shadow(vec![shadow::focus_ring()]))
            .cursor_pointer()
            .tooltip(|w, cx| {
                gpui_component::tooltip::Tooltip::new("Open queue at the playing track")
                    .build(w, cx)
            })
            .on_click(cx.listener(|this, _, _, cx| {
                let state = this.player.state.lock().unwrap();
                if !Arc::ptr_eq(&this.queue, &state.queue) {
                    this.queue_list.reset(state.queue.len());
                }
                this.queue = state.queue.clone();
                this.queue_index = state.index;
                drop(state);
                this.queue_view = true;
                this.form = Form::None;
                if !this.queue.is_empty() {
                    this.queue_list.scroll_to(ListOffset {
                        item_ix: this.queue_index,
                        offset_in_item: layout::zero(),
                    });
                }
                cx.notify();
            }))
            .child(ui::caps("NOW PLAYING").mb(space::sm()))
            .child(
                div()
                    .whitespace_normal()
                    .text_size(text::track_title())
                    .font_weight(FontWeight::BOLD)
                    .child(self.current_title.clone()),
            )
            .child(
                div()
                    .whitespace_normal()
                    .text_size(text::body())
                    .text_color(color::muted())
                    .mt(space::sm())
                    .child(self.current_artist.clone()),
            );
        let timeline = div()
            .flex()
            .items_center()
            .gap(space::sm())
            .font_family(font::MONO)
            .font_weight(FontWeight::BOLD)
            .child(div().w(layout::time_label()).child(time(self.position)))
            .child(div().flex_1().child(self.seek.clone()))
            .child(
                div()
                    .w(layout::time_label())
                    .text_right()
                    .child(time(self.duration)),
            );
        let volume = div()
            .w(layout::player_volume())
            .flex()
            .items_center()
            .gap(space::sm())
            .child(
                icon_button(
                    "mute",
                    if self.volume == 0. {
                        "volume-off"
                    } else {
                        "volume"
                    },
                    "Mute / restore volume (Ctrl+M)",
                    self.volume == 0.,
                    false,
                    false,
                )
                .on_click(cx.listener(|this, _, _, _| this.toggle_mute())),
            )
            .child(div().flex_1().min_w_0().child(self.volume_slider.clone()))
            .child(
                div()
                    .w(layout::time_label())
                    .font_family(font::MONO)
                    .font_weight(FontWeight::BOLD)
                    .child(format!("{}%", self.volume as u32)),
            );
        let player = ui::player_panel(
            info,
            controls,
            timeline,
            volume,
            window.viewport_size().width < layout::player_breakpoint(),
        );
        div()
            .key_context("Mousa")
            .tab_group()
            .track_focus(&self.focus)
            .on_action(
                cx.listener(|this, _: &TogglePlayback, _, _| this.player.send(Command::Toggle)),
            )
            .on_action(cx.listener(|this, _: &NextTrack, _, _| this.player.send(Command::Next)))
            .on_action(
                cx.listener(|this, _: &PreviousTrack, _, _| this.player.send(Command::Previous)),
            )
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search.read(cx).focus_handle(cx).focus(window)
            }))
            .on_action(cx.listener(|this, _: &SeekBack, _, _| {
                this.player.send(Command::Seek(this.position - 10.))
            }))
            .on_action(cx.listener(|this, _: &SeekForward, _, _| {
                this.player.send(Command::Seek(this.position + 10.))
            }))
            .on_action(
                cx.listener(|this, _: &ToggleShuffle, _, _| this.player.send(Command::Shuffle)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleRepeat, _, _| this.player.send(Command::Repeat)),
            )
            .on_action(cx.listener(|this, _: &ToggleMute, _, _| this.toggle_mute()))
            .size_full()
            .bg(color::canvas())
            .text_color(color::ink())
            .font_family(font::SANS)
            .text_size(text::body())
            .font_weight(FontWeight::MEDIUM)
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(sidebar)
                    .child(content),
            )
            .child(player)
    }
}
fn main() {
    Application::new().with_assets(ui::Assets).run(|cx| {
        ui::init(cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-space", TogglePlayback, Some("Mousa")),
            KeyBinding::new("ctrl-right", NextTrack, Some("Mousa")),
            KeyBinding::new("ctrl-left", PreviousTrack, Some("Mousa")),
            KeyBinding::new("ctrl-f", FocusSearch, Some("Mousa")),
            KeyBinding::new("ctrl-j", SeekBack, Some("Mousa")),
            KeyBinding::new("ctrl-l", SeekForward, Some("Mousa")),
            KeyBinding::new("ctrl-s", ToggleShuffle, Some("Mousa")),
            KeyBinding::new("ctrl-r", ToggleRepeat, Some("Mousa")),
            KeyBinding::new("ctrl-m", ToggleMute, Some("Mousa")),
        ]);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(layout::viewport_width(), layout::viewport_height()),
                    cx,
                ))),
                window_min_size: Some(size(layout::min_width(), layout::min_height())),
                app_id: Some("mousa".into()),
                titlebar: Some(TitlebarOptions {
                    title: Some("Mousa".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                window.set_window_title("Mousa");
                let view = cx.new(|cx| Mousa::new(window, cx));
                view.read(cx).focus.focus(window);
                let player = view.read(cx).player.clone();
                cx.on_app_quit(move |cx| {
                    let snapshot = player.state.lock().unwrap().clone();
                    cx.background_executor().spawn(async move {
                        if let (Ok(json), Ok(db)) =
                            (serde_json::to_string(&snapshot), db::library())
                        {
                            let _ = db.save("session", &json);
                        }
                    })
                })
                .detach();
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("open Mousa window");
        cx.activate(true);
    });
}
