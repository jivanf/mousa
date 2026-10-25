#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use gpui::{prelude::*, *};
use gpui_component::{
    Root,
    input::{InputEvent, InputState},
};
use mousa::{
    db::{self, Playlist, Sort, Track},
    integrations,
    player::{Command, PendingSeek, Player},
    sync,
};
use mousa::{
    theme::{border, color, font, layout, reorder, shadow, size as control_size, space, text},
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
    Reordered(Result<(), String>),
    Library(Vec<Playlist>),
    Tracks(u64, Vec<Track>, String),
    OpenQueueTrack(u64, i64, usize),
    Done(String),
    Status(String),
    Created(i64),
    Error(String),
    Account(bool, Result<Option<String>, String>),
}
#[derive(Clone, Copy, PartialEq)]
enum Form {
    None,
    Create,
    Rename,
    Import,
    Delete,
}
#[derive(Clone)]
struct TrackDrag {
    index: usize,
    ids: Vec<i64>,
    collapse_on_release: bool,
    pointer: Point<Pixels>,
    origin: Point<Pixels>,
    active: bool,
    offset: Pixels,
    generation: u64,
}
struct Mousa {
    track_drag: Option<TrackDrag>,
    track_selection: mousa::track_selection::TrackSelection,
    selection_playlist: Option<i64>,
    reorder_focus: Option<(i64, FocusHandle)>,
    reorder_reveal: Option<i64>,
    reorder_busy: bool,

    view_state: mousa::view_state::ViewState,
    sidebar_scroll: ScrollHandle,
    tracks_ready: bool,
    library_ready: bool,
    queue_ready: bool,
    player: Player,
    playlists: Vec<Playlist>,
    tracks: Arc<Vec<Track>>,
    selected: Option<i64>,
    unavailable_selection: Option<(bool, i64)>,
    seek: Entity<Bar>,
    volume_slider: Entity<Bar>,
    track_scrollbar: Entity<ui::TrackScrollbar>,
    queue_scrollbar: Entity<ui::TrackScrollbar>,
    track_list: ListState,
    queue_list: ListState,
    search: Entity<InputState>,
    playlist_search: Entity<InputState>,
    field: Entity<InputState>,
    google_name: Option<String>,
    lastfm_name: Option<String>,
    account_busy: bool,
    form: Form,
    create_google: bool,
    last_sync: std::time::Instant,
    freshness: String,
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
    pending_seek: Option<PendingSeek>,
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
    fn new(
        saved: mousa::view_state::ViewState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let player = Player::start();
        #[cfg(target_os = "linux")]
        mousa::mpris::start(player.clone());
        let (tx, rx) = mpsc::channel();
        let volume_slider =
            cx.new(|cx| Bar::new("Volume · Arrow keys adjust; Home mutes", 0.05, cx));
        let seek = cx.new(|cx| Bar::new("Seek · Arrow keys adjust; Home / End jump", 0.01, cx));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search this playlist…"));
        let playlist_search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Find a playlist…"));
        let field =
            cx.new(|cx| InputState::new(window, cx).placeholder("Playlist name or YouTube URL"));
        search.update(cx, |s, cx| {
            s.set_value(
                saved
                    .playlist
                    .and_then(|id| saved.playlists.get(&id))
                    .map(|p| p.query.clone())
                    .unwrap_or_default(),
                window,
                cx,
            )
        });
        playlist_search.update(cx, |s, cx| {
            s.set_value(saved.library_query.clone(), window, cx)
        });
        let sidebar_scroll = ScrollHandle::new();
        sidebar_scroll.set_offset(point(px(0.), px(saved.library_scroll.min(0.))));
        let track_list = ListState::new(0, ListAlignment::Top, layout::list_overscan());
        let queue_list = ListState::new(0, ListAlignment::Top, layout::list_overscan());
        let mut app = Self {
            track_scrollbar: cx.new(|cx| ui::TrackScrollbar::new(track_list.clone(), cx)),
            queue_scrollbar: cx.new(|cx| ui::TrackScrollbar::new(queue_list.clone(), cx)),
            sidebar_scroll,
            view_state: saved.clone(),
            track_drag: None,
            track_selection: Default::default(),
            selection_playlist: saved.playlist,
            reorder_focus: None,
            reorder_reveal: None,
            reorder_busy: false,
            tracks_ready: false,
            library_ready: false,
            queue_ready: false,
            player,
            playlists: vec![],
            tracks: Arc::new(vec![]),
            selected: saved.playlist,
            unavailable_selection: None,
            seek: seek.clone(),
            volume_slider: volume_slider.clone(),
            track_list,
            queue_list,
            search: search.clone(),
            playlist_search: playlist_search.clone(),
            field,
            google_name: integrations::get_secret("google-refresh")
                .ok()
                .and_then(|_| integrations::get_secret("google-name").ok()),
            lastfm_name: integrations::get_secret("lastfm-session")
                .ok()
                .and_then(|_| integrations::get_secret("lastfm-name").ok()),
            account_busy: false,
            form: Form::None,
            create_google: true,
            last_sync: std::time::Instant::now(),
            freshness: String::new(),
            queue_view: saved.queue,
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
            pending_seek: None,
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
            .push(cx.subscribe(&seek, |this, _, event, cx| {
                let BarChange(value) = event;
                this.position = *value as f64 * this.duration;
                this.pending_seek = Some(this.player.seek_with_preview(this.position));
                cx.notify();
            }));
        app.subscriptions
            .push(cx.subscribe(&search, |this, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    this.tracks_ready = false;
                    if let Some(id) = this.selected {
                        let query = this.search.read(cx).value().to_string();
                        let position = this.view_state.playlists.entry(id).or_default();
                        if position.query != query {
                            *position = mousa::view_state::BrowsePosition {
                                query,
                                ..Default::default()
                            };
                        }
                    }
                    this.schedule_search(cx)
                }
            }));
        app.subscriptions
            .push(cx.subscribe(&playlist_search, |_, _, _: &InputEvent, cx| cx.notify()));
        app.refresh_library();
        app.refresh_tracks(cx);
        if app.google_name.is_some() {
            app.sync_google();
        }
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                if this
                    .update_in(cx, |this, window, cx| this.tick(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        app
    }
    fn account_action(&mut self, google: bool, cx: &mut Context<Self>) {
        if self.account_busy {
            return;
        }
        let connected = if google {
            self.google_name.is_some()
        } else {
            self.lastfm_name.is_some()
        };
        self.account_busy = true;
        self.error = None;
        self.status = if connected {
            "Signing out…"
        } else {
            "Complete sign-in in your browser…"
        }
        .into();
        if google && connected {
            self.player.send(Command::Stop);
        }
        self.job(move || {
            let result = if connected {
                (if google {
                    integrations::google_logout()
                } else {
                    integrations::lastfm_logout()
                })
                .map(|_| None)
            } else {
                (if google {
                    integrations::google_login()
                } else {
                    integrations::lastfm_login()
                })
                .map(Some)
            };
            if google && result.is_ok() {
                db::library()?.save("youtube-catalog-checked", "0")?;
            }
            Ok(Reply::Account(google, result.map_err(|e| format!("{e:#}"))))
        });
        cx.notify();
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
        let selected = self.selected;
        let playing = self.playing_id;
        self.job(move || {
            sync::refresh(|| {
                let _ = tx.send(Reply::Done("Refreshing YouTube playlists…".into()));
            })?;
            if let Some(id) = selected {
                sync::refresh_playlist(id, false, false)?;
            }
            if let Some(entry) = playing
                && let Ok(id) = db::library()?.entry_playlist(entry)
                && Some(id) != selected
            {
                sync::refresh_playlist(id, false, false)?;
            }
            sync::refresh_background(selected)?;
            Ok(Reply::Done("Cached playlists ready".into()))
        });
    }
    fn sync_selected(&self, force: bool, opened: bool) {
        if self.google_name.is_none() {
            return;
        }
        if let Some(id) = self.selected {
            self.job(move || {
                sync::refresh_playlist(id, force, opened)?;
                Ok(Reply::Done("Playlist ready".into()))
            });
        }
    }
    fn schedule_search(&mut self, cx: &mut Context<Self>) {
        self.track_selection.clear();
        self.track_drag = None;
        self.generation += 1; // Invalidate an in-flight result as soon as input changes.
        self.search_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(80))
                .await;
            let _ = this.update(cx, |this, cx| this.refresh_tracks(cx));
        }));
    }
    fn refresh_tracks(&mut self, cx: &mut Context<Self>) {
        if self.selection_playlist != self.selected {
            self.track_selection.clear();
            self.track_drag = None;
            self.selection_playlist = self.selected;
        }
        self.generation += 1;
        let generation = self.generation;
        let query = self.search.read(cx).value().to_string();
        if let Some(id) = self.selected {
            self.job(move || {
                Ok(Reply::Tracks(
                    generation,
                    db::library()?.tracks(id, &query, Sort::Position)?,
                    mousa::sync_policy::freshness(&db::library()?, id, integrations::now())?,
                ))
            });
        } else {
            self.track_list.reset(0);
            self.tracks = Arc::new(vec![]);
        }
        cx.notify();
    }
    fn remember_view(&mut self, cx: &App) {
        if self.library_ready {
            self.view_state.library_scroll = f32::from(self.sidebar_scroll.offset().y);
        }
        self.view_state.playlist = self.selected;
        self.view_state.queue = self.queue_view;
        self.view_state.library_query = self.playlist_search.read(cx).value().to_string();
        if self.tracks_ready
            && let Some(id) = self.selected
        {
            let offset = self.track_list.logical_scroll_top();
            self.view_state.playlists.insert(
                id,
                mousa::view_state::BrowsePosition {
                    query: self.search.read(cx).value().to_string(),
                    item: offset.item_ix,
                    offset: f32::from(offset.offset_in_item),
                },
            );
        }
        if self.queue_ready {
            let offset = self.queue_list.logical_scroll_top();
            self.view_state.queue_position.item = offset.item_ix;
            self.view_state.queue_position.offset = f32::from(offset.offset_in_item);
        }
    }
    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.remember_view(cx);
        if self.google_name.is_some() && self.last_sync.elapsed() >= Duration::from_secs(600) {
            self.sync_google();
        }
        let mut changed = false;
        while let Ok(reply) = self.rx.try_recv() {
            changed = true;
            match reply {
                Reply::Reordered(result) => {
                    self.reorder_busy = false;
                    self.tracks_ready = false;
                    match result {
                        Ok(()) => self.status = "Track moved".into(),
                        Err(error) => {
                            self.status = error.clone();
                            self.error = Some(error);
                        }
                    }
                    self.refresh_tracks(cx);
                }
                Reply::Account(google, result) => {
                    self.account_busy = false;
                    match result {
                        Ok(name) => {
                            if google {
                                self.google_name = name;
                                self.form = Form::None;
                                if self.google_name.is_some() {
                                    self.sync_google();
                                } else {
                                    self.player.send(Command::Stop);
                                }
                            } else {
                                self.lastfm_name = name;
                            }
                            self.error = None;
                            self.status = "Account updated".into();
                        }
                        Err(e) => {
                            self.error = Some(e);
                        }
                    }
                }
                Reply::Library(p) => {
                    self.player.refresh_metadata();
                    if !self.library_ready {
                        self.sidebar_scroll
                            .set_offset(point(px(0.), px(self.view_state.library_scroll.min(0.))));
                        self.library_ready = true;
                    }
                    self.playlists = p;
                    if self.selected.is_none()
                        || !self.playlists.iter().any(|p| Some(p.id) == self.selected)
                    {
                        self.tracks_ready = false;
                        self.search.update(cx, |s, cx| s.set_value("", window, cx));
                        self.selected = self.playlists.first().map(|p| p.id);
                        if let Some(id) = self.selected {
                            self.view_state.playlists.remove(&id);
                        }
                        self.refresh_tracks(cx);
                        self.sync_selected(false, true);
                    }
                }
                Reply::OpenQueueTrack(g, playlist, index) => {
                    if g == self.generation && self.queue_view {
                        self.remember_view(cx);
                        self.tracks_ready = false;
                        self.selected = Some(playlist);
                        self.queue_view = false;
                        self.form = Form::None;
                        self.search.update(cx, |s, cx| s.set_value("", window, cx));
                        self.view_state.playlists.insert(
                            playlist,
                            mousa::view_state::BrowsePosition {
                                query: String::new(),
                                item: index,
                                offset: 0.,
                            },
                        );
                        self.freshness.clear();
                        self.refresh_tracks(cx);
                        self.sync_selected(false, true);
                    }
                }
                Reply::Tracks(g, t, freshness) => {
                    if g == self.generation {
                        // A refreshed snapshot may change indices beneath a drag.
                        self.track_drag = None;
                        let offset = self
                            .view_state
                            .playlists
                            .get(&self.selected.unwrap_or_default())
                            .cloned()
                            .unwrap_or_default();
                        self.track_list.reset(t.len());
                        if !t.is_empty() {
                            self.track_list.scroll_to(ListOffset {
                                item_ix: offset.item.min(t.len() - 1),
                                offset_in_item: px(offset.offset.max(0.)),
                            });
                        }
                        if let Some(id) = self.reorder_reveal.take()
                            && let Some(index) = t.iter().position(|track| track.id == id)
                        {
                            self.track_list.scroll_to_reveal_item(index);
                        }
                        self.track_selection
                            .retain(&t.iter().map(|track| track.id).collect::<Vec<_>>());
                        self.tracks_ready = true;
                        self.tracks = Arc::new(t);
                        self.freshness = freshness;
                    }
                }
                Reply::Done(s) => {
                    self.status = s;
                    self.refresh_library();
                    self.refresh_tracks(cx);
                }
                Reply::Created(id) => {
                    self.remember_view(cx);
                    self.tracks_ready = false;
                    self.queue_view = false;
                    self.search.update(cx, |s, cx| s.set_value("", window, cx));
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
        let preview = self.pending_seek.as_ref().and_then(PendingSeek::position);
        if preview.is_none() {
            self.pending_seek = None;
        }
        let position = preview.unwrap_or(s.position);
        let title = s
            .current()
            .map(|t| t.title.clone())
            .unwrap_or_else(|| "Nothing playing yet".into());
        if self.revision != s.revision {
            self.revision = s.revision;
            if !Arc::ptr_eq(&self.queue, &s.queue) {
                let offset = &self.view_state.queue_position;
                self.queue_list.reset(s.queue.len());
                if !s.queue.is_empty() {
                    self.queue_list.scroll_to(ListOffset {
                        item_ix: offset.item.min(s.queue.len() - 1),
                        offset_in_item: px(offset.offset.max(0.)),
                    });
                }
                self.queue_ready = true;
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
        let availability_changed = !s.error.is_empty() && self.status != s.error;
        if availability_changed {
            self.error = Some(s.error.clone());
            self.status = s.error.clone();
            changed = true;
        }
        drop(s);
        if availability_changed {
            self.player.refresh_metadata();
            self.refresh_tracks(cx);
        }
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
            _ => Ok(Reply::Done(String::new())),
        });
        cx.notify();
    }
    fn play(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Some(playlist) = self.selected {
            let player = self.player.clone();
            self.job(move || {
                let tracks: Vec<_> = db::library()?
                    .tracks(playlist, "", Sort::Position)?
                    .into_iter()
                    .filter(|t| t.unavailable.is_none())
                    .collect();
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
    fn reorder(&mut self, index: usize, boundary: usize, cx: &mut Context<Self>) {
        if self.reorder_busy
            || !self.tracks_ready
            || self.queue_view
            || !self.search.read(cx).value().is_empty()
            || index >= self.tracks.len()
        {
            return;
        }
        let order: Vec<_> = self.tracks.iter().map(|t| t.id).collect();
        let ids = if self.track_selection.ids.contains(&order[index]) {
            order
                .iter()
                .filter(|id| self.track_selection.ids.contains(id))
                .copied()
                .collect()
        } else {
            vec![order[index]]
        };
        let before = order.get(boundary).copied();
        let Ok(moved) = mousa::track_selection::moved_order(&order, &ids, before) else {
            return;
        };
        if moved == order {
            return;
        }
        self.reorder_busy = true;
        self.job(move || {
            Ok(Reply::Reordered(
                sync::move_entries_before(&ids, before).map_err(|e| e.to_string()),
            ))
        });
        cx.notify();
    }
    fn drop_boundary(&self, pointer: Point<Pixels>) -> Option<(usize, Pixels)> {
        let bounds = self.track_list.viewport_bounds();
        if !bounds.contains(&pointer) {
            return None;
        }
        let scroll = self.track_list.logical_scroll_top();
        let height = control_size::row();
        let y = pointer.y - bounds.top() + scroll.offset_in_item;
        let boundary = (scroll.item_ix as f32 + f32::from(y) / f32::from(height))
            .round()
            .clamp(0., self.tracks.len() as f32) as usize;
        Some((
            boundary,
            height * (boundary as f32 - scroll.item_ix as f32) - scroll.offset_in_item,
        ))
    }
    fn rows(&self, queue: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let tracks = if queue {
            self.queue.clone()
        } else {
            self.tracks.clone()
        };

        let can_move = !queue
            && self.tracks_ready
            && !self.reorder_busy
            && self.search.read(cx).value().is_empty();
        let source = self
            .track_drag
            .as_ref()
            .filter(|d| d.active)
            .map(|d| d.index);
        let selected_ids = self.track_selection.ids.clone();
        let dragging_ids = self
            .track_drag
            .as_ref()
            .filter(|d| d.active)
            .map(|d| d.ids.clone())
            .unwrap_or_default();
        let reorder_focus = self.reorder_focus.clone();
        let unavailable_selection = self.unavailable_selection;
        let active = self.playing_id;
        let active_queue_index = self.queue_index;
        let rows = list(
            if queue {
                self.queue_list.clone()
            } else {
                self.track_list.clone()
            },
            cx.processor(move |_this, i: usize, _, cx| {
                if !queue && dragging_ids.contains(&tracks[i].id) {
                    return div()
                        .h(control_size::row())
                        .w_full()
                        .bg(color::canvas())
                        .into_any_element();
                }
                let t = &tracks[i];
                let unavailable = t.unavailable.is_some();
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
                let inverted = unavailable
                    && (is_active
                        || unavailable_selection
                            == Some((queue, if queue { i as i64 } else { id })));
                ui::track_row(if queue { i } else { id as usize }, is_active)
                    .when(unavailable, |d| {
                        d.hover(move |s| {
                            s.bg(if inverted {
                                color::ink()
                            } else {
                                color::white()
                            })
                        })
                        .focus(move |s| {
                            s.bg(if inverted {
                                color::ink()
                            } else {
                                color::canvas()
                            })
                        })
                        .when(inverted, |d| d.bg(color::ink()).text_color(color::lime()))
                    })
                    .when(!queue && selected_ids.contains(&id), |d| {
                        d.border_l(reorder::line())
                            .when(!is_active && !inverted, |d| d.bg(color::canvas()))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.unavailable_selection = if unavailable {
                            Some((queue, if queue { i as i64 } else { id }))
                        } else {
                            None
                        };
                        cx.notify();
                    }))
                    .when_some(
                        reorder_focus
                            .as_ref()
                            .filter(|(entry, _)| !queue && *entry == id),
                        |d, (_, focus)| d.track_focus(focus),
                    )
                    .when(!queue, |d| d.h(control_size::row()).overflow_hidden())
                    .on_key_down(cx.listener(move |this, e: &KeyDownEvent, window, cx| {
                        if unavailable && matches!(e.keystroke.key.as_str(), "enter" | "space") {
                            this.unavailable_selection =
                                Some((queue, if queue { i as i64 } else { id }));
                            cx.notify();
                            cx.stop_propagation();
                            return;
                        }
                        if can_move && e.keystroke.modifiers.alt {
                            let indices: Vec<_> = if this.track_selection.ids.contains(&id) {
                                this.tracks
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, t)| this.track_selection.ids.contains(&t.id))
                                    .map(|(i, _)| i)
                                    .collect()
                            } else {
                                vec![i]
                            };
                            let boundary = match e.keystroke.key.as_str() {
                                "up" => indices[0].saturating_sub(1),
                                "down" => (indices[indices.len() - 1] + 2).min(this.tracks.len()),
                                _ => return,
                            };
                            this.reorder_focus = window.focused(cx).map(|focus| (id, focus));
                            this.reorder_reveal = Some(id);
                            this.reorder(i, boundary, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .when(!queue, |d| {
                        d.when(can_move, |d| d.cursor(CursorStyle::OpenHand))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                                    // A focused child control owns this press.
                                    if window.default_prevented() {
                                        return;
                                    }
                                    if !this.tracks_ready || this.reorder_busy {
                                        return;
                                    }
                                    let order: Vec<_> = this.tracks.iter().map(|t| t.id).collect();
                                    let collapse_on_release = this.track_selection.press(
                                        &order,
                                        i,
                                        e.modifiers.control,
                                        e.modifiers.shift,
                                    );
                                    this.unavailable_selection =
                                        if unavailable { Some((false, id)) } else { None };
                                    cx.notify();
                                    if !can_move {
                                        if collapse_on_release {
                                            this.track_selection.only(id);
                                        }
                                        return;
                                    }
                                    if !this.track_selection.ids.contains(&id) {
                                        return;
                                    }
                                    let ids = order
                                        .iter()
                                        .filter(|id| this.track_selection.ids.contains(id))
                                        .copied()
                                        .collect();
                                    let top = this
                                        .track_list
                                        .bounds_for_item(i)
                                        .map(|b| b.top())
                                        .unwrap_or(e.position.y);
                                    this.track_drag = Some(TrackDrag {
                                        index: i,
                                        ids,
                                        collapse_on_release,
                                        pointer: e.position,
                                        origin: e.position,
                                        active: false,
                                        offset: e.position.y - top,
                                        generation: this.generation,
                                    });
                                    cx.spawn(async move |this, cx| {
                                        loop {
                                            cx.background_executor()
                                                .timer(Duration::from_millis(16))
                                                .await;
                                            let keep = this
                                                .update(cx, |this, cx| {
                                                    let Some(drag) = this.track_drag.clone() else {
                                                        return false;
                                                    };
                                                    if drag.generation != this.generation
                                                        || this.queue_view
                                                    {
                                                        this.track_drag = None;
                                                        cx.notify();
                                                        return false;
                                                    }
                                                    let bounds = this.track_list.viewport_bounds();
                                                    if drag.active && bounds.contains(&drag.pointer)
                                                    {
                                                        let band = control_size::row();
                                                        let delta = if drag.pointer.y
                                                            < bounds.top() + band
                                                        {
                                                            -reorder::scroll_step()
                                                        } else if drag.pointer.y
                                                            > bounds.bottom() - band
                                                        {
                                                            reorder::scroll_step()
                                                        } else {
                                                            px(0.)
                                                        };
                                                        if delta != px(0.) {
                                                            let current = this
                                                                .track_list
                                                                .scroll_px_offset_for_scrollbar();
                                                            let max = this
                                                                .track_list
                                                                .max_offset_for_scrollbar()
                                                                .height;
                                                            let next = (-current.y + delta)
                                                                .clamp(px(0.), max);
                                                            this.track_list
                                                                .set_offset_from_scrollbar(point(
                                                                    px(0.),
                                                                    -next,
                                                                ));
                                                        }
                                                    }
                                                    cx.notify();
                                                    true
                                                })
                                                .unwrap_or(false);
                                            if !keep {
                                                break;
                                            }
                                        }
                                    })
                                    .detach();
                                    cx.notify();
                                }),
                            )
                    })
                    .child(
                        div()
                            .relative()
                            .w(layout::row_index())
                            .h(control_size::row_action())
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .font_family(font::MONO)
                            .text_size(text::secondary())
                            .child(
                                div()
                                    .when(!unavailable && source.is_none(), |d| {
                                        d.group_hover("track-row", |s| s.invisible())
                                    })
                                    .child(format!(
                                        "{:02}",
                                        if queue { i + 1 } else { t.playlist_index }
                                    )),
                            )
                            .when(!unavailable && source.is_none(), |d| {
                                d.child(
                                    ui::track_play_button(("play-track", i))
                                        .absolute()
                                        .left_0()
                                        .top_0()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            if queue {
                                                this.player.send(Command::Jump(i));
                                            } else {
                                                this.play(id, cx);
                                            }
                                        })),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .when(queue, |d| d.whitespace_normal())
                                    .when(!queue, |d| d.truncate())
                                    .font_weight(FontWeight::BOLD)
                                    .when(unavailable, |d| d.line_through())
                                    .child(title),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(text::secondary())
                                    .when(!unavailable, |d| d.text_color(color::muted()))
                                    .child(artist),
                            ),
                    )
                    .when(!unavailable || queue, |d| {
                        d.child(
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
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if queue {
                                        this.player.send(Command::Remove(i));
                                    } else {
                                        this.player.send(Command::Append(track.clone()));
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                    })
                    .when(queue && id > 0 && !unavailable, |d| {
                        let url = t.url.clone();
                        d.child(
                            icon_button(
                                ("open-playlist", i),
                                "external-link",
                                "Open track in playlist",
                                false,
                                false,
                                true,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    let generation = this.generation;
                                    let url = url.clone();
                                    this.job(move || {
                                        let library = db::library()?;
                                        let playlist = library.entry_playlist(id)?;
                                        let tracks =
                                            library.tracks(playlist, "", Sort::Position)?;
                                        let index = tracks
                                            .iter()
                                            .position(|track| track.id == id && track.url == url)
                                            .ok_or_else(|| {
                                                anyhow::anyhow!(
                                                    "Track is no longer in its playlist"
                                                )
                                            })?;
                                        Ok(Reply::OpenQueueTrack(generation, playlist, index))
                                    });
                                },
                            )),
                        )
                    })
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
                            .when(!unavailable, |d| d.bg(color::pink()))
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
                    .when_some(t.unavailable.clone(), |d, label| {
                        d.child(
                            div()
                                .w(control_size::availability_width())
                                .h(control_size::availability_height())
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .border(border::strong())
                                .border_color(if inverted {
                                    color::lime()
                                } else {
                                    color::ink()
                                })
                                .bg(if inverted {
                                    color::ink()
                                } else {
                                    color::canvas()
                                })
                                .text_color(if inverted {
                                    color::lime()
                                } else {
                                    color::ink()
                                })
                                .font_family(font::MONO)
                                .text_size(text::label())
                                .font_weight(FontWeight::BOLD)
                                .child(label),
                        )
                    })
                    .when(!unavailable, |d| {
                        d.child(
                            div()
                                .min_w(layout::time_label())
                                .flex_shrink_0()
                                .text_right()
                                .font_family(font::MONO)
                                .text_size(text::label())
                                .child(duration),
                        )
                    })
                    .into_any_element()
            }),
        )
        .h_full()
        .w_full();
        let entity = cx.entity();
        let weak = entity.downgrade();
        div()
            .flex()
            .size_full()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_hidden()
                    .child(rows)
                    .when(!queue, |d| {
                        d.child(
                            canvas(
                                move |bounds, window, cx| {
                                    entity.update(cx, |this, cx| {
                                        let mut overlay = div().size_full().relative();
                                        if let Some(drag) =
                                            this.track_drag.as_ref().filter(|d| d.active)
                                        {
                                            if let Some((_, y)) = this.drop_boundary(drag.pointer) {
                                                overlay = overlay.child(
                                                    div()
                                                        .absolute()
                                                        .left_0()
                                                        .right_0()
                                                        .top(y - reorder::line() / 2.)
                                                        .h(reorder::line())
                                                        .bg(color::ink())
                                                        .child(
                                                            div()
                                                                .absolute()
                                                                .left_0()
                                                                .top(
                                                                    (reorder::line()
                                                                        - reorder::cap())
                                                                        / 2.,
                                                                )
                                                                .size(reorder::cap())
                                                                .bg(color::ink()),
                                                        ),
                                                );
                                            }
                                            if let Some(track) = this.tracks.get(drag.index) {
                                                overlay =
                                                    overlay.child(
                                                        div()
                                                            .absolute()
                                                            .left(space::lg())
                                                            .right(space::lg())
                                                            .top(
                                                                (drag.pointer.y
                                                                    - bounds.top()
                                                                    - drag.offset)
                                                                    .clamp(
                                                                        px(0.),
                                                                        (bounds.size.height
                                                                            - control_size::row())
                                                                        .max(px(0.)),
                                                                    ),
                                                            )
                                                            .h(control_size::row())
                                                            .px(space::lg())
                                                            .flex()
                                                            .items_center()
                                                            .gap(space::md())
                                                            .border(border::strong())
                                                            .border_color(color::ink())
                                                            .shadow(shadow::lg())
                                                            .bg(
                                                                if Some(track.id) == this.playing_id
                                                                {
                                                                    color::lime()
                                                                } else {
                                                                    color::white()
                                                                },
                                                            )
                                                            .child(
                                                                div()
                                                                    .w(layout::row_index())
                                                                    .font_family(font::MONO)
                                                                    .child(
                                                                        track
                                                                            .playlist_index
                                                                            .to_string(),
                                                                    ),
                                                            )
                                                            .child(
                                                                div()
                                                                    .flex_1()
                                                                    .min_w_0()
                                                                    .child(
                                                                        div()
                                                                            .truncate()
                                                                            .font_weight(
                                                                                FontWeight::BOLD,
                                                                            )
                                                                            .child(
                                                                                track.title.clone(),
                                                                            ),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .truncate()
                                                                            .text_size(
                                                                                text::secondary(),
                                                                            )
                                                                            .child(
                                                                                track
                                                                                    .artist
                                                                                    .clone(),
                                                                            ),
                                                                    ),
                                                            )
                                                            .child(if drag.ids.len() > 1 {
                                                                format!("{} tracks", drag.ids.len())
                                                            } else {
                                                                time(track.duration)
                                                            }),
                                                    );
                                            }
                                        }
                                        let mut element = overlay.into_any_element();
                                        element.layout_as_root(
                                            bounds.size.map(AvailableSpace::Definite),
                                            window,
                                            cx,
                                        );
                                        element.prepaint_at(bounds.origin, window, cx);
                                        (
                                            element,
                                            this.track_drag.as_ref().is_some_and(|d| d.active),
                                        )
                                    })
                                },
                                move |_, (mut element, dragging), window, cx| {
                                    element.paint(window, cx);
                                    if dragging {
                                        window.set_window_cursor_style(CursorStyle::ClosedHand);
                                    }
                                    let moving = weak.clone();
                                    window.on_mouse_event(
                                        move |e: &MouseMoveEvent, phase, window, cx| {
                                            if phase == DispatchPhase::Capture {
                                                let _ = moving.update(cx, |this, cx| {
                                                    if let Some(drag) = &mut this.track_drag {
                                                        drag.pointer = e.position;
                                                        let was_active = drag.active;
                                                        drag.active |= (e.position.x
                                                            - drag.origin.x)
                                                            .abs()
                                                            >= reorder::drag_threshold()
                                                            || (e.position.y - drag.origin.y).abs()
                                                                >= reorder::drag_threshold();
                                                        if drag.active && !was_active {
                                                            this.focus.focus(window);
                                                        }
                                                        cx.notify();
                                                    }
                                                });
                                            }
                                        },
                                    );
                                    let ending = weak.clone();
                                    window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                                        if phase == DispatchPhase::Capture
                                            && e.button == MouseButton::Left
                                        {
                                            let _ = ending.update(cx, |this, cx| {
                                                if let Some(drag) = this.track_drag.take() {
                                                    if drag.active
                                                        && drag.generation == this.generation
                                                        && let Some((boundary, _)) =
                                                            this.drop_boundary(e.position)
                                                    {
                                                        this.reorder(drag.index, boundary, cx);
                                                    }
                                                    if !drag.active
                                                        && drag.collapse_on_release
                                                        && drag.generation == this.generation
                                                    {
                                                        this.track_selection
                                                            .only(this.tracks[drag.index].id);
                                                    }
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                }
                                            });
                                        }
                                    });
                                },
                            )
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                        )
                    }),
            )
            .child(if queue {
                self.queue_scrollbar.clone()
            } else {
                self.track_scrollbar.clone()
            })
    }
}
impl Render for Mousa {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window_bounds = window.window_bounds();
        let bounds = window_bounds.get_bounds();
        self.view_state.maximized = matches!(window_bounds, WindowBounds::Maximized(_));
        self.view_state.window_origin =
            Some([f32::from(bounds.origin.x), f32::from(bounds.origin.y)]);
        self.view_state.window_size =
            Some([f32::from(bounds.size.width), f32::from(bounds.size.height)]);
        if self.google_name.is_none() {
            let sign_in = ui::google_sign_in(self.account_busy)
                .on_click(cx.listener(|this, _, _, cx| this.account_action(true, cx)));
            return ui::landing(sign_in, self.error.as_deref()).into_any_element();
        }
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
        let unavailable_count = if self.queue_view {
            &self.queue
        } else {
            &self.tracks
        }
        .iter()
        .filter(|t| t.unavailable.is_some())
        .count();
        let availability_summary = if unavailable_count > 0 {
            format!(" / {unavailable_count} unavailable")
        } else {
            String::new()
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
                button(
                    "library",
                    "YOUR LIBRARY",
                    if !self.queue_view {
                        color::ink()
                    } else {
                        color::surface()
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.queue_view = false;
                    cx.notify();
                })),
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
            .track_scroll(&self.sidebar_scroll)
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
                    self.selected == Some(id) && !self.queue_view,
                )
                .on_click(cx.listener(move |this, _, w, cx| {
                    this.remember_view(cx);
                    this.tracks_ready = false;
                    this.selected = Some(id);
                    this.queue_view = false;
                    let position = this
                        .view_state
                        .playlists
                        .get(&id)
                        .cloned()
                        .unwrap_or_default();
                    this.search
                        .update(cx, |s, cx| s.set_value(position.query.clone(), w, cx));
                    this.view_state.playlists.insert(id, position);
                    this.freshness.clear();
                    this.refresh_tracks(cx);
                    this.sync_selected(false, true);
                })),
            );
        }
        sidebar = sidebar
            .child(list)
            .child(
                ui::account_button(
                    "google",
                    "google",
                    self.google_name.as_deref(),
                    self.account_busy,
                )
                .on_click(cx.listener(|this, _, _, cx| this.account_action(true, cx))),
            )
            .child(
                ui::account_button(
                    "lastfm",
                    "lastfm",
                    self.lastfm_name.as_deref(),
                    self.account_busy,
                )
                .on_click(cx.listener(|this, _, _, cx| this.account_action(false, cx))),
            );
        let toolbar = div()
            .flex()
            .items_center()
            .gap(space::sm())
            .child(div().flex_1().child(ui::input(&self.search, cx)))
            .child(
                button("refresh-current", "REFRESH", color::surface())
                    .on_click(cx.listener(|this, _, _, _| this.sync_selected(true, true))),
            )
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
            .child(div().text_size(text::secondary()).child(
                if self.queue_view || self.freshness.is_empty() {
                    format!("{n} tracks{availability_summary}")
                } else {
                    format!(
                        "{n} cached tracks{availability_summary} · {}",
                        self.freshness
                    )
                },
            ));
        if let Some(error) = self.error.clone() {
            content = content.child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::md())
                    .p(space::md())
                    .border(border::strong())
                    .border_color(color::ink())
                    .bg(color::pink())
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
                .occlude()
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
                .occlude()
                .on_click(cx.listener(|this, _, _, _| this.player.send(Command::Previous))),
            )
            .child(step_button("back10", "−10").occlude().on_click(
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
                .occlude()
                .on_click(cx.listener(|this, _, _, _| this.player.send(Command::Toggle))),
            )
            .child(step_button("forward10", "+10").occlude().on_click(
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
                .occlude()
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
                .occlude()
                .on_click(cx.listener(|this, _, _, _| this.player.send(Command::Repeat))),
            );
        let info = div()
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
                .occlude()
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
        )
        .id("now-playing-open-queue")
        .tab_index(0)
        .focus(|s| s.shadow(vec![shadow::focus_ring()]))
        .cursor_pointer()
        .hover(|s| s.bg(color::white()))
        .active(|s| s.bg(color::canvas()))
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
        }));
        div()
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" && this.track_drag.is_some() {
                    this.track_drag = None;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
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
            .into_any_element()
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
                // Leave the platform close callback before quitting (X11 holds a borrow).
                cx.spawn(async |cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(1))
                        .await;
                    let _ = cx.update(|cx| cx.quit());
                })
                .detach();
            }
        })
        .detach();
        let saved = mousa::view_state::ViewState::load();
        let saved_size = saved.window_size.filter(|s| {
            s.iter()
                .all(|v| v.is_finite() && *v >= 100. && *v <= 10000.)
        });
        let window_size = saved_size
            .map(|s| {
                size(
                    px(s[0]).max(layout::min_width()),
                    px(s[1]).max(layout::min_height()),
                )
            })
            .unwrap_or_else(|| size(layout::viewport_width(), layout::viewport_height()));
        let mut bounds = Bounds::centered(None, window_size, cx);
        if let Some(origin) = saved
            .window_origin
            .filter(|o| o.iter().all(|v| v.is_finite()))
        {
            let origin = point(px(origin[0]), px(origin[1]));
            if cx
                .displays()
                .iter()
                .any(|display| display.bounds().contains(&origin))
            {
                bounds.origin = origin;
            }
        }
        let window_bounds = if saved.maximized {
            WindowBounds::Maximized(bounds)
        } else {
            WindowBounds::Windowed(bounds)
        };
        cx.open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
                window_min_size: Some(size(layout::min_width(), layout::min_height())),
                app_id: Some("mousa".into()),
                titlebar: Some(TitlebarOptions {
                    title: Some("Mousa".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |window, cx| {
                window.set_window_title("Mousa");
                let view = cx.new(|cx| Mousa::new(saved, window, cx));
                view.read(cx).focus.focus(window);
                let player = view.read(cx).player.clone();
                let saved_view = view.clone();
                cx.on_app_quit(move |cx| {
                    let browsing = saved_view.update(cx, |v, cx| {
                        v.remember_view(cx);
                        v.view_state.clone()
                    });
                    let snapshot = player.state.lock().unwrap().clone();
                    cx.background_executor().spawn(async move {
                        let _ = browsing.save();
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
