use gpui::{prelude::*, *};
use gpui_component::slider::{Slider, SliderEvent, SliderState};
use gpui_component::{
    Root,
    input::{Input, InputEvent, InputState},
};
use mousa::{
    db::{self, Playlist, Sort, Track},
    integrations, mpris,
    player::{Command, Player},
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
    [TogglePlayback, NextTrack, PreviousTrack, FocusSearch]
);

const INK: u32 = 0x191919;
const PAPER: u32 = 0xf5f1e8;
const LIME: u32 = 0xd4fb55;
const PINK: u32 = 0xff9ac1;
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
    sort: Sort,
    seek: Entity<SliderState>,
    search: Entity<InputState>,
    playlist_search: Entity<InputState>,
    field: Entity<InputState>,
    secret: Entity<InputState>,
    form: Form,
    queue_view: bool,
    status: String,
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
    paused: bool,
    shuffle: bool,
    repeat: bool,
    playing_id: Option<i64>,
    revision: u64,
    queue: Arc<Vec<Track>>,
    queue_index: usize,
}
fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, color: u32) -> Stateful<Div> {
    div()
        .id(id)
        .px_3()
        .py_2()
        .border_2()
        .border_color(rgb(INK))
        .shadow(vec![BoxShadow {
            color: rgb(INK).into(),
            offset: point(px(3.), px(3.)),
            blur_radius: px(0.),
            spread_radius: px(0.),
        }])
        .bg(rgb(color))
        .text_color(rgb(INK))
        .text_sm()
        .font_weight(FontWeight::BOLD)
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0xffffff)))
        .child(label.into())
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
        let seek = cx.new(|_| SliderState::new().min(0.).max(1.).step(0.001));
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
            sort: Sort::Position,
            seek: seek.clone(),
            search: search.clone(),
            playlist_search: playlist_search.clone(),
            field,
            secret,
            form: Form::None,
            queue_view: false,
            status: "Your music. Your rules.".into(),
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
            paused: true,
            shuffle: false,
            repeat: false,
            playing_id: None,
            revision: u64::MAX,
            queue: Arc::new(vec![]),
            queue_index: 0,
        };
        app.subscriptions
            .push(cx.subscribe(&seek, |this, _, event, _| {
                let SliderEvent::Change(value) = event;
                this.player
                    .send(Command::Seek(value.start() as f64 * this.duration));
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
        let sort = self.sort;
        if let Some(id) = self.selected {
            self.job(move || {
                Ok(Reply::Tracks(
                    generation,
                    db::library()?.tracks(id, &query, sort)?,
                ))
            });
        } else {
            self.tracks = Arc::new(vec![]);
        }
        cx.notify();
    }
    fn tick(&mut self, cx: &mut Context<Self>) {
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
                Reply::Error(e) => self.status = e,
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
            self.status = s.error.clone();
            changed = true;
        }
        drop(s);
        if changed {
            cx.notify();
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
        self.field
            .update(cx, |s, cx| s.set_value(value, window, cx));
        cx.notify();
    }
    fn submit(&mut self, cx: &mut Context<Self>) {
        let text = self.field.read(cx).value().to_string();
        let selected = self.selected;
        let form = self.form;
        let secret = self.secret.read(cx).value().to_string();
        self.form = Form::None;
        self.status = "Working…".into();
        self.job(move || {
            let mut db = db::library()?;
            match form {
                Form::Create => Ok(Reply::Created(db.create(&text)?)),
                Form::Rename => {
                    db.rename(
                        selected.ok_or_else(|| anyhow::anyhow!("Select a playlist"))?,
                        &text,
                    )?;
                    Ok(Reply::Done("Playlist renamed".into()))
                }
                Form::Delete => {
                    db.delete(selected.ok_or_else(|| anyhow::anyhow!("Select a playlist"))?)?;
                    Ok(Reply::Done(
                        "Playlist deleted; the current queue is preserved".into(),
                    ))
                }
                Form::Import => {
                    let id = selected.ok_or_else(|| anyhow::anyhow!("Create a playlist first"))?;
                    let tracks = integrations::import_url(&text)?;
                    let n = tracks.len();
                    db.add(id, &tracks)?;
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
            }
        });
        cx.notify();
    }
    fn play(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Some(playlist) = self.selected {
            let player = self.player.clone();
            let sort = self.sort;
            self.job(move || {
                let tracks = db::library()?.tracks(playlist, "", sort)?;
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
        let count = tracks.len();
        let can_move =
            !queue && self.sort == Sort::Position && self.search.read(cx).value().is_empty();
        let active = self.playing_id;
        uniform_list(
            if queue { "queue-list" } else { "track-list" },
            count,
            cx.processor(move |_this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|i| {
                        let t = &tracks[i];
                        let id = t.id;
                        let title = t.title.clone();
                        let artist = t.artist.clone();
                        let duration = time(t.duration);
                        let track = t.clone();
                        div()
                            .id(("track", i))
                            .h(px(62.))
                            .w_full()
                            .flex()
                            .items_center()
                            .gap_3()
                            .px_4()
                            .border_b_1()
                            .border_color(rgb(INK))
                            .bg(rgb(if Some(id) == active {
                                LIME
                            } else if i % 2 == 0 {
                                PAPER
                            } else {
                                0xfffcf5
                            }))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if queue {
                                    this.player.send(Command::Jump(i));
                                } else {
                                    this.play(id, cx);
                                }
                            }))
                            .child(div().w(px(35.)).text_sm().child(format!("{:02}", i + 1)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div().truncate().font_weight(FontWeight::BOLD).child(title),
                                    )
                                    .child(div().truncate().text_sm().child(artist)),
                            )
                            .child(div().text_sm().child(duration))
                            .when(can_move, |d| {
                                d.child(button(("up", i), "↑", PAPER).on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.job(move || {
                                            db::library()?.move_entry(id, -1)?;
                                            Ok(Reply::Done("Track moved".into()))
                                        });
                                    },
                                )))
                                .child(
                                    button(("down", i), "↓", PAPER).on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.job(move || {
                                                db::library()?.move_entry(id, 1)?;
                                                Ok(Reply::Done("Track moved".into()))
                                            });
                                        },
                                    )),
                                )
                            })
                            .child(
                                button(("enqueue", i), if queue { "×" } else { "+ Queue" }, PAPER)
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
                                d.child(button(("remove", i), "×", PAPER).on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.job(move || {
                                            db::library()?.remove(id)?;
                                            Ok(Reply::Done("Track removed".into()))
                                        });
                                        cx.notify();
                                    },
                                )))
                            })
                    })
                    .collect()
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
        if (self.seek.read(cx).value().start() - progress).abs() > 0.001 {
            self.seek
                .update(cx, |s, cx| s.set_value(progress, window, cx));
        }
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
            .w(px(260.))
            .flex_shrink_0()
            .h_full()
            .border_r_2()
            .border_color(rgb(INK))
            .bg(rgb(LIME))
            .p_5()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .text_3xl()
                    .font_weight(FontWeight::EXTRA_BOLD)
                    .child("MOUSA"),
            )
            .child(div().text_xs().child("LOUD DESIGN. FAST MUSIC."))
            .child(
                button("library", "YOUR LIBRARY", PAPER).on_click(cx.listener(|this, _, _, cx| {
                    this.queue_view = false;
                    cx.notify();
                })),
            )
            .child(
                button(
                    "queue",
                    format!("QUEUE  /  {}", self.queue.len()),
                    if self.queue_view { PINK } else { LIME },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.queue_view = true;
                    cx.notify();
                })),
            )
            .child(Input::new(&self.playlist_search))
            .child(
                button("new", "+ NEW PLAYLIST", PAPER)
                    .on_click(cx.listener(|this, _, w, cx| this.show_form(Form::Create, w, cx))),
            );
        let mut list = div()
            .id("playlists")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_2();
        for p in self
            .playlists
            .iter()
            .filter(|p| p.name.to_lowercase().contains(&filter))
        {
            let id = p.id;
            list = list.child(
                button(
                    ("playlist", id as usize),
                    format!("{}  ·  {}", p.name, p.count),
                    if self.selected == Some(id) && !self.queue_view {
                        PINK
                    } else {
                        LIME
                    },
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
                button("google", "CONNECT GOOGLE ↗", PAPER).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.status = "Complete Google sign-in in your browser…".into();
                        this.job(|| {
                            integrations::google_login()?;
                            Ok(Reply::Done(
                                "Google connected. Import your library below.".into(),
                            ))
                        });
                        cx.notify();
                    },
                )),
            )
            .child(
                button("sync", "IMPORT GOOGLE PLAYLISTS", LIME).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.status = "Importing your YouTube playlists…".into();
                        this.job(|| {
                            let lists = integrations::google_playlists()?;
                            let mut db = db::library()?;
                            for (name, tracks) in lists {
                                let id = db.create(&name)?;
                                db.add(id, &tracks)?;
                            }
                            Ok(Reply::Done(
                                "Google playlists imported as local copies".into(),
                            ))
                        });
                        cx.notify();
                    },
                )),
            )
            .child(
                button("settings", "LAST.FM / SETTINGS", PAPER)
                    .on_click(cx.listener(|this, _, w, cx| this.show_form(Form::Settings, w, cx))),
            );
        let toolbar = div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().flex_1().child(Input::new(&self.search)))
            .child(
                button("sort", self.sort.label(), PAPER).on_click(cx.listener(|this, _, _, cx| {
                    this.sort = this.sort.next();
                    this.refresh_tracks(cx);
                })),
            )
            .child(
                button("import", "+ ADD YOUTUBE", LIME)
                    .on_click(cx.listener(|this, _, w, cx| this.show_form(Form::Import, w, cx))),
            );
        let mut content = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .child("A LITTLE LESS WAITING. A LOT MORE LISTENING."),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .bg(rgb(PINK))
                            .border_2()
                            .border_color(rgb(INK))
                            .text_xs()
                            .child("LOCAL FIRST"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .text_3xl()
                            .font_weight(FontWeight::EXTRA_BOLD)
                            .child(title),
                    )
                    .when(!self.queue_view && self.selected.is_some(), |d| {
                        d.child(button("rename", "RENAME", PAPER).on_click(
                            cx.listener(|this, _, w, cx| this.show_form(Form::Rename, w, cx)),
                        ))
                        .child(button("delete", "DELETE", PINK).on_click(
                            cx.listener(|this, _, w, cx| this.show_form(Form::Delete, w, cx)),
                        ))
                    }),
            )
            .child(
                div()
                    .text_sm()
                    .child(format!("{n} tracks  /  built to keep up")),
            );
        if !self.queue_view {
            content = content.child(toolbar);
        }
        if self.form != Form::None {
            let label = match self.form {
                Form::Create => "NEW PLAYLIST",
                Form::Rename => "RENAME PLAYLIST",
                Form::Import => "ADD A VIDEO OR AN ENTIRE PLAYLIST",
                Form::Delete => "DELETE THIS PLAYLIST? This cannot be undone.",
                Form::Settings => "LAST.FM — API KEY & SECRET",
                Form::None => "",
            };
            let mut form = div()
                .p_4()
                .border_2()
                .border_color(rgb(INK))
                .bg(rgb(PINK))
                .flex()
                .flex_col()
                .gap_3()
                .child(label);
            if self.form != Form::Delete {
                form = form.child(Input::new(&self.field));
            }
            if self.form == Form::Settings {
                form =
                    form.child(Input::new(&self.secret)).child(
                        div()
                            .flex()
                            .gap_2()
                            .child(button("lf-auth", "1. AUTHORIZE ↗", PAPER).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.job(|| {
                                        integrations::lastfm_begin()?;
                                        Ok(Reply::Done(
                                            "Approve Mousa on Last.fm, then click Finish".into(),
                                        ))
                                    });
                                    cx.notify();
                                }),
                            ))
                            .child(button("lf-finish", "2. FINISH", LIME).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.job(|| {
                                        integrations::lastfm_finish()?;
                                        Ok(Reply::Done("Last.fm connected".into()))
                                    });
                                    cx.notify();
                                },
                            ))),
                    );
            }
            form = form.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        button(
                            "save",
                            if self.form == Form::Delete {
                                "DELETE"
                            } else {
                                "SAVE"
                            },
                            LIME,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                    )
                    .child(button("cancel", "CANCEL", PAPER).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.form = Form::None;
                            cx.notify();
                        },
                    ))),
            );
            content = content.child(form);
        }
        content = content.child(
            div()
                .flex_1()
                .min_h_0()
                .border_2()
                .border_color(rgb(INK))
                .when(n == 0, |d| {
                    d.flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_4()
                        .child(div().text_2xl().font_weight(FontWeight::BOLD).child(
                            if self.queue_view {
                                "The queue is yours to fill."
                            } else if !self.search.read(cx).value().is_empty() {
                                "No matching tracks."
                            } else {
                                "Start with something you love."
                            },
                        ))
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
            .gap_2()
            .child(
                button("prev", "|◀", PAPER)
                    .on_click(cx.listener(|this, _, _, _| this.player.send(Command::Previous))),
            )
            .child(
                button("play", if self.paused { "▶ PLAY" } else { "Ⅱ PAUSE" }, LIME)
                    .on_click(cx.listener(|this, _, _, _| this.player.send(Command::Toggle))),
            )
            .child(
                button("next", "▶|", PAPER)
                    .on_click(cx.listener(|this, _, _, _| this.player.send(Command::Next))),
            )
            .child(
                button(
                    "shuffle",
                    "SHUFFLE",
                    if self.shuffle { PINK } else { PAPER },
                )
                .on_click(cx.listener(|this, _, _, _| this.player.send(Command::Shuffle))),
            )
            .child(
                button("repeat", "REPEAT", if self.repeat { PINK } else { PAPER })
                    .on_click(cx.listener(|this, _, _, _| this.player.send(Command::Repeat))),
            );
        let player =
            div()
                .border_t_2()
                .border_color(rgb(INK))
                .bg(rgb(0xffffff))
                .p_4()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .child(
                            div()
                                .w(px(260.))
                                .min_w_0()
                                .child(
                                    div()
                                        .truncate()
                                        .font_weight(FontWeight::BOLD)
                                        .child(self.current_title.clone()),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .text_sm()
                                        .child(self.current_artist.clone()),
                                ),
                        )
                        .child(controls)
                        .child(div().flex_1())
                        .child(button("vol-down", "−", PAPER).on_click(cx.listener(
                            |this, _, _, _| this.player.send(Command::Volume(this.volume - 5.)),
                        )))
                        .child(format!("{}%", self.volume as u32))
                        .child(button("vol-up", "+", PAPER).on_click(cx.listener(
                            |this, _, _, _| this.player.send(Command::Volume(this.volume + 5.)),
                        ))),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .text_sm()
                        .child(time(self.position))
                        .child(button("back10", "−10s", PAPER).on_click(cx.listener(
                            |this, _, _, _| this.player.send(Command::Seek(this.position - 10.)),
                        )))
                        .child(
                            div()
                                .flex_1()
                                .child(Slider::new(&self.seek).disabled(self.duration <= 0.)),
                        )
                        .child(button("forward10", "+10s", PAPER).on_click(cx.listener(
                            |this, _, _, _| this.player.send(Command::Seek(this.position + 10.)),
                        )))
                        .child(time(self.duration)),
                );
        div()
            .key_context("Mousa")
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
            .size_full()
            .bg(rgb(PAPER))
            .text_color(rgb(INK))
            .font_family("DejaVu Sans")
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
            .child(
                div()
                    .px_4()
                    .py_1()
                    .border_t_1()
                    .border_color(rgb(INK))
                    .text_xs()
                    .child(self.status.clone()),
            )
            .child(player)
    }
}
fn main() {
    Application::new().run(|cx| {
        gpui_component::init(cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-space", TogglePlayback, Some("Mousa")),
            KeyBinding::new("ctrl-right", NextTrack, Some("Mousa")),
            KeyBinding::new("ctrl-left", PreviousTrack, Some("Mousa")),
            KeyBinding::new("ctrl-f", FocusSearch, Some("Mousa")),
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
                    size(px(1280.), px(840.)),
                    cx,
                ))),
                window_min_size: Some(size(px(1050.), px(650.))),
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
