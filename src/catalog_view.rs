//! Catalog navigation is independent of playlist selection and playback state.
use crate::{
    catalog::{self, Destination, Item, Page},
    db::{self, Playlist, Track},
    player::{Command, Player},
    sync,
    theme::{border, catalog as metrics, color, font, shadow, size, space, text},
    ui::{self, button},
};
use gpui::{prelude::*, *};
use gpui_component::input::{InputEvent, InputState};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::Duration,
};

pub enum Event {
    Close,
    Saved,
}
#[derive(Clone)]
struct View {
    page: Page,
    destination: Destination,
    item: Option<Item>,
    scroll: ScrollHandle,
    filter: String,
    selection: std::collections::BTreeMap<TrackTable, crate::track_selection::TrackSelection>,
}
impl View {
    fn snapshot(&self) -> crate::view_state::CatalogPageState {
        crate::view_state::CatalogPageState {
            page: self.page.clone(),
            destination: self.destination.clone(),
            item: self.item.clone(),
            filter: self.filter.clone(),
            scroll: f32::from(self.scroll.offset().y),
        }
    }

    fn restore(saved: crate::view_state::CatalogPageState) -> Self {
        let scroll = ScrollHandle::new();
        scroll.set_offset(point(px(0.), px(saved.scroll.min(0.))));
        Self {
            page: saved.page,
            destination: saved.destination,
            item: saved.item,
            filter: saved.filter,
            scroll,
            selection: Default::default(),
        }
    }
}
/// A page can contain several independently selectable track tables.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TrackTable {
    Results,
    Collection,
    Section(String, bool),
}
impl TrackTable {
    fn selected_items(
        &self,
        items: &[Item],
        selection: &crate::track_selection::TrackSelection,
    ) -> Vec<Item> {
        self.order(items)
            .into_iter()
            .filter(|id| selection.ids.contains(id))
            .map(|id| items[id as usize].clone())
            .collect()
    }
    fn order(&self, items: &[Item]) -> Vec<i64> {
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                (item.video.is_some() || item.unavailable)
                    && match self {
                        Self::Results => false,
                        Self::Collection => item.collection_member,
                        Self::Section(category, exclude_collection) => {
                            (!exclude_collection || !item.collection_member)
                                && &item.category == category
                        }
                    }
            })
            .map(|(i, _)| i as i64)
            .collect()
    }
}
#[derive(Clone, Copy)]
enum Action {
    Play(usize),
    Shuffle,
    Queue,
    Save,
    Library,
}
enum Feedback {
    Queued(usize),
    Playlist { count: usize, name: String },
    Library(String),
    AlreadyInLibrary(String),
}
impl Feedback {
    fn message(&self) -> String {
        match self {
            Self::Queued(n) => format!("Added {n} tracks to the queue"),
            Self::Playlist { count, name } => {
                format!("Added {count} tracks to the playlist “{name}”")
            }
            Self::Library(name) => format!("Added the playlist “{name}” to your library"),
            Self::AlreadyInLibrary(name) => {
                format!("The playlist “{name}” is already in your library")
            }
        }
    }
}
enum Message {
    SongAlbum(u64, Result<Option<Item>, String>),
    Page(u64, bool, Result<Page, String>),
    Images,
    Subscription(u64, Result<Option<String>, String>),
    Prepared(u64, Action, Result<Vec<Item>, String>),
    Playlists(Result<Vec<Playlist>, String>),
    Saved(Result<Feedback, String>),
}
pub struct CatalogView {
    input: Entity<InputState>,
    name: Entity<InputState>,
    picker_filter: Entity<InputState>,
    view: View,
    history: Vec<View>,
    filters: Vec<(String, Destination)>,
    recent: Vec<String>,
    generation: u64,
    ticket: Arc<AtomicU64>,
    operation: u64,
    busy: bool,
    error: Option<String>,
    feedback: Option<Feedback>,
    album_unavailable: bool,
    debounce: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
    player: Player,
    tx: Sender<Message>,
    rx: Receiver<Message>,
    pending: Option<Vec<Track>>,
    external_picker: bool,
    playlists: Vec<Playlist>,
    save_busy: bool,
    create_google: bool,
    subscription: Option<Option<String>>,
    subscription_busy: bool,
    subscription_error: Option<String>,
    cancelled: Arc<AtomicBool>,
    preparing: bool,
    last_request: Option<(Destination, bool)>,
    scrollbar: Entity<ui::CatalogScrollbar>,
    standalone: bool,
    offline_preview: bool,
    focus_handle: FocusHandle,
    restore_focus: bool,
}
impl EventEmitter<Event> for CatalogView {}
impl CatalogView {
    /// Deterministic, offline component review used by the catalog gallery.
    #[doc(hidden)]
    pub fn preview(
        player: Player,
        page: Page,
        item: Option<Item>,
        state: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self::new(player, window, cx);
        view.standalone = true;
        view._subscriptions.clear();
        view.input.update(cx, |s, cx| {
            s.set_value(
                if matches!(state, "empty" | "first-use") {
                    ""
                } else if state == "no-results" {
                    "bachh"
                } else {
                    "bach"
                },
                window,
                cx,
            )
        });
        view.filters = page.filters.clone();
        view.view.page = page;
        view.view.item = item;
        if state.starts_with("selection") {
            let order = TrackTable::Collection.order(&view.view.page.items);
            let mut selection = crate::track_selection::TrackSelection::default();
            selection
                .ids
                .extend(
                    order
                        .into_iter()
                        .take(if state == "selection-single" { 1 } else { 4 }),
                );
            if state == "selection-unavailable" {
                if let Some(item) = view.view.page.items.get_mut(1) {
                    item.unavailable = true;
                }
            }
            view.view
                .selection
                .insert(TrackTable::Collection, selection);
        }
        view.recent = vec![
            "bach cantatas".into(),
            "harnoncourt".into(),
            "goldberg variations".into(),
        ];
        if state == "first-use" {
            view.recent.clear();
        }
        view.busy = state == "loading";
        view.error = (state == "error").then(|| "Offline preview".into());
        if state == "no-results" {
            view.view.filter = "Videos".into();
        }
        view.feedback = match state {
            "alert-success" => Some(Feedback::Playlist {
                count: 20,
                name: "Bach for Studying".into(),
            }),
            "alert-info" => Some(Feedback::AlreadyInLibrary("Bach for Studying".into())),
            _ => None,
        };
        view.subscription = Some(None);
        view
    }
    /// Keep workbench interactions local, including retry, navigation and account actions.
    #[doc(hidden)]
    pub fn isolate_preview(&mut self) {
        self.offline_preview = true;
    }

    pub fn new(player: Player, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_saved_state(player, None, window, cx)
    }

    pub fn snapshot(&self, cx: &App) -> crate::view_state::CatalogState {
        crate::view_state::CatalogState {
            query: self.input.read(cx).value().to_string(),
            current: self.view.snapshot(),
            history: self.history.iter().map(View::snapshot).collect(),
            filters: self.filters.clone(),
            reload: (self.busy || self.error.is_some()) && self.view.page.items.is_empty(),
        }
    }

    pub fn with_saved_state(
        player: Player,
        saved: Option<crate::view_state::CatalogState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Search songs, videos, albums, artists…")
        });
        if let Some(saved) = &saved {
            input.update(cx, |input, cx| {
                input.set_value(saved.query.clone(), window, cx)
            });
        }
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("New playlist name"));
        let picker_filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter destination playlists…"));
        let (tx, rx) = mpsc::channel();
        let recent = db::library()
            .ok()
            .and_then(|db| db.load("catalog-recent"))
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default();
        // set_value emits a deferred Change; don't let hydration erase restored history.
        let mut previous_query = input.read(cx).value().to_string();
        let subscriptions = vec![
            cx.subscribe(&input, move |this, _, e, cx| {
                let query = this.input.read(cx).value().to_string();
                if matches!(e, InputEvent::Change) && query != previous_query {
                    previous_query = query;
                    this.schedule(cx);
                }
            }),
            cx.subscribe(&picker_filter, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if this.update(cx, |this, cx| this.poll(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        let mut result = Self {
            standalone: false,
            offline_preview: false,
            focus_handle: cx.focus_handle(),
            restore_focus: false,
            scrollbar: cx.new(|_| ui::CatalogScrollbar::new()),
            input,
            name,
            picker_filter,
            view: View {
                page: Page::default(),
                destination: catalog::search(""),
                item: None,
                scroll: ScrollHandle::new(),
                filter: "All".into(),
                selection: Default::default(),
            },
            history: vec![],
            filters: vec![],
            recent,
            generation: 0,
            ticket: Arc::new(AtomicU64::new(0)),
            operation: 0,
            busy: false,
            error: None,
            feedback: None,
            album_unavailable: false,
            debounce: None,
            _subscriptions: subscriptions,
            player,
            tx,
            rx,
            pending: None,
            external_picker: false,
            playlists: vec![],
            save_busy: false,
            create_google: true,
            subscription: None,
            subscription_busy: false,
            subscription_error: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            preparing: false,
            last_request: None,
        };
        if let Some(saved) = saved {
            result.view = View::restore(saved.current);
            result.history = saved.history.into_iter().map(View::restore).collect();
            result.filters = saved.filters;
            result.load_subscription();
            if saved.reload {
                result.request(false, cx);
            }
        }
        result
    }
    pub fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.view.item.is_some() && !self.save_busy {
            self.cancel_collection();
            self.invalidate();
            self.busy = false;
            while let Some(view) = self.history.pop() {
                self.view = view;
            }
            self.view.item = None;
            self.pending = None;
            self.error = None;
        }
        self.input.read(cx).focus_handle(cx).focus(window);
        cx.notify();
    }
    fn invalidate(&mut self) {
        self.generation += 1;
        self.ticket.store(self.generation, Ordering::Relaxed);
    }
    fn cancel_collection(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.operation += 1;
        self.preparing = false;
    }
    pub fn reset_account(&mut self, cx: &mut Context<Self>) {
        self.feedback = None;
        self.album_unavailable = false;
        self.external_picker = false;
        self.invalidate();
        self.cancel_collection();
        self.debounce = None;
        self.pending = None;
        self.subscription = None;
        self.subscription_busy = false;
        self.busy = false;
        sync::clear_subscription_cache();
        cx.notify();
    }
    pub fn back(&mut self, cx: &mut Context<Self>) {
        if self.save_busy {
            return;
        }
        self.feedback = None;
        self.album_unavailable = false;
        if self.pending.take().is_some() || self.external_picker {
            if std::mem::take(&mut self.external_picker) {
                cx.emit(Event::Close);
            }
            self.restore_focus = true;
            self.feedback = None;
            cx.notify();
            return;
        }
        self.cancel_collection();
        self.debounce = None;
        self.invalidate();
        self.busy = false;
        self.error = None;
        if let Some(view) = self.history.pop() {
            self.view = view;
            self.restore_focus = true;
            self.load_subscription();
        } else {
            cx.emit(Event::Close);
        }
        cx.notify();
    }
    fn schedule(&mut self, cx: &mut Context<Self>) {
        self.invalidate();
        self.cancel_collection();
        self.error = None;
        self.feedback = None;
        self.history.clear();
        self.album_unavailable = false;
        self.view.item = None;
        let query = self.input.read(cx).value().trim().to_owned();
        self.view.selection.clear();
        self.view.page = Page::default();
        self.view.scroll = ScrollHandle::new();
        self.busy = !query.is_empty();
        self.view.destination = self
            .filters
            .iter()
            .find(|(label, _)| label == &self.view.filter)
            .map(|(_, d)| {
                let mut d = d.clone();
                d.body["query"] = query.clone().into();
                d
            })
            .unwrap_or_else(|| catalog::search(&query));
        if query.is_empty() {
            self.busy = false;
            self.debounce = None;
            cx.notify();
            return;
        }
        self.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            let _ = this.update(cx, |this, cx| this.request(false, cx));
        }));
        cx.notify();
    }
    fn request(&mut self, append: bool, cx: &mut Context<Self>) {
        self.fetch_request(self.view.destination.clone(), append, cx);
    }
    fn fetch_request(&mut self, destination: Destination, append: bool, cx: &mut Context<Self>) {
        if self.offline_preview {
            return;
        }
        self.last_request = Some((destination.clone(), append));
        self.invalidate();
        let generation = self.generation;
        self.busy = true;
        self.error = None;
        let tx = self.tx.clone();
        let ticket = self.ticket.clone();
        std::thread::spawn(move || {
            let result = catalog::fetch_cancellable(&destination, || {
                ticket.load(Ordering::Relaxed) != generation
            })
            .map_err(|e| format!("{e:#}"));
            let images: Vec<_> = result
                .as_ref()
                .map(|p| {
                    p.items
                        .iter()
                        .filter_map(|i| i.thumbnail.clone())
                        .chain(p.thumbnail.clone())
                        .collect()
                })
                .unwrap_or_default();
            let _ = tx.send(Message::Page(generation, append, result));
            for url in images {
                if ticket.load(Ordering::Relaxed) != generation {
                    break;
                }
                if catalog::load_thumbnail(&url).is_ok() {
                    let _ = tx.send(Message::Images);
                }
            }
        });
        cx.notify();
    }
    fn open(&mut self, item: Item, cx: &mut Context<Self>) {
        if self.offline_preview {
            return;
        }
        let Some(destination) = item.destination.clone() else {
            return;
        };
        self.debounce = None;
        self.cancel_collection();
        self.restore_focus = true;
        self.history.push(self.view.clone());
        self.view = View {
            destination,
            item: Some(item),
            page: Page::default(),
            scroll: ScrollHandle::new(),
            filter: self.view.filter.clone(),
            selection: Default::default(),
        };
        self.feedback = None;
        self.request(false, cx);
        self.load_subscription();
    }
    fn open_search_result(&mut self, item: Item, cx: &mut Context<Self>) {
        if self.offline_preview {
            return;
        }
        self.album_unavailable = false;
        if let Some(target) = item.search_target() {
            self.open(target, cx);
            return;
        }
        if item.kind != "Song" {
            return;
        }
        let Some(video) = item.video else { return };
        self.debounce = None;
        self.cancel_collection();
        self.invalidate();
        self.feedback = None;
        self.error = None;
        let generation = self.generation;
        let ticket = self.ticket.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catalog::fetch_cancellable(
                &Destination {
                    api: "next".into(),
                    body: serde_json::json!({"videoId": video}),
                },
                || ticket.load(Ordering::Relaxed) != generation,
            )
            .map(|page| {
                page.items
                    .into_iter()
                    .find(|i| i.video.as_deref() == Some(video.as_str()))
                    .and_then(|mut song| {
                        song.kind = "Song".into();
                        song.search_target()
                    })
            })
            .map_err(|e| e.to_string());
            let _ = tx.send(Message::SongAlbum(generation, result));
        });
        cx.notify();
    }
    fn load_subscription(&mut self) {
        if self.offline_preview {
            return;
        }
        self.subscription = None;
        self.subscription_error = None;
        self.subscription_busy = false;
        if !self.view.item.as_ref().is_some_and(|i| i.kind == "Artist") {
            return;
        }
        let Some(channel) = self.view.destination.body["browseId"]
            .as_str()
            .filter(|v| v.starts_with("UC"))
            .map(str::to_owned)
        else {
            return;
        };
        self.subscription_busy = true;
        let tx = self.tx.clone();
        let g = self.generation;
        std::thread::spawn(move || {
            let _ = tx.send(Message::Subscription(
                g,
                sync::artist_subscription(&channel).map_err(|e| e.to_string()),
            ));
        });
    }
    fn toggle_subscription(&mut self, cx: &mut Context<Self>) {
        if self.offline_preview {
            self.subscription = Some(if self.subscription.as_ref().is_some_and(Option::is_some) {
                None
            } else {
                Some("preview".into())
            });
            cx.notify();
            return;
        }
        if self.subscription_busy {
            return;
        }
        let Some(existing) = self.subscription.clone() else {
            self.load_subscription();
            cx.notify();
            return;
        };
        let Some(channel) = self.view.destination.body["browseId"]
            .as_str()
            .map(str::to_owned)
        else {
            return;
        };
        self.subscription_busy = true;
        let tx = self.tx.clone();
        let g = self.generation;
        std::thread::spawn(move || {
            let _ = tx.send(Message::Subscription(
                g,
                sync::set_artist_subscription(&channel, existing.as_deref())
                    .map_err(|e| e.to_string()),
            ));
        });
        cx.notify();
    }
    fn act(&mut self, action: Action, items: Option<Vec<Item>>, cx: &mut Context<Self>) {
        if self.offline_preview {
            let count = items
                .as_ref()
                .unwrap_or(&self.view.page.items)
                .iter()
                .filter(|item| item.track().is_some())
                .count();
            self.feedback = match action {
                Action::Queue => Some(Feedback::Queued(count)),
                Action::Save => Some(Feedback::Playlist {
                    count,
                    name: "Preview playlist".into(),
                }),
                Action::Library => Some(Feedback::Library(self.view.page.title.clone())),
                _ => None,
            };
            cx.notify();
            return;
        }
        if self.preparing || self.busy || self.save_busy {
            return;
        }
        self.error = None;
        if let Some(items) = items {
            self.apply(action, items, cx);
            return;
        }
        self.preparing = true;
        self.operation += 1;
        self.feedback = None;
        self.cancelled = Arc::new(AtomicBool::new(false));
        let cancelled = self.cancelled.clone();
        let page = self.view.page.clone();
        let tx = self.tx.clone();
        let op = self.operation;
        let shuffle = if matches!(action, Action::Shuffle)
            && self.view.item.as_ref().is_some_and(|i| i.kind == "Artist")
        {
            page.shuffle.clone()
        } else {
            None
        };
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<Vec<Item>> {
                if let Some(d) = shuffle {
                    return Ok(catalog::fetch(&d)?.items);
                }
                catalog::collection(&page, || cancelled.load(Ordering::Relaxed), |_| {})
            })()
            .map_err(|e| e.to_string());
            let _ = tx.send(Message::Prepared(op, action, result));
        });
        cx.notify();
    }
    fn apply(&mut self, action: Action, items: Vec<Item>, cx: &mut Context<Self>) {
        self.feedback = None;
        let tracks: Vec<_> = items.iter().filter_map(Item::track).collect();
        if tracks.is_empty() {
            self.error = Some("No playable tracks are available in this selection.".into());
            cx.notify();
            return;
        }
        match action {
            Action::Play(index) => {
                let index = index.min(tracks.len() - 1);
                self.player.send(Command::SetShuffle(false));
                self.player.send(Command::Queue(tracks, index));
            }
            Action::Shuffle => {
                self.player.send(Command::SetShuffle(true));
                self.player.send(Command::Queue(tracks, 0));
            }
            Action::Queue => {
                let count = tracks.len();
                for t in tracks {
                    self.player.send(Command::Append(t));
                }
                self.feedback = Some(Feedback::Queued(count));
            }
            Action::Library => {
                let name = self.view.page.title.clone();
                let fallback = self
                    .view
                    .item
                    .as_ref()
                    .map(|i| i.title.clone())
                    .unwrap_or_default();
                let key = format!("catalog-library:{}", self.view.destination.body);
                self.save_busy = true;
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let result = (|| -> anyhow::Result<Feedback> {
                        let mut db = db::library()?;
                        if let Some(saved) = db.load(&key)
                            && let Ok(id) = saved.parse::<i64>()
                            && let Some(playlist) = db.playlists()?.into_iter().find(|p| p.id == id)
                        {
                            return Ok(Feedback::AlreadyInLibrary(playlist.name));
                        }
                        let name = if name.is_empty() { fallback } else { name };
                        let id = db.create(&name)?;
                        db.add(id, &tracks)?;
                        db.save(&key, &id.to_string())?;
                        Ok(Feedback::Library(name))
                    })()
                    .map_err(|e| e.to_string());
                    let _ = tx.send(Message::Saved(result));
                });
            }
            Action::Save => {
                self.pending = Some(tracks);
                self.playlists.clear();
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Message::Playlists(
                        db::library()
                            .and_then(|db| db.playlists())
                            .map_err(|e| e.to_string()),
                    ));
                });
            }
        }
        cx.notify();
    }
    fn save(&mut self, playlist: Option<i64>, cx: &mut Context<Self>) {
        if self.offline_preview {
            return;
        }
        if self.save_busy {
            return;
        }
        let Some(tracks) = self.pending.clone() else {
            return;
        };
        let name = self.name.read(cx).value().trim().to_owned();
        if playlist.is_none() && name.is_empty() {
            self.error = Some("Enter a playlist name.".into());
            cx.notify();
            return;
        }
        self.save_busy = true;
        self.error = None;
        let google = self.create_google;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<Feedback> {
                let (id, name) = match playlist {
                    Some(id) => (id, db::library()?.playlist_fields(id)?.title),
                    None => (sync::create(&name, google)?, name),
                };
                sync::add(id, &tracks)?;
                Ok(Feedback::Playlist {
                    count: tracks.len(),
                    name,
                })
            })()
            .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Message::Saved(result));
        });
        cx.notify();
    }
    fn feedback_alert(&self, cx: &Context<Self>) -> Option<Div> {
        let feedback = self.feedback.as_ref()?;
        Some(ui::feedback_alert(
            feedback.message(),
            matches!(feedback, Feedback::AlreadyInLibrary(_)),
            button("dismiss-feedback", "DISMISS", color::surface()).on_click(cx.listener(
                |this, _, _, cx| {
                    this.feedback = None;
                    cx.notify();
                },
            )),
        ))
    }
    fn save_recent(&self) {
        if let Ok(db) = db::library() {
            let _ = db.save(
                "catalog-recent",
                &serde_json::to_string(&self.recent).unwrap_or_default(),
            );
        }
    }
    fn poll(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        while let Ok(message) = self.rx.try_recv() {
            changed = true;
            match message {
                Message::SongAlbum(g, result) if g == self.generation => {
                    self.busy = false;
                    match result {
                        Ok(Some(album)) => self.open(album, cx),
                        Ok(None) => self.album_unavailable = true,
                        Err(error) => {
                            self.feedback = None;
                            self.error = Some(error);
                        }
                    }
                }
                Message::Page(g, append, result) if g == self.generation => {
                    self.busy = false;
                    match result {
                        Ok(mut page) => {
                            if !page.filters.is_empty() {
                                self.filters = page
                                    .filters
                                    .iter()
                                    .filter(|(label, _)| label != "All" && !label.trim().is_empty())
                                    .cloned()
                                    .collect();
                            }
                            // Before the first search, chips have names but no service tokens.
                            // Resolve the chosen category from the first response, then fetch it.
                            if !append
                                && self.view.item.is_none()
                                && self.view.filter != "All"
                                && self.view.destination.body.get("params").is_none()
                                && let Some((_, destination)) = self
                                    .filters
                                    .iter()
                                    .find(|(label, _)| label == &self.view.filter)
                            {
                                let mut destination = destination.clone();
                                destination.body["query"] =
                                    self.input.read(cx).value().trim().into();
                                self.view.destination = destination;
                                self.request(false, cx);
                                continue;
                            }
                            if append {
                                self.view.page.items.append(&mut page.items);
                                self.view.page.more = page.more;
                            } else {
                                self.view.selection.clear();
                                self.view.page = page;
                            }
                            if self.view.item.is_none() {
                                let query = self.input.read(cx).value().trim().to_owned();
                                if !query.is_empty() {
                                    self.recent.retain(|q| q != &query);
                                    self.recent.insert(0, query);
                                    self.recent.truncate(20);
                                    self.save_recent();
                                }
                            }
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                Message::Subscription(g, result) if g == self.generation => {
                    self.subscription_busy = false;
                    match result {
                        Ok(id) => {
                            self.subscription = Some(id);
                            self.subscription_error = None;
                        }
                        Err(e) => {
                            self.subscription = None;
                            self.subscription_error = Some(e);
                        }
                    }
                }
                Message::Prepared(op, action, result) if op == self.operation => {
                    self.preparing = false;
                    if !self.cancelled.load(Ordering::Relaxed) {
                        match result {
                            Ok(items) => self.apply(action, items, cx),
                            Err(e) => self.error = Some(e),
                        }
                    }
                }
                Message::Playlists(result) => match result {
                    Ok(p) => self.playlists = p,
                    Err(e) => self.error = Some(e),
                },
                Message::Saved(result) => {
                    self.save_busy = false;
                    self.pending = None;
                    // Partial writes are never automatically retried.
                    match result {
                        Ok(feedback) => self.feedback = Some(feedback),
                        Err(e) => self.error = Some(e),
                    }
                    cx.emit(Event::Saved);
                    if self.error.is_none() && std::mem::take(&mut self.external_picker) {
                        cx.emit(Event::Close);
                    }
                }
                _ => (),
            }
        }
        if changed {
            cx.notify();
        }
    }
    fn activate_item(&mut self, item: Item, index: usize, cx: &mut Context<Self>) {
        if self.busy || self.preparing {
            return;
        }
        if item.video.is_some() {
            if item.collection_member
                && self.view.item.as_ref().is_some_and(|i| {
                    matches!(
                        i.kind.as_str(),
                        "Album" | "Playlist" | "Podcast" | "Single" | "EP"
                    )
                })
            {
                let offset = self
                    .view
                    .page
                    .items
                    .iter()
                    .take(index)
                    .filter(|i| i.collection_member && i.video.is_some() && !i.unavailable)
                    .count();
                self.act(Action::Play(offset), None, cx);
            } else {
                self.act(Action::Play(0), Some(vec![item]), cx);
            }
        } else {
            self.open(item, cx);
        }
    }
}
fn cover(url: Option<&str>, size: f32) -> AnyElement {
    let frame = div()
        .size(px(size))
        .flex_shrink_0()
        .bg(color::canvas())
        .border(border::strong())
        .border_color(color::ink())
        .overflow_hidden();
    if let Some(path) = url.map(catalog::thumbnail_path).filter(|p| p.is_file()) {
        frame
            .child(
                img(path)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .with_fallback(|| div().child(ui::icon("disc", false)).into_any_element()),
            )
            .into_any_element()
    } else {
        frame
            .flex()
            .items_center()
            .justify_center()
            .child(ui::icon("disc", false))
            .into_any_element()
    }
}
fn action_button(
    id: impl Into<ElementId>,
    label: &str,
    icon: Option<&'static str>,
    primary: bool,
) -> Stateful<Div> {
    button(
        id,
        "",
        if primary {
            color::lime()
        } else {
            color::surface()
        },
    )
    .h(metrics::action())
    .px(metrics::action_padding())
    .gap(metrics::nav_gap())
    .text_size(text::secondary())
    .when_some(icon, |d, name| {
        d.child(ui::icon(name, false).size(metrics::icon()))
    })
    .child(label.to_owned())
}
fn small_icon(id: impl Into<ElementId>, name: &'static str, label: &'static str) -> Stateful<Div> {
    button(id, "", color::white())
        .size(size::row_action())
        .px(crate::theme::layout::zero())
        .mr(crate::theme::layout::zero())
        .mb(crate::theme::layout::zero())
        .shadow(vec![])
        .child(ui::icon(name, false).size(metrics::icon()))
        .when(ui::icon_needs_tooltip(name), |d| {
            d.tooltip(move |w, cx| gpui_component::tooltip::Tooltip::new(label).build(w, cx))
        })
}
fn row_icon(id: impl Into<ElementId>, name: &'static str, label: &'static str) -> Stateful<Div> {
    ui::track_action(small_icon(id, name, label))
        .opacity(0.)
        .group_hover("track-row", |s| s.opacity(1.))
        .focus(|s| s.opacity(1.).shadow(vec![shadow::focus_ring()]))
}
fn time_label(seconds: f64) -> String {
    if seconds <= 0. {
        return String::new();
    }
    let n = seconds as u64;
    if n >= 3600 {
        format!("{}:{:02}:{:02}", n / 3600, n / 60 % 60, n % 60)
    } else {
        format!("{}:{:02}", n / 60, n % 60)
    }
}
fn trailing_metadata(item: &Item) -> String {
    if item.duration > 0. {
        return time_label(item.duration);
    }
    item.subtitle
        .split(['·', '•'])
        .map(str::trim)
        .find(|s| s.ends_with(" tracks") || s.ends_with(" songs") || s.ends_with(" episodes"))
        .unwrap_or("")
        .to_owned()
}
fn description(item: &Item, include_kind: bool) -> String {
    let trailing = trailing_metadata(item);
    let mut parts: Vec<String> = item
        .subtitle
        .split(['·', '•'])
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != item.kind && *s != trailing)
        .map(str::to_owned)
        .collect();
    if include_kind {
        parts.insert(0, item.kind.clone());
    }
    parts.join(" · ")
}
impl CatalogView {
    fn choose_all(&mut self, cx: &mut Context<Self>) {
        self.view.filter = "All".into();
        self.view.destination = catalog::search(self.input.read(cx).value().trim());
        self.view.scroll = ScrollHandle::new();
        self.request(false, cx);
    }
    fn more(&mut self, label: String, destination: Destination, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if destination.body.get("continuation").is_some() {
            self.fetch_request(destination, true, cx);
        } else {
            self.open(
                Item {
                    title: label.trim_start_matches("More ").to_owned(),
                    kind: "Collection".into(),
                    destination: Some(destination),
                    ..Default::default()
                },
                cx,
            );
        }
    }
    /// Reuse the catalog destination picker for a selection from the library.
    pub fn save_tracks(&mut self, tracks: Vec<Track>, cx: &mut Context<Self>) {
        self.external_picker = true;
        self.pending = Some(tracks);
        self.playlists.clear();
        self.error = None;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Message::Playlists(
                db::library()
                    .and_then(|db| db.playlists())
                    .map_err(|e| e.to_string()),
            ));
        });
        cx.notify();
    }

    fn selection_strip(
        &self,
        table: TrackTable,
        sticky: bool,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let selection = self.view.selection.get(&table)?;
        let scope: SharedString = format!("selection-{table:?}").into();
        let items = table.selected_items(&self.view.page.items, selection);
        if items.is_empty() {
            return None;
        }
        let count = items.len();
        let unavailable = items.iter().filter(|i| i.track().is_none()).count();
        let mut actions = div().flex().items_center().gap(space::md());
        for (id, label, action) in [
            ("selection-play", "PLAY", Action::Play(0)),
            ("selection-queue", "QUEUE", Action::Queue),
            ("selection-save", "SAVE TO PLAYLIST", Action::Save),
        ] {
            let items = items.clone();
            actions = actions.child(
                ui::selection_action(
                    id,
                    label,
                    match action {
                        Action::Play(_) => "player-play",
                        Action::Queue => "playlist-add",
                        _ => "plus",
                    },
                    false,
                )
                .when(count == unavailable, |d| {
                    d.bg(color::canvas())
                        .text_color(color::muted())
                        .shadow(vec![])
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    if count > unavailable {
                        this.act(action, Some(items.clone()), cx);
                    }
                })),
            );
        }
        let clear = ui::selection_action("selection-clear", "CLEAR", "x", false).on_click(
            cx.listener(move |this, _, _, cx| {
                if let Some(selection) = this.view.selection.get_mut(&table) {
                    selection.clear();
                }
                cx.notify();
            }),
        );
        let strip = if sticky {
            ui::sticky_selection_bar(self.view.scroll.clone(), unavailable, move |pinned| {
                ui::selection_bar(count, unavailable, pinned, actions, clear)
            })
            .into_any_element()
        } else {
            ui::selection_bar(count, unavailable, false, actions, clear).into_any_element()
        };
        Some(div().id(scope).child(strip).into_any_element())
    }

    fn row(
        &self,
        item: Item,
        index: usize,
        number: Option<usize>,
        album: bool,
        table: TrackTable,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let detail = number.is_some();
        let selection = self.view.selection.get(&table).filter(|_| detail);
        let selected = selection.is_some_and(|s| s.ids.contains(&(index as i64)));
        let selecting = selection.is_some_and(|s| !s.ids.is_empty());
        let playable = item.video.is_some() && !item.unavailable;
        let open = item.clone();
        let play = item.clone();
        let queue = item.clone();
        let save = item.clone();
        let mut lead = div()
            .relative()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .w(if detail {
                metrics::number()
                    - if selected {
                        crate::theme::selection::marker()
                    } else {
                        px(0.)
                    }
            } else {
                metrics::thumbnail()
            })
            .h(if detail {
                metrics::track_row()
            } else {
                metrics::thumbnail()
            });
        if let Some(n) = number {
            lead = lead.child(
                div()
                    .font_family(font::MONO)
                    .font_weight(FontWeight::NORMAL)
                    .child(n.to_string()),
            );
        } else {
            lead = lead.child(cover(
                item.thumbnail.as_deref(),
                f32::from(metrics::thumbnail()),
            ));
        }
        if playable {
            lead = lead.child(
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        row_icon(("catalog-play", index), "player-play", "Play").on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.activate_item(play.clone(), index, cx);
                                cx.stop_propagation();
                            }),
                        ),
                    ),
            );
        }
        let mut row = div()
            .id(("catalog-row", index))
            .group("track-row")
            .tab_index(0)
            .h(if detail {
                metrics::track_row()
            } else {
                metrics::result_row()
            })
            .flex_shrink_0()
            .pr(space::md())
            .when(!detail, |d| {
                d.pl(space::sm()
                    - if selected {
                        crate::theme::selection::marker()
                    } else {
                        px(0.)
                    })
            })
            .border_b(border::hairline())
            .border_color(color::ink())
            .bg(color::surface())
            .hover(move |s| {
                s.bg(if selected {
                    color::canvas()
                } else {
                    color::white()
                })
            })
            .focus(move |s| {
                s.bg(if selected {
                    color::canvas()
                } else {
                    color::white()
                })
                .shadow(vec![shadow::focus_ring()])
            })
            .flex()
            .items_center()
            .gap(space::md())
            .cursor_pointer()
            .child(lead)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(border::strong())
                    .child(
                        div()
                            .text_size(text::body())
                            .truncate()
                            .child(item.title.clone()),
                    )
                    .when(!album, |d| {
                        d.child(
                            div()
                                .text_size(text::secondary())
                                .text_color(color::muted())
                                .truncate()
                                .child(if detail && !item.artist.is_empty() {
                                    item.artist.clone()
                                } else {
                                    description(&item, !detail)
                                }),
                        )
                    }),
            )
            .when(
                !detail || (item.video.is_none() && !item.unavailable),
                |d| {
                    d.on_click(cx.listener(move |this, _, _, cx| {
                        if !detail {
                            this.open_search_result(open.clone(), cx);
                        } else {
                            this.open(open.clone(), cx);
                        }
                    }))
                },
            );
        if detail && (item.video.is_some() || item.unavailable) {
            row = ui::selectable_track_row(
                row,
                selected,
                false,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    let order = table.order(&this.view.page.items);
                    if let Some(position) = order.iter().position(|id| *id == index as i64) {
                        let selection = this.view.selection.entry(table.clone()).or_default();
                        selection.retain(&order);
                        selection.click(&order, position, e.modifiers.control, e.modifiers.shift);
                        cx.notify();
                    }
                }),
            );
        }
        if playable && !selecting {
            let mut actions = div().flex().gap(space::md());
            actions = actions.child(
                row_icon(("result-queue", index), "playlist-add", "Add to queue").on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.act(Action::Queue, Some(vec![queue.clone()]), cx);
                        cx.stop_propagation();
                    }),
                ),
            );
            if !detail {
                actions = actions.child(
                    row_icon(("result-save", index), "plus", "Add to playlist").on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.act(Action::Save, Some(vec![save.clone()]), cx);
                            cx.stop_propagation();
                        }),
                    ),
                );
            }
            row = row.child(actions);
        }
        row.child(
            div()
                .w(if detail {
                    metrics::track_duration()
                } else {
                    metrics::duration()
                })
                .flex_shrink_0()
                .text_right()
                .font_family(font::MONO)
                .font_weight(FontWeight::NORMAL)
                .text_size(text::body())
                .child(if item.unavailable {
                    "UNAVAILABLE".into()
                } else {
                    trailing_metadata(&item)
                }),
        )
    }
    fn details(&self, cx: &Context<Self>) -> Div {
        let item = self.view.item.as_ref().unwrap();
        let page = &self.view.page;
        let artist = item.kind == "Artist";
        let album = matches!(item.kind.as_str(), "Album" | "Single" | "EP");
        let finite = album || matches!(item.kind.as_str(), "Playlist" | "Podcast");
        let title = if page.title.is_empty() {
            &item.title
        } else {
            &page.title
        };
        let mut actions = div().flex().flex_wrap().gap(space::md()).mt(space::md());
        if finite {
            for (id, label, icon, action, primary) in [
                (
                    "collection-play",
                    "PLAY",
                    Some("player-play"),
                    Action::Play(0),
                    true,
                ),
                (
                    "collection-shuffle",
                    "SHUFFLE",
                    Some("arrows-shuffle"),
                    Action::Shuffle,
                    false,
                ),
                (
                    "collection-queue",
                    "QUEUE",
                    Some("playlist-add"),
                    Action::Queue,
                    false,
                ),
                (
                    "collection-save",
                    if item.kind == "Playlist" {
                        "ADD TO LIBRARY"
                    } else {
                        "SAVE TO PLAYLIST"
                    },
                    None,
                    if item.kind == "Playlist" {
                        Action::Library
                    } else {
                        Action::Save
                    },
                    false,
                ),
            ] {
                actions = actions.child(
                    action_button(id, label, icon, primary)
                        .on_click(cx.listener(move |this, _, _, cx| this.act(action, None, cx))),
                );
            }
        }
        if artist {
            actions = actions.child(
                action_button("artist-shuffle", "SHUFFLE", Some("arrows-shuffle"), true)
                    .on_click(cx.listener(|this, _, _, cx| this.act(Action::Shuffle, None, cx))),
            );
            let label = if self.subscription_busy {
                "LOADING…"
            } else if self.subscription.as_ref().is_some_and(Option::is_some) {
                "UNSUBSCRIBE"
            } else {
                "SUBSCRIBE"
            };
            actions = actions.child(
                action_button("artist-subscribe", label, Some("plus"), false)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_subscription(cx))),
            );
        }
        let byline = if !page.byline.is_empty() {
            page.byline.clone()
        } else if !item.artist.is_empty() {
            item.artist.clone()
        } else {
            item.subtitle
                .split(['·', '•'])
                .map(str::trim)
                .filter(|s| !s.is_empty() && *s != item.kind)
                .find(|s| !s.chars().all(|c| c.is_ascii_digit()))
                .unwrap_or("")
                .to_owned()
        };
        let meta = page
            .subtitle
            .split(['·', '•'])
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != item.kind)
            .chain(std::iter::once(page.metadata.as_str()).filter(|s| !s.is_empty()))
            .collect::<Vec<_>>()
            .join(" · ");
        let mut info = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(space::sm())
            .pb(space::xs())
            .child(ui::caps(&item.kind.to_uppercase()))
            .child(
                div()
                    .text_size(text::page_title())
                    .line_height(relative(1.1))
                    .child(title.clone()),
            );
        if !byline.is_empty() {
            info = info.child(div().id("catalog-creator").child(byline.clone()).when_some(
                page.creator.clone(),
                |d, destination| {
                    d.underline()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open(
                                Item {
                                    title: byline.clone(),
                                    kind: "Artist".into(),
                                    destination: Some(destination.clone()),
                                    ..Default::default()
                                },
                                cx,
                            )
                        }))
                },
            ));
        }
        if !meta.is_empty() {
            info = info.child(
                div()
                    .text_size(text::secondary())
                    .text_color(color::muted())
                    .child(meta),
            );
        }
        info = info.child(actions);
        let mut content = div()
            .flex()
            .flex_col()
            .when(finite, |d| d.size_full().min_h_0())
            .p(space::xxl())
            .min_w_0()
            .child(
                div().flex().flex_shrink_0().child(
                    action_button("catalog-back", "BACK", Some("arrow-left"), false)
                        .h(size::control())
                        .mb(space::xl())
                        .on_click(cx.listener(|this, _, _, cx| this.back(cx))),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_end()
                    .gap(space::xxl())
                    .child(div().flex_shrink_0().shadow(shadow::md()).child(cover(
                        page.thumbnail.as_deref().or(item.thumbnail.as_deref()),
                        f32::from(if artist {
                            metrics::portrait()
                        } else {
                            metrics::cover()
                        }),
                    )))
                    .child(info),
            );
        if let Some(error) = &self.subscription_error {
            content = content.child(
                div()
                    .mt(space::md())
                    .text_color(color::muted())
                    .child(error.clone()),
            );
        }
        if let Some(alert) = self.feedback_alert(cx) {
            content = content.child(alert.mt(space::xl()));
        }
        if self.preparing {
            content = content.child(
                button("cancel-loading", "CANCEL LOADING", color::surface()).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.cancel_collection();
                        this.feedback = None;
                        cx.notify();
                    },
                )),
            );
        }
        if self.busy && page.items.is_empty() {
            return content.child(div().mt(space::xxl()).child(ui::caps("LOADING…")));
        }
        if let Some(error) = &self.error {
            content = content.child(self.error_panel(error, cx));
        }
        // Collection headers and selection controls stay outside the track viewport.
        let header = finite.then(|| std::mem::replace(&mut content, div()));
        if finite {
            let mut rows = div();
            let mut n = 0;
            for (index, row) in page
                .items
                .iter()
                .enumerate()
                .filter(|(_, i)| i.collection_member && (i.video.is_some() || i.unavailable))
            {
                n += 1;
                rows = rows.child(self.row(
                    row.clone(),
                    index,
                    Some(n),
                    album,
                    TrackTable::Collection,
                    cx,
                ));
            }
            content = content.child(rows);
        }
        let mut sections: Vec<String> = Vec::new();
        for row in &page.items {
            if (!finite || !row.collection_member) && !sections.contains(&row.category) {
                sections.push(row.category.clone());
            }
        }
        for (section_index, category) in sections.iter().enumerate() {
            let label = if artist && category == "Songs" {
                "TOP SONGS".to_owned()
            } else {
                category.to_uppercase()
            };
            let mut heading = div()
                .flex()
                .items_center()
                .justify_between()
                .h(size::row_action())
                .mt(space::xxl())
                .mb(space::md())
                .child(ui::caps(&label));
            if let Some((label, destination)) = page
                .more
                .iter()
                .find(|(label, d)| {
                    label == &format!("More {category}") && d.body.get("continuation").is_none()
                })
                .cloned()
            {
                heading = heading.child(
                    button(("section-more", section_index), "SEE ALL", color::surface())
                        .h(size::row_action())
                        .text_size(text::secondary())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.more(label.clone(), destination.clone(), cx)
                        })),
                );
            }
            content = content.child(heading);
            let rows: Vec<_> = page
                .items
                .iter()
                .enumerate()
                .filter(|(_, i)| i.category == *category && (!finite || !i.collection_member))
                .collect();
            let cards = rows
                .iter()
                .all(|(_, i)| i.video.is_none() && !i.unavailable);
            if cards {
                let mut grid = div().flex().flex_wrap().gap(space::xl());
                for (index, row) in rows {
                    let open = row.clone();
                    grid =
                        grid.child(
                            div()
                                .id(("catalog-card", index))
                                .tab_index(0)
                                .w(metrics::card())
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(space::sm())
                                .cursor_pointer()
                                .child(div().shadow(shadow::sm()).mb(space::sm()).child(cover(
                                    row.thumbnail.as_deref(),
                                    f32::from(metrics::card()),
                                )))
                                .child(div().truncate().child(row.title.clone()))
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(text::secondary())
                                        .text_color(color::muted())
                                        .child(description(row, true)),
                                )
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.open(open.clone(), cx)),
                                ),
                        );
                }
                content = content.child(grid);
            } else {
                let mut table = div()
                    .border(border::strong())
                    .border_color(color::ink())
                    .bg(color::surface());
                for (n, (index, row)) in rows.into_iter().enumerate() {
                    table = table.child(self.row(
                        row.clone(),
                        index,
                        Some(n + 1),
                        false,
                        TrackTable::Section(category.clone(), finite),
                        cx,
                    ));
                }
                content = content
                    .when_some(
                        self.selection_strip(
                            TrackTable::Section(category.clone(), finite),
                            true,
                            cx,
                        ),
                        |d, bar| d.child(bar),
                    )
                    .child(table);
            }
        }
        for (index, (label, destination)) in page
            .more
            .iter()
            .filter(|(_, d)| d.body.get("continuation").is_some())
            .cloned()
            .enumerate()
        {
            content = content.child(
                button(("detail-more", index), "LOAD MORE", color::surface())
                    .mt(space::lg())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.more(label.clone(), destination.clone(), cx)
                    })),
            );
        }
        if let Some(header) = header {
            header.child(
                div()
                    .mt(space::xxl())
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .when_some(
                        self.selection_strip(TrackTable::Collection, false, cx),
                        |d, bar| d.child(div().flex_shrink_0().child(bar)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .border(border::strong())
                            .border_color(color::ink())
                            .bg(color::surface())
                            .when(
                                self.view
                                    .selection
                                    .get(&TrackTable::Collection)
                                    .is_some_and(|s| !s.ids.is_empty()),
                                |d| d.border_t_0(),
                            )
                            .child(
                                div()
                                    .id("catalog-collection-tracks")
                                    .track_scroll(&self.view.scroll)
                                    .overflow_y_scroll()
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .child(content),
                            )
                            .child(self.scrollbar.clone()),
                    ),
            )
        } else {
            content
        }
    }
    fn error_panel(&self, error: &str, cx: &Context<Self>) -> Div {
        div()
            .border(border::strong())
            .border_color(color::ink())
            .bg(color::surface())
            .p(metrics::state_padding())
            .flex()
            .flex_col()
            .items_start()
            .gap(space::md())
            .child(ui::icon("plug-connected-x", false))
            .child(
                div()
                    .text_size(text::track_title())
                    .font_weight(FontWeight::BOLD)
                    .child("Couldn’t reach YouTube"),
            )
            .child(
                div()
                    .text_size(text::secondary())
                    .child("Check your connection and try again.")
                    .id("error-explanation")
                    .tooltip({
                        let error = error.to_owned();
                        move |w, cx| {
                            gpui_component::tooltip::Tooltip::new(error.clone()).build(w, cx)
                        }
                    }),
            )
            .child(
                action_button("catalog-retry", "RETRY", None, true)
                    .h(size::control())
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some((d, append)) = this.last_request.clone() {
                            this.fetch_request(d, append, cx);
                        }
                    })),
            )
    }
}
impl Render for CatalogView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.restore_focus {
            self.restore_focus = false;
            if self.view.item.is_some() {
                self.focus_handle.focus(window);
            } else {
                self.input.read(cx).focus_handle(cx).focus(window);
            }
        }
        self.scrollbar
            .update(cx, |bar, _| bar.scroll = self.view.scroll.clone());
        let mut root = div()
            .id("catalog-view")
            .track_focus(&self.focus_handle)
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .line_height(relative(1.2));
        if self.view.item.is_some() && self.pending.is_none() {
            if self.view.item.as_ref().is_some_and(|item| {
                matches!(
                    item.kind.as_str(),
                    "Album" | "Single" | "EP" | "Playlist" | "Podcast"
                )
            }) {
                return root.child(self.details(cx)).into_any_element();
            }
            return root
                .flex_row()
                .child(
                    div()
                        .id("catalog-detail")
                        .track_scroll(&self.view.scroll)
                        .overflow_y_scroll()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .child(self.details(cx)),
                )
                .child(self.scrollbar.clone())
                .into_any_element();
        }
        root = root
            .p(if self.standalone {
                space::xxl()
            } else {
                space::lg()
            })
            .gap(space::lg())
            .child(ui::search_input(&self.input, true, cx));
        if self.pending.is_some() {
            if let Some(error) = &self.error {
                root = root.child(div().text_color(color::ink()).child(error.clone()));
            }
        }
        if let Some(tracks) = &self.pending {
            let mut picker = div()
                .id("catalog-picker")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(space::md())
                .child(format!("SAVE {} TRACKS TO PLAYLIST", tracks.len()))
                .child(ui::search_input(&self.picker_filter, false, cx));
            let query = self.picker_filter.read(cx).value().to_lowercase();
            for playlist in self
                .playlists
                .iter()
                .filter(|p| p.name.to_lowercase().contains(&query))
            {
                let id = playlist.id;
                picker = picker.child(
                    button(
                        ("destination", id as u64),
                        format!(
                            "{} · {}",
                            playlist.name,
                            if playlist.google { "YouTube" } else { "Local" }
                        ),
                        color::surface(),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.save(Some(id), cx))),
                );
            }
            picker = picker
                .child(ui::input(&self.name, cx))
                .child(
                    button(
                        "destination-provider",
                        if self.create_google {
                            "YOUTUBE · Switch to local"
                        } else {
                            "LOCAL · Switch to YouTube"
                        },
                        color::surface(),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.create_google = !this.create_google;
                        cx.notify();
                    })),
                )
                .child(
                    button("destination-create", "CREATE AND SAVE", color::surface())
                        .on_click(cx.listener(|this, _, _, cx| this.save(None, cx))),
                )
                .child(
                    button(
                        "destination-cancel",
                        if self.save_busy {
                            "SAVING…"
                        } else {
                            "CANCEL"
                        },
                        color::surface(),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !this.save_busy {
                            this.back(cx);
                        }
                    })),
                );
            return root.child(picker).into_any_element();
        }
        let query = self.input.read(cx).value().trim().to_owned();
        if query.is_empty()
            || (!self.busy && self.error.is_none() && !self.view.page.items.is_empty())
        {
            let mut filters = self.filters.clone();
            let order = [
                "Songs",
                "Videos",
                "Albums",
                "Artists",
                "Community playlists",
                "Featured playlists",
                "Podcasts",
                "Episodes",
                "Profiles",
            ];
            if filters.is_empty() {
                filters = order
                    .iter()
                    .map(|label| (label.to_string(), catalog::search(&query)))
                    .collect();
            }
            filters.sort_by_key(|(name, _)| {
                order
                    .iter()
                    .position(|s| s.eq_ignore_ascii_case(name))
                    .unwrap_or(order.len())
            });
            let mut chips = div().flex().flex_wrap().gap(space::md());
            for (index, (label, mut destination)) in
                std::iter::once(("All".to_owned(), catalog::search(&query)))
                    .chain(filters)
                    .enumerate()
            {
                destination.body["query"] = query.clone().into();
                chips = chips.child(
                    button(
                        ("catalog-filter", index),
                        label.to_uppercase(),
                        if self.view.filter == label {
                            color::ink()
                        } else {
                            color::surface()
                        },
                    )
                    .text_size(text::secondary())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.debounce = None;
                        this.cancel_collection();
                        this.view.filter = label.clone();
                        this.view.destination = destination.clone();
                        this.view.scroll = ScrollHandle::new();
                        if this.input.read(cx).value().trim().is_empty() {
                            cx.notify();
                        } else {
                            this.request(false, cx);
                        }
                    })),
                );
            }
            root = root.child(chips);
        }
        if query.is_empty() {
            if self.recent.is_empty() {
                return root.child(
                    div().border(border::strong()).border_color(color::ink())
                        .bg(color::surface()).p(space::xl()).flex().gap(metrics::state_padding())
                        .items_start()
                        .child(div().size(metrics::field()).flex_shrink_0()
                            .border(border::strong()).border_color(color::ink()).bg(color::canvas())
                            .flex().items_center().justify_center()
                            .child(ui::icon("search", false).size(px(28.))))
                        .child(div().flex_1().min_w_0().max_w(px(560.)).flex().flex_col().gap(space::sm())
                            .child(div().text_size(px(18.)).font_weight(FontWeight::BOLD).child("Find something to play"))
                            .child(div().text_size(text::body()).text_color(color::muted()).line_height(relative(1.4))
                                .child("Search for songs, videos, albums, artists, playlists and episodes. Your recent searches will appear here.")))
                ).into_any_element();
            }
            root = root.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(ui::caps("RECENT SEARCHES"))
                    .child(
                        button("clear-recent-searches", "CLEAR ALL", color::surface())
                            .h(size::row_action())
                            .text_size(text::secondary())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.recent.clear();
                                this.save_recent();
                                cx.notify();
                            })),
                    ),
            );
            if !self.recent.is_empty() {
                let mut history = div()
                    .id("recent-searches")
                    .overflow_y_scroll()
                    .min_h_0()
                    .border(border::strong())
                    .border_color(color::ink())
                    .bg(color::surface());
                for (index, query) in self.recent.clone().into_iter().enumerate() {
                    let remove = query.clone();
                    history = history.child(
                        div()
                            .id(("recent", index))
                            .tab_index(0)
                            .h(metrics::recent_row())
                            .flex()
                            .items_center()
                            .gap(space::md())
                            .pl(space::md())
                            .pr(space::sm())
                            .border_b(border::hairline())
                            .border_color(color::ink())
                            .cursor_pointer()
                            .hover(|s| s.bg(color::white()))
                            .child(ui::icon("clock", false).size(metrics::icon()))
                            .child(div().flex_1().child(query.clone()))
                            .child(
                                small_icon(("remove-recent", index), "x", "Remove from history")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.recent.retain(|q| q != &remove);
                                        this.save_recent();
                                        cx.notify();
                                        cx.stop_propagation();
                                    })),
                            )
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.view.filter = "All".into();
                                this.input
                                    .update(cx, |s, cx| s.set_value(query.clone(), w, cx));
                            })),
                    );
                }
                root = root.child(history);
            }
            return root.into_any_element();
        }
        if self.busy && self.view.page.items.is_empty() {
            let mut skeleton = div()
                .border(border::strong())
                .border_color(color::ink())
                .bg(color::surface());
            for width in [0.62, 0.48, 0.70] {
                skeleton = skeleton.child(
                    div()
                        .h(metrics::result_row())
                        .flex()
                        .items_center()
                        .gap(space::md())
                        .px(space::sm())
                        .border_b(border::hairline())
                        .border_color(color::ink())
                        .child(div().size(metrics::thumbnail()).bg(color::canvas()))
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(space::sm())
                                .child(
                                    div()
                                        .w(relative(width))
                                        .h(metrics::skeleton_title())
                                        .bg(color::canvas()),
                                )
                                .child(
                                    div()
                                        .w(relative(width / 2.))
                                        .h(metrics::skeleton_subtitle())
                                        .bg(color::canvas()),
                                ),
                        )
                        .child(
                            div()
                                .w(metrics::track_duration())
                                .h(space::xl())
                                .bg(color::canvas()),
                        ),
                );
            }
            return root
                .child(ui::caps("SEARCHING…"))
                .child(skeleton)
                .into_any_element();
        }
        if let Some(error) = &self.error {
            return root.child(self.error_panel(error, cx)).into_any_element();
        }
        if self.view.page.items.is_empty() {
            let filter = if self.view.filter == "All" {
                "results".into()
            } else {
                self.view.filter.to_lowercase()
            };
            let icon = match self.view.filter.as_str() {
                "Videos" => "video",
                "Artists" | "Profiles" => "user",
                "Playlists" | "Community playlists" | "Featured playlists" => "playlist",
                "Episodes" | "Podcasts" => "microphone",
                _ => "search",
            };
            return root
                .child(
                    div()
                        .border(border::strong())
                        .border_color(color::ink())
                        .bg(color::surface())
                        .p(metrics::state_padding())
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap(space::md())
                        .child(ui::icon(icon, false))
                        .child(
                            div()
                                .text_size(text::track_title())
                                .font_weight(FontWeight::BOLD)
                                .child(format!("No {filter} for “{query}”")),
                        )
                        .child(
                            div()
                                .text_size(text::secondary())
                                .child("Check the spelling, or search all types."),
                        )
                        .child(
                            action_button("search-all", "SEARCH ALL TYPES", None, true)
                                .h(size::control())
                                .on_click(cx.listener(|this, _, _, cx| this.choose_all(cx))),
                        ),
                )
                .into_any_element();
        }
        root = root
            .child(ui::caps(&format!("RESULTS FOR “{}”", query.to_uppercase())).mt(space::xs()));
        if let Some(alert) = self.feedback_alert(cx) {
            root = root.child(alert);
        }
        if self.album_unavailable {
            root = root.child(ui::feedback_alert(
                "Album information is unavailable for this song.",
                true,
                button("dismiss-album-notice", "DISMISS", color::surface()).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.album_unavailable = false;
                        cx.notify();
                    },
                )),
            ));
        }
        let mut list = div()
            .id("catalog-results")
            .track_scroll(&self.view.scroll)
            .overflow_y_scroll()
            .flex_1()
            .min_w_0()
            .h_full();
        for (index, item) in self.view.page.items.iter().cloned().enumerate() {
            list = list.child(self.row(item, index, None, false, TrackTable::Results, cx));
        }
        for (index, (label, destination)) in self.view.page.more.iter().cloned().enumerate() {
            list = list.child(
                button(
                    ("catalog-more", index),
                    label.to_uppercase(),
                    color::surface(),
                )
                .m(space::md())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.more(label.clone(), destination.clone(), cx)
                })),
            );
        }
        root.child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .border(border::strong())
                .border_color(color::ink())
                .bg(color::surface())
                .child(list)
                .child(self.scrollbar.clone()),
        )
        .into_any_element()
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::{Item, Page, TrackTable, View, catalog};
    use gpui::px;

    #[test]
    fn restores_page_and_scroll_without_transient_selection() {
        let saved = crate::view_state::CatalogPageState {
            destination: catalog::search("Bach"),
            filter: "Albums".into(),
            scroll: -240.,
            page: Page {
                title: "Results".into(),
                items: vec![Item {
                    title: "Goldberg Variations".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            ..Default::default()
        };
        let mut view = View::restore(saved.clone());
        view.selection
            .insert(TrackTable::Collection, Default::default());
        let json = serde_json::to_string(&view.snapshot()).unwrap();
        let restored = View::restore(serde_json::from_str(&json).unwrap());
        assert_eq!(restored.destination, saved.destination);
        assert_eq!(restored.filter, "Albums");
        assert_eq!(restored.page.items[0].title, "Goldberg Variations");
        assert_eq!(restored.scroll.offset().y, px(-240.));
        assert!(restored.selection.is_empty());
    }
}

#[cfg(test)]
mod presentation_tests {
    use super::{description, time_label, trailing_metadata};
    use crate::catalog::Item;
    #[test]
    fn result_metadata_keeps_category_in_description_and_duration_in_trailing_column() {
        let item = Item {
            kind: "Song".into(),
            subtitle: "Song • Karl Richter • 5:12 • Bach: Motets".into(),
            duration: 312.,
            ..Default::default()
        };
        assert_eq!(
            description(&item, true),
            "Song · Karl Richter · Bach: Motets"
        );
        assert_eq!(trailing_metadata(&item), "5:12");
        assert_eq!(time_label(3721.), "1:02:01");
    }
    #[test]
    fn collection_count_is_not_repeated_in_description() {
        let item = Item {
            kind: "Playlist".into(),
            subtitle: "Playlist • Classical Focus • 214 tracks".into(),
            ..Default::default()
        };
        assert_eq!(description(&item, true), "Playlist · Classical Focus");
        assert_eq!(trailing_metadata(&item), "214 tracks");
    }
}

#[cfg(test)]
mod selection_tests {
    use super::TrackTable;
    use crate::catalog::Item;
    #[test]
    fn bulk_actions_keep_display_order_duplicates_and_skip_unavailable() {
        let song = |title: &str, unavailable| Item {
            title: title.into(),
            video: Some("same-video".into()),
            unavailable,
            collection_member: true,
            ..Default::default()
        };
        let items = vec![
            song("First", false),
            song("Missing", true),
            song("Repeat", false),
            song("Not selected", false),
        ];
        let mut selection = crate::track_selection::TrackSelection::default();
        selection.ids.extend([2, 1, 0, 99]);
        let selected = TrackTable::Collection.selected_items(&items, &selection);
        assert_eq!(selected.len(), 3);
        let playable: Vec<_> = selected.iter().filter_map(Item::track).collect();
        assert_eq!(
            playable
                .iter()
                .map(|t| t.title.as_str())
                .collect::<Vec<_>>(),
            ["First", "Repeat"]
        );
        assert_eq!(playable[0].url, playable[1].url);
        selection.ids = [1].into();
        assert!(
            TrackTable::Collection
                .selected_items(&items, &selection)
                .iter()
                .filter_map(Item::track)
                .next()
                .is_none()
        );
        selection.clear();
        assert!(
            TrackTable::Collection
                .selected_items(&items, &selection)
                .is_empty()
        );
    }
    #[test]
    fn ranges_follow_visible_tracks_and_keep_sections_separate() {
        let song = |category: &str, collection_member| Item {
            video: Some("same-video".into()),
            category: category.into(),
            collection_member,
            ..Default::default()
        };
        let items = vec![
            song("Songs", false),
            Item::default(),
            song("Videos", false),
            song("Songs", false),
            song("", true),
        ];
        assert!(TrackTable::Results.order(&items).is_empty());
        assert_eq!(
            TrackTable::Section("Songs".into(), false).order(&items),
            [0, 3]
        );
        assert_eq!(TrackTable::Collection.order(&items), [4]);
        let mut selection = crate::track_selection::TrackSelection::default();
        let order = TrackTable::Section("Songs".into(), false).order(&items);
        selection.click(&order, 0, false, false);
        selection.click(&order, 1, false, true);
        assert_eq!(selection.ids.into_iter().collect::<Vec<_>>(), [0, 3]);
    }
}
