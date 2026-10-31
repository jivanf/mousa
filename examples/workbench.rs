//! Native story browser with isolated preview data.
#![windows_subsystem = "windows"]
#[allow(dead_code)]
#[path = "catalog_gallery.rs"]
mod catalog;
#[path = "support/collection_preview.rs"]
mod collection_preview;
#[allow(dead_code)]
#[path = "gallery.rs"]
mod components;
#[allow(dead_code)]
#[path = "selection_gallery.rs"]
mod selection;

use gpui::{prelude::*, *};
use gpui_component::{
    Root,
    input::{InputEvent, InputState},
};
use mousa::{
    catalog_view::CatalogView,
    player::{Player, Snapshot},
    playlist_editor::PlaylistEditor,
    theme::{border, color, font, layout, size as control_size, space, text},
    ui,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
};

const STORIES: &[(&str, &str)] = &[
    ("Components", "controls"),
    ("Components", "player"),
    ("Components", "landing"),
    ("Components", "accounts"),
    ("Components", "scrollbar"),
    ("Components", "scrollbar-short"),
    ("Components", "selection"),
    ("Catalog", "search"),
    ("Catalog", "album"),
    ("Catalog", "playlist"),
    ("Catalog", "artist"),
    ("Catalog", "first-use"),
    ("Catalog", "empty"),
    ("Catalog", "loading"),
    ("Catalog", "no-results"),
    ("Catalog", "error"),
    ("Catalog", "selection"),
    ("Catalog", "selection-single"),
    ("Catalog", "selection-unavailable"),
    ("Catalog", "alert-success"),
    ("Catalog", "alert-info"),
    ("Playlist editor", "pristine"),
    ("Playlist editor", "dirty"),
    ("Playlist editor", "saving"),
    ("Playlist editor", "error"),
    ("Playlist editor", "empty"),
    ("Playlist editor", "discard"),
    ("Playlist editor", "local"),
    // Append stories to preserve the numeric identities saved by older workbenches.
    ("Catalog", "channel"),
    ("Catalog", "channel-loading"),
    ("Catalog", "channel-error"),
    ("Catalog", "channel-empty"),
    ("Catalog", "channel-search-videos"),
    ("Catalog", "channel-search-playlists"),
    ("Catalog", "channel-search-mixed"),
    ("Catalog", "channel-no-results"),
    ("Catalog", "channel-selection-single"),
    ("Catalog", "channel-selection"),
    ("Catalog", "channel-selection-unavailable"),
    ("Catalog", "channel-playing"),
    ("Catalog", "channel-feedback"),
    ("Catalog", "channel-save-picker"),
    ("Catalog", "album-search"),
    ("Catalog", "album-no-matches"),
    ("Catalog", "album-empty"),
    ("Catalog", "album-loading"),
    ("Catalog", "album-error"),
    ("Catalog", "playlist-search"),
    ("Catalog", "playlist-no-matches"),
    ("Catalog", "playlist-empty"),
    ("Library", "library-playlist"),
    ("Library", "library-playlist-search"),
    ("Library", "library-playlist-no-matches"),
    ("Library", "library-playlist-empty"),
    ("Library", "library-playlist-error"),
    ("Library", "library-playlist-no-artwork"),
];

#[derive(Default, Serialize, Deserialize)]
struct Preferences {
    story: usize,
    compact: bool,
    filter: String,
}
struct Workbench {
    preferences: Preferences,
    path: PathBuf,
    search: Entity<InputState>,
    preview: AnyView,
    _subscription: Subscription,
}
struct EditorPreview {
    editor: Option<Entity<PlaylistEditor>>,
    _subscription: Subscription,
}
impl Render for EditorPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .p(space::xxl())
                    .child("Playlist editor preview · Reset to reopen"),
            )
            .children(self.editor.clone())
    }
}
impl Workbench {
    fn preview(index: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyView {
        let (group, state) = STORIES[index];
        match group {
            "Library" => cx
                .new(|cx| collection_preview::Preview::new(state, window, cx))
                .into(),
            "Catalog" => {
                let (page, item) = catalog::sample(state);
                let (tx, _) = mpsc::channel();
                let player = Player {
                    tx,
                    state: Arc::new(Mutex::new(Snapshot::default())),
                };
                cx.new(|cx| {
                    let mut view = CatalogView::preview(player, page, item, state, window, cx);
                    view.isolate_preview();
                    view
                })
                .into()
            }
            "Playlist editor" => {
                let editor = cx.new(|cx| PlaylistEditor::preview(state, window, cx));
                cx.new(|cx| {
                    let subscription = cx.subscribe(
                        &editor,
                        |this: &mut EditorPreview, _, _: &mousa::playlist_editor::Event, cx| {
                            this.editor = None;
                            cx.notify();
                        },
                    );
                    EditorPreview {
                        editor: Some(editor),
                        _subscription: subscription,
                    }
                })
                .into()
            }
            _ if state == "selection" => cx.new(|_| selection::Gallery).into(),
            _ => cx
                .new(|cx| components::Gallery::new(state, window, cx))
                .into(),
        }
    }
    fn save(&self) {
        if let Ok(json) = serde_json::to_vec(&self.preferences) {
            if let Err(error) = std::fs::write(&self.path, json) {
                eprintln!("Could not save workbench preferences: {error}");
            }
        }
    }
    fn new(path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut preferences: Preferences = std::fs::read(&path)
            .ok()
            .and_then(|s| serde_json::from_slice(&s).ok())
            .unwrap_or_default();
        preferences.story = preferences.story.min(STORIES.len() - 1);
        let search = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Find a story…");
            input.set_value(preferences.filter.clone(), window, cx);
            input
        });
        let subscription = cx.subscribe(&search, |this, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                this.preferences.filter = this.search.read(cx).value().to_string();
                this.save();
                cx.notify();
            }
        });
        let preview = Self::preview(preferences.story, window, cx);
        Self {
            preferences,
            path,
            search,
            preview,
            _subscription: subscription,
        }
    }
}
impl Render for Workbench {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let filter = self.preferences.filter.to_lowercase();
        let mut stories = div()
            .id("stories")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(space::sm())
            .p(space::sm());
        let mut previous_group = "";
        let mut ordered_stories: Vec<_> = STORIES.iter().enumerate().collect();
        ordered_stories.sort_by_key(|(_, (group, _))| match *group {
            "Components" => 0,
            "Catalog" => 1,
            _ => 2,
        });
        for (index, &(group, state)) in ordered_stories {
            if !format!("{group} {state}").to_lowercase().contains(&filter) {
                continue;
            }
            if group != previous_group {
                stories = stories.child(ui::caps(group).mt(space::lg()));
                previous_group = group;
            }
            stories = stories.child(
                ui::button(
                    ("story", index),
                    state,
                    if self.preferences.story == index {
                        color::ink()
                    } else {
                        color::surface()
                    },
                )
                .shadow(vec![])
                .justify_start()
                .text_size(text::secondary())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.preferences.story = index;
                    this.preview = Self::preview(index, window, cx);
                    this.save();
                    cx.notify();
                })),
            );
        }
        let (group, state) = STORIES[self.preferences.story];
        div()
            .size_full()
            .flex()
            .bg(color::canvas())
            .text_color(color::ink())
            .font_family(font::SANS)
            .font_weight(FontWeight::MEDIUM)
            .text_size(text::body())
            .child(
                div()
                    .w(control_size::sidebar())
                    .h_full()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .p(space::sm())
                    .border_r(border::strong())
                    .border_color(color::ink())
                    .child(ui::caps("Mousa workbench").p(space::sm()))
                    .child(ui::search_input(&self.search, false, cx))
                    .child(stories),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(space::md())
                            .p(space::lg())
                            .child(div().flex_1().child(format!("{group} / {state}")))
                            .child(
                                ui::button(
                                    "viewport",
                                    if self.preferences.compact {
                                        "COMPACT"
                                    } else {
                                        "FILL"
                                    },
                                    color::surface(),
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.preferences.compact = !this.preferences.compact;
                                        this.save();
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(ui::button("reset", "RESET", color::surface()).on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.preview =
                                        Self::preview(this.preferences.story, window, cx);
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(
                        div()
                            .id("canvas-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_scroll()
                            .child(
                                div()
                                    .relative()
                                    .h_full()
                                    .w_full()
                                    .when(self.preferences.compact, |d| {
                                        d.w(layout::min_width()).flex_shrink_0()
                                    })
                                    .child(self.preview.clone()),
                            ),
                    ),
            )
    }
}
fn main() {
    // Require explicit isolation even for direct cargo invocations. Set before GPUI starts threads.
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/workbench/data");
    std::fs::create_dir_all(&data).expect("create isolated workbench data");
    unsafe {
        std::env::set_var("MOUSA_DATA_DIR", &data);
    }
    let path = data.parent().unwrap().join("preferences.json");
    Application::new().with_assets(ui::Assets).run(move |cx| {
        ui::init(cx);
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
                    size(
                        layout::viewport_width() + control_size::sidebar(),
                        layout::viewport_height(),
                    ),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Mousa Workbench".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |window, cx| {
                let view = cx.new(|cx| Workbench::new(path, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("open workbench");
        cx.activate(true);
    });
}
