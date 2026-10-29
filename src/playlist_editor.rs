//! Native modal editor. Drafts survive failed writes; background content is inert.
use crate::{
    playlist_edit::{DESCRIPTION_LIMIT, Details, Fields, TITLE_LIMIT},
    sync,
    theme::{border, color, font, layout, playlist_editor as token, shadow, size, space, text},
    ui,
};
use gpui::{prelude::*, *};
use gpui_component::input::{Input, InputEvent, InputState};

pub enum Event {
    Closed,
    Saved,
}
pub struct PlaylistEditor {
    id: i64,
    original: Option<Details>,
    title: Entity<InputState>,
    description: Entity<InputState>,
    description_scrollbar: Entity<ui::CatalogScrollbar>,
    last_cursor: usize,
    privacy: Option<String>,
    loading: bool,
    saving: bool,
    error: Option<String>,
    confirm_discard: bool,
    focus_title: bool,
    focus: FocusHandle,
    controls: Vec<FocusHandle>,
    subscriptions: Vec<Subscription>,
    preview: bool,
}
impl EventEmitter<Event> for PlaylistEditor {}
impl PlaylistEditor {
    pub fn new(id: i64, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Playlist title"));
        let description = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .auto_grow(1, DESCRIPTION_LIMIT + 1)
                .soft_wrap(true)
                .placeholder("Add a description")
        });
        let subscriptions = [&title, &description]
            .into_iter()
            .map(|field| cx.subscribe(field, |_, _, _: &InputEvent, cx| cx.notify()))
            .collect();
        let mut editor = Self {
            id,
            original: None,
            title,
            description,
            description_scrollbar: cx.new(|_| ui::CatalogScrollbar::new()),
            last_cursor: usize::MAX,
            privacy: None,
            loading: false,
            saving: false,
            error: None,
            confirm_discard: false,
            focus_title: false,
            focus: cx.focus_handle(),
            controls: (0..9).map(|_| cx.focus_handle()).collect(),
            subscriptions,
            preview: false,
        };
        editor.focus.focus(window);
        editor.load(window, cx);
        editor
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.loading = true;
        self.error = None;
        let id = self.id;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { sync::playlist_details(id).map_err(|e| format!("{e:#}")) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.loading = false;
                match result {
                    Ok(details) => this.populate(details, window, cx),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn populate(&mut self, details: Details, window: &mut Window, cx: &mut Context<Self>) {
        self.title.update(cx, |s, cx| {
            s.set_value(details.fields.title.clone(), window, cx)
        });
        self.description.update(cx, |s, cx| {
            s.set_value(details.fields.description.clone(), window, cx)
        });
        self.privacy = details.fields.privacy.clone();
        self.original = Some(details);
        self.focus_title = true;
    }
    fn fields(&self, cx: &App) -> Fields {
        Fields {
            title: self.title.read(cx).value().to_string(),
            description: self.description.read(cx).value().to_string(),
            privacy: self.privacy.clone(),
        }
    }
    fn dirty(&self, cx: &App) -> bool {
        self.original
            .as_ref()
            .is_some_and(|o| o.fields != self.fields(cx))
    }
    fn close(&mut self, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        if self.dirty(cx) {
            self.confirm_discard = true;
            cx.notify();
        } else {
            cx.emit(Event::Closed);
        }
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.saving || !self.dirty(cx) {
            return;
        }
        let fields = self.fields(cx);
        if fields.validate().is_err() {
            return;
        }
        let Some(original) = self.original.clone() else {
            return;
        };
        self.saving = true;
        self.error = None;
        self.focus.focus(window);
        cx.notify();
        if self.preview {
            return;
        }
        let id = self.id;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    sync::edit_playlist(id, &original, &fields).map_err(|e| format!("{e:#}"))
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.saving = false;
                match result {
                    Ok(()) => cx.emit(Event::Saved),
                    Err(e) => {
                        this.error = Some(e);
                        this.controls[5].focus(window);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    /// Offline gallery uses the production component without account or network access.
    pub fn preview(state: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Playlist title"));
        let description = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .auto_grow(1, DESCRIPTION_LIMIT + 1)
                .soft_wrap(true)
        });
        let subscriptions = [&title, &description]
            .into_iter()
            .map(|f| cx.subscribe(f, |_, _, _: &InputEvent, cx| cx.notify()))
            .collect();
        let mut s = Self {
            id: 0,
            original: None,
            title,
            description,
            description_scrollbar: cx.new(|_| ui::CatalogScrollbar::new()),
            last_cursor: usize::MAX,
            privacy: None,
            loading: false,
            saving: false,
            error: None,
            confirm_discard: false,
            focus_title: true,
            focus: cx.focus_handle(),
            controls: (0..9).map(|_| cx.focus_handle()).collect(),
            subscriptions,
            preview: true,
        };
        let fields=Fields {title:"Giovanni Pierluigi da Palestrina: Masses".into(),description:"WORK IN PROGRESS. Still adding masses.\n\n# Sorting\n\nThe masses are sorted by catalog order as shown in the following spreadsheet, which contains every mass along with its catalog number, number of voices, publication book and date.\n\nhttps://docs.google.com/spreadsheets/d/example/edit".into(),privacy:if state=="local" {None} else {Some("public".into())}};
        s.populate(
            Details {
                fields,
                remote: None,
            },
            window,
            cx,
        );
        if state != "pristine" && state != "local" {
            s.original.as_mut().unwrap().fields.title = "Original title".into();
        }
        if state == "empty" {
            s.title.update(cx, |s, cx| s.set_value("", window, cx));
        }
        if state == "saving" {
            s.saving = true;
            s.focus_title = false;
            s.focus.focus(window);
        }
        if state == "error" {
            s.error =
                Some("Couldn’t save your changes. Check your connection and try again.".into());
        }
        if state == "discard" {
            s.confirm_discard = true;
        }
        s
    }
    fn field(
        &self,
        state: &Entity<InputState>,
        multiline: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let height = if multiline {
            token::description_height()
        } else {
            token::title_height()
        };
        let field = div()
            .id(("playlist-field", state.entity_id()))
            .track_focus(&state.read(cx).focus_handle(cx))
            .h(height)
            .w_full()
            .bg(color::white())
            .border(border::strong())
            .border_color(color::ink())
            .focus(|s| s.shadow(vec![shadow::focus_ring()]));
        if multiline {
            let scroll = self.description_scrollbar.read(cx).scroll.clone();
            let owner = cx.entity().downgrade();
            field
                .flex()
                .child(
                    div()
                        .id("description-viewport")
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .overflow_y_scroll()
                        .track_scroll(&scroll)
                        .child(
                            Input::new(state)
                                .appearance(false)
                                .disabled(self.saving)
                                .px(token::field_inset())
                                .py(token::description_inset())
                                .text_size(text::body())
                                .line_height(relative(token::DESCRIPTION_LINE_HEIGHT))
                                .rounded_none(),
                        ),
                )
                .child(self.description_scrollbar.clone())
                .relative()
                .child(
                    canvas(
                        |_, _, _| (),
                        move |_, _, window, cx| {
                            let _ = owner.update(cx, |this, cx| this.reveal_caret(window, cx));
                        },
                    )
                    .absolute()
                    .size_full(),
                )
                .into_any_element()
        } else {
            field
                .child(
                    // Input::h only sizes multiline inputs; size the single-line
                    // layout itself so its centered text fills the bordered field.
                    Styled::h(
                        Input::new(state).appearance(false).disabled(self.saving),
                        height - border::strong() * 2.,
                    )
                    .px(token::field_inset())
                    .text_size(if multiline {
                        text::body()
                    } else {
                        token::title_text()
                    })
                    .rounded_none(),
                )
                .into_any_element()
        }
    }
    fn reveal_caret(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Reuse the native input for IME/selection/wrapping and the app's ruled
        // scrollbar for the viewport. Keep keyboard caret movement visible.
        let cursor = self.description.read(cx).cursor();
        if cursor != self.last_cursor
            && self
                .description
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        {
            let value = self.description.read(cx).value().to_string();
            let utf16 = value[..cursor.min(value.len())].encode_utf16().count();
            let caret = self.description.update(cx, |input, cx| {
                input.bounds_for_range(utf16..utf16, Bounds::default(), window, cx)
            });
            if let Some(caret) = caret {
                let scroll = &self.description_scrollbar.read(cx).scroll;
                let top = -scroll.offset().y;
                let height = token::description_height() - border::strong() * 2. - space::lg();
                let target = if caret.top() < top {
                    caret.top()
                } else if caret.bottom() > top + height {
                    caret.bottom() - height
                } else {
                    top
                };
                let offset = point(layout::zero(), -target.max(layout::zero()));
                if scroll.offset() != offset {
                    scroll.set_offset(offset);
                    cx.notify();
                }
            }
        }
        self.last_cursor = cursor;
    }
    fn label(label: &str) -> Div {
        ui::caps(label).text_color(color::muted())
    }
    fn counter(label: &str) -> Div {
        div()
            .font_family(font::MONO)
            .text_size(text::label())
            .font_weight(FontWeight::NORMAL)
            .text_color(color::muted())
            .child(label.to_owned())
    }
}
impl Render for PlaylistEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _ = &self.subscriptions;
        if self.focus_title {
            self.title.read(cx).focus_handle(cx).focus(window);
            self.focus_title = false;
        }
        if self.confirm_discard
            && !self.controls[6].is_focused(window)
            && !self.controls[7].is_focused(window)
        {
            self.controls[6].focus(window);
        }
        let fields = self.fields(cx);
        let dirty = self.dirty(cx);
        let valid = fields.validate().is_ok();
        let ready = self.original.is_some();
        let mut focus_order = vec![self.controls[0].clone()];
        if self.confirm_discard {
            focus_order = vec![self.controls[6].clone(), self.controls[7].clone()];
        } else if ready {
            focus_order.extend([
                self.title.read(cx).focus_handle(cx),
                self.description.read(cx).focus_handle(cx),
            ]);
            if self.privacy.is_some() {
                focus_order.extend(self.controls[1..4].iter().cloned());
            }
            focus_order.push(self.controls[4].clone());
            if dirty && valid {
                focus_order.push(self.controls[5].clone());
            }
        } else {
            focus_order.push(self.controls[4].clone());
            if !self.loading {
                focus_order.push(self.controls[5].clone());
            }
        }
        if self.error.is_some() && !self.confirm_discard {
            focus_order.insert(1, self.controls[8].clone());
        }
        let saving = self.saving;
        let mut body = div()
            .flex()
            .flex_col()
            .gap(token::field_gap())
            .p(space::xl());
        if let Some(error) = self.error.clone() {
            body = body.child(ui::error_alert(
                error,
                ui::button("edit-dismiss", "DISMISS", color::surface())
                    .track_focus(&self.controls[8])
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.error = None;
                        cx.notify();
                    })),
            ));
        }
        if self.confirm_discard {
            body = body
                .child("Discard unsaved changes?")
                .child("Your playlist has not been changed.")
                .child(
                    div()
                        .flex()
                        .gap(space::md())
                        .child(
                            ui::button("keep-editing", "KEEP EDITING", color::surface())
                                .track_focus(&self.controls[6])
                                .on_click(cx.listener(|this, _, w, cx| {
                                    this.confirm_discard = false;
                                    this.title.read(cx).focus_handle(cx).focus(w);
                                    cx.notify();
                                })),
                        )
                        .child(
                            ui::button("discard", "DISCARD", color::pink())
                                .track_focus(&self.controls[7])
                                .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::Closed))),
                        ),
                );
        } else if !ready {
            body = body.child(if self.loading {
                "Loading playlist…"
            } else {
                "Couldn’t load playlist details."
            });
            if !self.loading {
                body = body.child(
                    ui::button("retry-load", "RETRY", color::lime())
                        .track_focus(&self.controls[5])
                        .on_click(cx.listener(|this, _, w, cx| this.load(w, cx))),
                );
            }
        } else {
            let title_error = if fields.title.trim().is_empty() {
                Some("Give the playlist a title to save it.")
            } else if fields.title.chars().count() > TITLE_LIMIT {
                Some("Title must be 150 characters or fewer.")
            } else {
                None
            };
            body = body
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(space::sm())
                        .child(Self::label("TITLE"))
                        .child(self.field(&self.title, false, cx))
                        .child(
                            div()
                                .min_h(token::counter_height())
                                .flex()
                                .justify_end()
                                .when(title_error.is_some(), |d| d.justify_between())
                                .child(Self::counter(
                                    &title_error.map(str::to_owned).unwrap_or_else(|| {
                                        format!("{} / {TITLE_LIMIT}", fields.title.chars().count())
                                    }),
                                ))
                                .when(fields.title.trim().is_empty(), |d| {
                                    d.child(Self::label("REQUIRED"))
                                }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(space::sm())
                        .child(Self::label("DESCRIPTION"))
                        .child(self.field(&self.description, true, cx))
                        .child(div().flex().justify_end().child(Self::counter(&format!(
                            "{} / {DESCRIPTION_LIMIT}{}",
                            fields.description.chars().count(),
                            if fields.description.chars().count() > DESCRIPTION_LIMIT {
                                " · TOO LONG"
                            } else {
                                ""
                            }
                        )))),
                );
            if let Some(privacy) = &self.privacy {
                let mut choices = div().flex().gap(space::md());
                for (i, (value, label, icon)) in [
                    ("public", "PUBLIC", "world"),
                    ("unlisted", "UNLISTED", "link"),
                    ("private", "PRIVATE", "lock"),
                ]
                .into_iter()
                .enumerate()
                {
                    if self.saving {
                        choices = choices.child(ui::disabled_button(label).flex_1());
                    } else {
                        let selected = privacy == value;
                        choices = choices.child(
                            ui::button(
                                ("privacy", i),
                                "",
                                if selected {
                                    color::ink()
                                } else {
                                    color::surface()
                                },
                            )
                            .flex_1()
                            .px(space::sm())
                            .text_size(text::secondary())
                            .track_focus(&self.controls[i + 1])
                            .child(ui::icon(icon, false).size(token::icon()).text_color(
                                if selected {
                                    color::lime()
                                } else {
                                    color::ink()
                                },
                            ))
                            .child(label)
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.privacy = Some(value.into());
                                    cx.notify();
                                },
                            )),
                        );
                    }
                }
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(space::sm())
                        .child(Self::label("PRIVACY"))
                        .child(choices)
                        .child(
                            div()
                                .text_size(text::secondary())
                                .text_color(color::muted())
                                .child(match privacy.as_str() {
                                    "public" => "Anyone can find and play this playlist.",
                                    "unlisted" => "Anyone with the link can play this playlist.",
                                    _ => "Only you can play this playlist.",
                                }),
                        ),
                );
            }
        }
        let mut footer = div()
            .border_t(border::strong())
            .border_color(color::ink())
            .px(space::xl())
            .py(space::lg())
            .flex()
            .items_center()
            .justify_between()
            .child(Self::label(if self.saving {
                "SAVING…"
            } else if dirty {
                "UNSAVED CHANGES"
            } else {
                ""
            }));
        let mut actions = div().flex().gap(space::md());
        if self.saving {
            actions = actions
                .child(ui::disabled_button("CANCEL"))
                .child(ui::disabled_button("SAVING…"));
        } else {
            actions = actions.child(
                ui::button("edit-cancel", "CANCEL", color::surface())
                    .track_focus(&self.controls[4])
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            );
            if ready && dirty && valid {
                actions = actions.child(
                    ui::button("edit-save", "SAVE", color::lime())
                        .track_focus(&self.controls[5])
                        .on_click(cx.listener(|this, _, w, cx| this.save(w, cx))),
                );
            } else {
                actions = actions.child(ui::disabled_button("SAVE"));
            }
        }
        footer = footer.child(actions);
        div()
            .id("playlist-modal")
            .absolute()
            .inset_0()
            .occlude()
            .bg(color::scrim())
            .flex()
            .items_center()
            .justify_center()
            .font_family(font::SANS)
            .text_size(text::body())
            .text_color(color::ink())
            .font_weight(FontWeight::MEDIUM)
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, w, cx| {
                if event.keystroke.key == "escape" {
                    if this.confirm_discard {
                        this.confirm_discard = false;
                        this.focus_title = true;
                        cx.notify();
                    } else {
                        this.close(cx);
                    }
                }
                if event.keystroke.key == "tab" && !saving {
                    let current = focus_order.iter().position(|f| f.is_focused(w));
                    let next = if event.keystroke.modifiers.shift {
                        current.map_or(focus_order.len() - 1, |i| {
                            (i + focus_order.len() - 1) % focus_order.len()
                        })
                    } else {
                        current.map_or(0, |i| (i + 1) % focus_order.len())
                    };
                    focus_order[next].focus(w);
                }
                if matches!(event.keystroke.key.as_str(), "escape" | "tab") {
                    cx.stop_propagation();
                    w.prevent_default();
                }
            }))
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("playlist-dialog-scroll")
                    .max_h(window.viewport_size().height - space::xxl())
                    .overflow_y_scroll()
                    .p(space::sm())
                    .child(
                        div()
                            .w(token::width())
                            .border(border::strong())
                            .border_color(color::ink())
                            .bg(color::surface())
                            .shadow(shadow::lg())
                            .child(
                                div()
                                    .h(token::header())
                                    .border_b(border::strong())
                                    .border_color(color::ink())
                                    .pl(space::xl())
                                    .pr(space::md())
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        Self::label("EDIT PLAYLIST")
                                            .text_color(color::ink())
                                            .text_size(text::secondary()),
                                    )
                                    .child(if self.saving {
                                        div()
                                            .size(size::row_action())
                                            .border(border::strong())
                                            .border_color(color::ink())
                                            .bg(color::canvas())
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(
                                                ui::icon("x", false)
                                                    .size(token::close_icon())
                                                    .text_color(color::muted()),
                                            )
                                            .into_any_element()
                                    } else {
                                        ui::button("edit-close", "", color::surface())
                                            .size(size::row_action())
                                            .p(layout::zero())
                                            .shadow(vec![])
                                            .track_focus(&self.controls[0])
                                            .child(ui::icon("x", false).size(token::close_icon()))
                                            .on_click(cx.listener(|this, _, _, cx| this.close(cx)))
                                            .into_any_element()
                                    }),
                            )
                            .child(body)
                            .when(!self.confirm_discard, |d| d.child(footer)),
                    ),
            )
    }
}
