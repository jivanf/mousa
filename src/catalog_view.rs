//! Catalog navigation is independent of playlist selection and playback state.
use crate::{
    catalog::{self, Destination, Item, Page},
    db::{self, Playlist, Track},
    player::{Command, Player},
    sync,
    theme::{color, space, text},
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
}
#[derive(Clone, Copy)]
enum Action {
    Play(usize),
    Shuffle,
    Queue,
    Save,
}
enum Message {
    Page(u64, bool, Result<Page, String>),
    Images,
    Subscription(u64, Result<Option<String>, String>),
    Prepared(u64, Action, Result<Vec<Item>, String>),
    Progress(u64, usize),
    Playlists(Result<Vec<Playlist>, String>),
    Saved(Result<String, String>),
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
    status: String,
    debounce: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
    player: Player,
    tx: Sender<Message>,
    rx: Receiver<Message>,
    pending: Option<Vec<Track>>,
    playlists: Vec<Playlist>,
    save_busy: bool,
    create_google: bool,
    subscription: Option<Option<String>>,
    subscription_busy: bool,
    subscription_error: Option<String>,
    cancelled: Arc<AtomicBool>,
    preparing: bool,
    last_request: Option<(Destination, bool)>,
}
impl EventEmitter<Event> for CatalogView {}
impl CatalogView {
    pub fn new(player: Player, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search songs, albums, artists…"));
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("New playlist name"));
        let picker_filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter destination playlists…"));
        let (tx, rx) = mpsc::channel();
        let recent = db::library()
            .ok()
            .and_then(|db| db.load("catalog-recent"))
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default();
        let subscriptions = vec![
            cx.subscribe(&input, |this, _, e, cx| {
                if matches!(e, InputEvent::Change) {
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
        Self {
            input,
            name,
            picker_filter,
            view: View {
                page: Page::default(),
                destination: catalog::search(""),
                item: None,
                scroll: ScrollHandle::new(),
                filter: "All".into(),
            },
            history: vec![],
            filters: vec![],
            recent,
            generation: 0,
            ticket: Arc::new(AtomicU64::new(0)),
            operation: 0,
            busy: false,
            error: None,
            status: String::new(),
            debounce: None,
            _subscriptions: subscriptions,
            player,
            tx,
            rx,
            pending: None,
            playlists: vec![],
            save_busy: false,
            create_google: true,
            subscription: None,
            subscription_busy: false,
            subscription_error: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            preparing: false,
            last_request: None,
        }
    }
    pub fn focus(&self, window: &mut Window, cx: &App) {
        self.input.read(cx).focus_handle(cx).focus(window);
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
        if self.pending.take().is_some() {
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
        self.status.clear();
        self.history.clear();
        self.view.item = None;
        let query = self.input.read(cx).value().trim().to_owned();
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
        let Some(destination) = item.destination.clone() else {
            return;
        };
        self.debounce = None;
        self.cancel_collection();
        self.history.push(self.view.clone());
        self.view = View {
            destination,
            item: Some(item),
            page: Page::default(),
            scroll: ScrollHandle::new(),
            filter: self.view.filter.clone(),
        };
        self.status.clear();
        self.request(false, cx);
        self.load_subscription();
    }
    fn load_subscription(&mut self) {
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
        self.status = "Loading collection…".into();
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
                catalog::collection(
                    &page,
                    || cancelled.load(Ordering::Relaxed),
                    |n| {
                        let _ = tx.send(Message::Progress(op, n));
                    },
                )
            })()
            .map_err(|e| e.to_string());
            let _ = tx.send(Message::Prepared(op, action, result));
        });
        cx.notify();
    }
    fn apply(&mut self, action: Action, items: Vec<Item>, cx: &mut Context<Self>) {
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
                self.status = "Playing selection".into();
            }
            Action::Shuffle => {
                self.player.send(Command::SetShuffle(true));
                self.player.send(Command::Queue(tracks, 0));
                self.status = "Playing shuffled collection".into();
            }
            Action::Queue => {
                let count = tracks.len();
                for t in tracks {
                    self.player.send(Command::Append(t));
                }
                self.status = format!("Added {count} tracks to queue");
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
            let result = (|| -> anyhow::Result<String> {
                let id = match playlist {
                    Some(id) => id,
                    None => sync::create(&name, google)?,
                };
                sync::add(id, &tracks)?;
                Ok(format!("Added {} tracks to playlist", tracks.len()))
            })()
            .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Message::Saved(result));
        });
        cx.notify();
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
                Message::Page(g, append, result) if g == self.generation => {
                    self.busy = false;
                    match result {
                        Ok(mut page) => {
                            if !page.filters.is_empty() {
                                self.filters = page
                                    .filters
                                    .iter()
                                    .filter(|(label, _)| label != "All")
                                    .cloned()
                                    .collect();
                            }
                            if append {
                                self.view.page.items.append(&mut page.items);
                                self.view.page.more = page.more;
                            } else {
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
                Message::Progress(op, n) if op == self.operation && self.preparing => {
                    self.status = format!("Loading collection… {n} items")
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
                        Ok(s) => self.status = s,
                        Err(e) => self.error = Some(e),
                    }
                    cx.emit(Event::Saved);
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
        .border_1()
        .border_color(color::ink())
        .overflow_hidden();
    if let Some(path) = url.map(catalog::thumbnail_path).filter(|p| p.is_file()) {
        frame
            .child(
                img(path)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .with_fallback(|| div().child("♪").into_any_element()),
            )
            .into_any_element()
    } else {
        frame
            .flex()
            .items_center()
            .justify_center()
            .child("♪")
            .into_any_element()
    }
}
impl Render for CatalogView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .size_full()
            .flex()
            .flex_col()
            .gap(space::md())
            .p(space::xl())
            .child(ui::input(&self.input, cx));
        if let Some(error) = &self.error {
            root = root.child(
                div()
                    .p(space::md())
                    .bg(color::pink())
                    .child(error.clone())
                    .child(button("catalog-retry", "RETRY", color::surface()).on_click(
                        cx.listener(|this, _, _, cx| {
                            if this.pending.is_none()
                                && let Some((d, append)) = this.last_request.clone()
                            {
                                this.fetch_request(d, append, cx);
                            }
                        }),
                    )),
            );
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
                .child(ui::input(&self.picker_filter, cx));
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
                            this.pending = None;
                            cx.notify();
                        }
                    })),
                );
            return root.child(picker).into_any_element();
        }
        if self.view.item.is_none() {
            let mut filters = div().flex().flex_wrap().gap(space::sm());
            for (index, (label, mut destination)) in std::iter::once((
                "All".to_owned(),
                catalog::search(self.input.read(cx).value().trim()),
            ))
            .chain(self.filters.clone())
            .enumerate()
            {
                destination.body["query"] = self.input.read(cx).value().trim().to_owned().into();
                let active = self.view.filter == label;
                filters = filters.child(
                    button(
                        ("catalog-filter", index),
                        label.to_uppercase(),
                        if active {
                            color::ink()
                        } else {
                            color::surface()
                        },
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.input.read(cx).value().trim().is_empty() {
                            return;
                        }
                        this.debounce = None;
                        this.cancel_collection();
                        this.view.filter = label.clone();
                        this.view.destination = destination.clone();
                        this.view.scroll = ScrollHandle::new();
                        this.request(false, cx);
                    })),
                );
            }
            root = root.child(filters);
        } else {
            let item = self.view.item.as_ref().unwrap();
            let title = if self.view.page.title.is_empty() {
                item.title.clone()
            } else {
                self.view.page.title.clone()
            };
            let subtitle = if self.view.page.subtitle.is_empty() {
                item.subtitle.clone()
            } else {
                self.view.page.subtitle.clone()
            };
            root = root.child(
                div()
                    .flex()
                    .gap(space::md())
                    .child(cover(
                        self.view
                            .page
                            .thumbnail
                            .as_deref()
                            .or(item.thumbnail.as_deref()),
                        90.,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .child(item.kind.to_uppercase())
                            .child(div().text_size(text::page_title()).child(title))
                            .child(subtitle),
                    ),
            );
            let finite = matches!(
                item.kind.as_str(),
                "Album" | "Single" | "EP" | "Playlist" | "Podcast"
            );
            let artist = item.kind == "Artist";
            let mut actions = div().flex().flex_wrap().gap(space::sm());
            if finite {
                for (id, label, action) in [
                    ("collection-play", "PLAY", Action::Play(0)),
                    ("collection-shuffle", "SHUFFLE", Action::Shuffle),
                    ("collection-queue", "QUEUE", Action::Queue),
                    ("collection-save", "SAVE TO PLAYLIST", Action::Save),
                ] {
                    actions =
                        actions.child(button(id, label, color::surface()).on_click(
                            cx.listener(move |this, _, _, cx| this.act(action, None, cx)),
                        ));
                }
            }
            if artist {
                if self.view.page.shuffle.is_some() {
                    actions = actions.child(
                        button("artist-shuffle", "SHUFFLE", color::surface()).on_click(
                            cx.listener(|this, _, _, cx| this.act(Action::Shuffle, None, cx)),
                        ),
                    );
                }
                let label = if self.subscription_busy {
                    "LOADING SUBSCRIPTION…"
                } else if self.subscription.as_ref().is_some_and(Option::is_some) {
                    "UNSUBSCRIBE"
                } else if self.subscription.is_some() {
                    "SUBSCRIBE"
                } else {
                    "RETRY SUBSCRIPTION"
                };
                actions = actions.child(
                    button("artist-subscribe", label, color::surface())
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_subscription(cx))),
                );
            }
            root = root.child(actions);
            if let Some(error) = &self.subscription_error {
                root = root.child(div().text_size(text::secondary()).child(error.clone()));
            }
        }
        if !self.status.is_empty() {
            root = root.child(self.status.clone());
        }
        if self.preparing {
            root = root.child(
                button("collection-cancel", "CANCEL LOADING", color::surface()).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.cancel_collection();
                        this.status = "Cancelled".into();
                        cx.notify();
                    }),
                ),
            );
        }
        if self.busy {
            root = root.child("SEARCHING / LOADING…");
        }
        let mut list = div()
            .id("catalog-results")
            .track_scroll(&self.view.scroll)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .bg(color::surface())
            .border_1()
            .border_color(color::ink())
            .flex()
            .flex_col();
        if self.input.read(cx).value().trim().is_empty() && self.view.item.is_none() {
            list = list.child("RECENT SEARCHES");
            for (index, query) in self.recent.clone().into_iter().enumerate() {
                let remove = query.clone();
                list =
                    list.child(
                        div()
                            .flex()
                            .gap(space::sm())
                            .child(
                                button(("recent", index), query.clone(), color::surface())
                                    .on_click(cx.listener(move |this, _, w, cx| {
                                        this.view.filter = "All".into();
                                        this.input
                                            .update(cx, |s, cx| s.set_value(query.clone(), w, cx));
                                    })),
                            )
                            .child(
                                button(("remove-recent", index), "REMOVE", color::surface())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.recent.retain(|q| q != &remove);
                                        this.save_recent();
                                        cx.notify();
                                    })),
                            ),
                    );
            }
        } else if !self.busy && self.view.page.items.is_empty() && self.error.is_none() {
            list = list.child("No results. Check the spelling or search all types.");
        }
        let mut category = String::new();
        for (index, item) in self.view.page.items.iter().cloned().enumerate() {
            if !item.category.is_empty() && item.category != category {
                category = item.category.clone();
                list = list.child(div().py(space::md()).child(ui::caps(&category)));
            }
            let open = item.clone();
            let queue = item.clone();
            let save = item.clone();
            let playable = item.video.is_some() && !item.unavailable;
            let mut row = div()
                .id(("catalog-row", index))
                .p(space::md())
                .border_b_1()
                .border_color(color::ink())
                .flex()
                .items_center()
                .gap(space::md())
                .child(cover(
                    item.thumbnail.as_deref().or_else(|| {
                        self.view
                            .item
                            .as_ref()
                            .filter(|parent| parent.kind == "Album")
                            .and_then(|parent| {
                                self.view
                                    .page
                                    .thumbnail
                                    .as_deref()
                                    .or(parent.thumbnail.as_deref())
                            })
                    }),
                    52.,
                ))
                .child(
                    div()
                        .id(("catalog-open", index))
                        .tab_index(0)
                        .cursor_pointer()
                        .flex_1()
                        .min_w_0()
                        .child(item.title.clone())
                        .child(
                            div()
                                .text_size(text::secondary())
                                .child(item.subtitle.clone()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.activate_item(open.clone(), index, cx)
                        })),
                )
                .child(
                    div()
                        .text_size(text::secondary())
                        .child(item.kind.to_uppercase()),
                );
            if playable {
                row = row
                    .child(
                        button(("result-queue", index), "+ QUEUE", color::surface()).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.act(Action::Queue, Some(vec![queue.clone()]), cx)
                            }),
                        ),
                    )
                    .child(
                        button(("result-save", index), "+ PLAYLIST", color::surface()).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.act(Action::Save, Some(vec![save.clone()]), cx)
                            }),
                        ),
                    );
            }
            list = list.child(row);
        }
        for (index, (label, destination)) in self.view.page.more.clone().into_iter().enumerate() {
            list = list.child(
                button(("catalog-more", index), label.clone(), color::surface()).on_click(
                    cx.listener(move |this, _, _, cx| {
                        if this.busy {
                            return;
                        }
                        if destination.body.get("continuation").is_some() {
                            this.fetch_request(destination.clone(), true, cx);
                        } else {
                            this.open(
                                Item {
                                    title: label.clone(),
                                    kind: "Collection".into(),
                                    destination: Some(destination.clone()),
                                    ..Default::default()
                                },
                                cx,
                            );
                        }
                    }),
                ),
            );
        }
        root.child(list).into_any_element()
    }
}
