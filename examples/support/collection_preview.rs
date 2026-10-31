//! Isolated library collection layout and interactions for the Workbench.
use gpui::{prelude::*, *};
use gpui_component::input::{InputEvent, InputState};
use mousa::{
    catalog_view::action_button,
    playlist_editor::PlaylistEditor,
    theme::{border, catalog, color, space, text},
    ui,
};

pub struct Preview {
    input: Entity<InputState>,
    state: String,
    selected: Option<usize>,
    playing: Option<usize>,
    confirm_delete: bool,
    editor: Option<Entity<PlaylistEditor>>,
    subscriptions: Vec<Subscription>,
}
impl Preview {
    pub fn new(state: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search tracks…"));
        input.update(cx, |s, cx| {
            s.set_value(
                if state.ends_with("no-matches") {
                    "missing"
                } else if state.ends_with("search") {
                    "aria"
                } else {
                    ""
                },
                window,
                cx,
            )
        });
        let subscription = cx.subscribe(&input, |this, _, e, cx| {
            if matches!(e, InputEvent::Change) {
                this.selected = None;
                cx.notify();
            }
        });
        Self {
            input,
            state: state.into(),
            selected: None,
            playing: None,
            confirm_delete: false,
            editor: None,
            subscriptions: vec![subscription],
        }
    }
}
impl Render for Preview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.input.read(cx).value().trim().to_lowercase();
        let names = [
            "Aria: Hier ist das rechte Osterlamm",
            "Choral: Wir essen und leben wohl",
            "Sinfonia",
            "Aria: Es ist vollbracht",
        ];
        let mut rows = div()
            .id("library-preview-rows")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        let mut matches = 0;
        if !self.state.ends_with("empty") {
            for (i, name) in names.iter().enumerate().filter(|(_, name)| {
                name.to_lowercase().contains(&query) || "karl richter".contains(&query)
            }) {
                matches += 1;
                let row = ui::track_row(i, self.playing == Some(i))
                    .child(
                        ui::button(("preview-play", i), "PLAY", color::surface()).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.playing = Some(i);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child((*name).to_owned())
                            .child(div().text_size(text::secondary()).child("Karl Richter")),
                    )
                    .child("4:20");
                rows = rows.child(ui::selectable_track_row(
                    row,
                    self.selected == Some(i),
                    self.playing == Some(i),
                    cx.listener(move |this, _, _, cx| {
                        this.selected = Some(i);
                        cx.notify();
                    }),
                ));
            }
        }
        if matches == 0 {
            rows = rows.child(div().p(space::xl()).child(if query.is_empty() {
                "Start with something you love."
            } else {
                "No matching tracks."
            }));
        }
        let actions = div()
            .child(
                action_button("preview-edit", "EDIT", None, false).on_click(cx.listener(
                    |this, _, window, cx| {
                        let editor = cx.new(|cx| PlaylistEditor::preview("local", window, cx));
                        this.subscriptions.push(cx.subscribe(
                            &editor,
                            |this, _, _: &mousa::playlist_editor::Event, cx| {
                                this.editor = None;
                                cx.notify();
                            },
                        ));
                        this.editor = Some(editor);
                        cx.notify();
                    },
                )),
            )
            .child(
                action_button("preview-delete", "DELETE", None, false)
                    .bg(color::pink())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.confirm_delete = true;
                        cx.notify();
                    })),
            )
            .child(
                action_button("preview-refresh", "REFRESH", None, false).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.state = "library-playlist".into();
                        cx.notify();
                    },
                )),
            );
        let mut content = div()
            .size_full()
            .p(space::xxl())
            .flex()
            .flex_col()
            .gap(space::xxl())
            .child(ui::collection_header(
                ui::collection_artwork(
                    if self.state.ends_with("no-artwork") {
                        None
                    } else {
                        Some(
                            std::sync::Arc::new(Image::from_bytes(
                                ImageFormat::Svg,
                                include_bytes!("../../tests/fixtures/catalog/collection-cover.svg")
                                    .to_vec(),
                            ))
                            .into(),
                        )
                    },
                    catalog::cover(),
                ),
                ui::collection_info(
                    "PLAYLIST",
                    "Bach for Studying".into(),
                    format!(
                        "{} tracks · Local playlist",
                        if self.state.ends_with("empty") { 0 } else { 4 }
                    ),
                ),
            ))
            .child(ui::collection_toolbar(
                actions,
                ui::search_input(&self.input, false, cx),
            ));
        if self.state.ends_with("error") {
            content = content.child(ui::error_alert(
                "Could not refresh this playlist. Cached tracks remain available.",
                ui::button("preview-dismiss", "DISMISS", color::surface()).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.state = "library-playlist".into();
                        cx.notify();
                    },
                )),
            ));
        }
        if self.confirm_delete {
            content = content.child(
                div()
                    .flex()
                    .gap(space::md())
                    .child("Delete local playlist?")
                    .child(
                        ui::button("preview-confirm", "DELETE", color::pink()).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.state = "library-playlist-empty".into();
                                this.confirm_delete = false;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        ui::button("preview-cancel", "CANCEL", color::surface()).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.confirm_delete = false;
                                cx.notify();
                            }),
                        ),
                    ),
            );
        }
        if self.selected.is_some() {
            content = content.child(
                div()
                    .flex()
                    .justify_between()
                    .child(ui::caps("1 SELECTED"))
                    .child(
                        ui::button("preview-clear", "CLEAR", color::surface()).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.selected = None;
                                cx.notify();
                            }),
                        ),
                    ),
            );
        }
        content = content.child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .border(border::strong())
                .border_color(color::ink())
                .child(rows),
        );
        div()
            .relative()
            .size_full()
            .child(content)
            .children(self.editor.clone())
    }
}
