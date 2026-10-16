//! Shared native components for the app and component gallery.
use crate::theme::{border, color, font, layout, press, shadow, size, space, text};
use gpui::{prelude::*, *};
use gpui_component::{
    input::{Input, InputState},
    tooltip::Tooltip,
};
use std::borrow::Cow;

pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            "mousa.svg" => include_bytes!("../assets/mousa.svg"),
            "icons/google.svg" => include_bytes!("../assets/icons/google.svg"),
            "icons/lastfm.svg" => include_bytes!("../assets/icons/lastfm.svg"),
            "icons/arrows-shuffle.svg" => include_bytes!("../assets/icons/arrows-shuffle.svg"),
            "icons/playlist-add.svg" => include_bytes!("../assets/icons/playlist-add.svg"),
            "icons/player-skip-forward.svg" => {
                include_bytes!("../assets/icons/player-skip-forward.svg")
            }
            "icons/player-pause.svg" => include_bytes!("../assets/icons/player-pause.svg"),
            "icons/x.svg" => include_bytes!("../assets/icons/x.svg"),
            "icons/player-play.svg" => include_bytes!("../assets/icons/player-play.svg"),
            "icons/volume-off.svg" => include_bytes!("../assets/icons/volume-off.svg"),
            "icons/arrow-down.svg" => include_bytes!("../assets/icons/arrow-down.svg"),
            "icons/external-link.svg" => include_bytes!("../assets/icons/external-link.svg"),
            "icons/repeat.svg" => include_bytes!("../assets/icons/repeat.svg"),
            "icons/arrow-up.svg" => include_bytes!("../assets/icons/arrow-up.svg"),
            "icons/player-skip-back.svg" => include_bytes!("../assets/icons/player-skip-back.svg"),
            "icons/volume.svg" => include_bytes!("../assets/icons/volume.svg"),
            "fonts/SpaceMono-Regular.ttf" => {
                include_bytes!("../assets/fonts/SpaceMono-Regular.ttf")
            }
            "fonts/SpaceGrotesk.ttf" => include_bytes!("../assets/fonts/SpaceGrotesk.ttf"),
            "fonts/SpaceMono-Bold.ttf" => include_bytes!("../assets/fonts/SpaceMono-Bold.ttf"),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }
    fn list(&self, _: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(vec![])
    }
}
pub fn init(cx: &mut App) {
    let assets = Assets;
    cx.text_system()
        .add_fonts(
            [
                "fonts/SpaceGrotesk.ttf",
                "fonts/SpaceMono-Regular.ttf",
                "fonts/SpaceMono-Bold.ttf",
            ]
            .iter()
            .map(|p| assets.load(p).unwrap().unwrap())
            .collect(),
        )
        .expect("load bundled design fonts");
    gpui_component::init(cx);
    let t = gpui_component::Theme::global_mut(cx);
    t.font_family = font::SANS.into();
    t.font_size = text::body();
    t.mono_font_family = font::MONO.into();
    t.mono_font_size = text::body();
    t.radius = layout::zero();
    t.radius_lg = layout::zero();
    t.colors.background = color::surface();
    t.colors.foreground = color::ink();
    t.colors.border = color::ink();
    t.colors.input = color::ink();
    t.colors.muted = color::canvas();
    t.colors.muted_foreground = color::muted();
    t.colors.ring = color::ink();
}
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    fill: Hsla,
) -> Stateful<Div> {
    let on = fill == color::ink();
    let label: SharedString = label.into();
    div()
        .id(id)
        .tab_index(0)
        .relative()
        .h(size::control())
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .px(space::md())
        .gap(space::sm())
        .border(border::strong())
        .border_color(color::ink())
        .rounded_none()
        .bg(fill)
        .text_color(if on { color::lime() } else { color::ink() })
        .font_family(font::MONO)
        .text_size(text::body())
        .font_weight(FontWeight::BOLD)
        .shadow(if on { shadow::on() } else { shadow::sm() })
        .mr(press::sm())
        .mb(press::sm())
        .cursor_pointer()
        .hover(move |s| s.bg(if on { color::ink() } else { color::white() }))
        .active(|s| s.top(press::sm()).left(press::sm()).shadow(vec![]))
        .focus(|s| s.shadow(vec![shadow::focus_ring()]))
        .when(!label.is_empty(), |d| d.child(label))
}
pub fn icon(name: &'static str, hero: bool) -> Svg {
    svg().path(format!("icons/{name}.svg")).size(if hero {
        layout::hero_icon()
    } else {
        layout::icon()
    })
}
pub fn icon_button(
    id: impl Into<ElementId>,
    name: &'static str,
    label: &'static str,
    on: bool,
    hero: bool,
    row: bool,
) -> Stateful<Div> {
    let fill = if on {
        color::ink()
    } else if hero {
        color::lime()
    } else {
        color::surface()
    };
    let side = if hero {
        size::hero()
    } else if row {
        size::row_action()
    } else {
        size::transport()
    };
    button(id, "", fill)
        .size(side)
        .px(layout::zero())
        .child(icon(name, hero).text_color(if on { color::lime() } else { color::ink() }))
        .when(hero, |d| {
            d.shadow(shadow::md())
                .mr(press::md())
                .mb(press::md())
                .active(|s| s.top(press::md()).left(press::md()).shadow(vec![]))
        })
        .when(row, |d| {
            d.shadow(vec![]).mr(layout::zero()).mb(layout::zero())
        })
        .when(row, |d| {
            d.opacity(0.)
                .group_hover("track-row", |s| s.opacity(1.))
                .focus(|s| s.opacity(1.).shadow(vec![shadow::focus_ring()]))
        })
        .tooltip(move |w, cx| Tooltip::new(label).build(w, cx))
}
pub fn step_button(id: &'static str, label: &'static str) -> Stateful<Div> {
    button(id, label, color::surface())
        .size(size::transport())
        .px(layout::zero())
        .tooltip(move |w, cx| {
            Tooltip::new(if label == "−10" {
                "Back 10 seconds (Ctrl+J)"
            } else {
                "Forward 10 seconds (Ctrl+L)"
            })
            .build(w, cx)
        })
}
pub fn input(state: &Entity<InputState>, cx: &App) -> Stateful<Div> {
    div()
        .id(("field", state.entity_id()))
        .track_focus(&state.read(cx).focus_handle(cx))
        .tab_index(0)
        .h(size::control())
        .flex_shrink_0()
        .bg(color::white())
        .border(border::strong())
        .border_color(color::ink())
        .rounded_none()
        .focus(|s| s.shadow(vec![shadow::focus_ring()]))
        .child(
            Input::new(state)
                .appearance(false)
                .h(size::control() - border::strong() * 2.)
                .rounded_none(),
        )
}
pub fn playlist_item(
    id: i64,
    name: String,
    count: usize,
    source: &'static str,
    on: bool,
) -> Stateful<Div> {
    button(
        ("playlist", id as usize),
        "",
        if on { color::ink() } else { color::surface() },
    )
    .h_auto()
    .min_h(size::row())
    .py(space::sm())
    .shadow(vec![])
    .mr(layout::zero())
    .mb(layout::zero())
    .flex_col()
    .items_start()
    .gap(space::xs())
    .font_family(font::SANS)
    .child(
        div()
            .w_full()
            .truncate()
            .font_weight(FontWeight::MEDIUM)
            .child(name),
    )
    .child(
        div()
            .font_family(font::MONO)
            .text_size(text::label())
            .child(format!("{count} · {source}")),
    )
}
pub fn player_panel(
    info: impl IntoElement,
    controls: impl IntoElement,
    timeline: impl IntoElement,
    volume: impl IntoElement,
    compact: bool,
) -> Div {
    div()
        .mx(space::lg())
        .mb(space::xxl())
        .mt(space::sm())
        .p(space::xl())
        .border(border::strong())
        .border_color(color::ink())
        .bg(color::surface())
        .shadow(shadow::lg())
        .flex()
        .items_center()
        .gap(space::xl())
        .flex_shrink_0()
        .child(div().flex_1().min_w_0().child(info))
        .child(
            div()
                .w(if compact {
                    layout::compact_center()
                } else {
                    layout::player_center()
                })
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap(space::lg())
                .id("player-controls")
                .cursor_default()
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(controls)
                .child(timeline),
        )
        .child(
            div()
                .id("player-volume")
                .flex_1()
                .min_w_0()
                .flex()
                .justify_end()
                .child(
                    div()
                        .id("player-volume-controls")
                        .cursor_default()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(volume),
                ),
        )
}
#[derive(Clone)]
pub struct BarChange(pub f32);
pub struct Bar {
    pub value: f32,
    pub disabled: bool,
    dragging: bool,
    bounds: Bounds<Pixels>,
    focus: FocusHandle,
    label: &'static str,
    step: f32,
}
impl EventEmitter<BarChange> for Bar {}
impl Bar {
    pub fn new(label: &'static str, step: f32, cx: &mut Context<Self>) -> Self {
        Self {
            value: 0.,
            disabled: false,
            dragging: false,
            bounds: Bounds::default(),
            focus: cx.focus_handle(),
            label,
            step,
        }
    }
    pub fn sync(&mut self, value: f32, disabled: bool, cx: &mut Context<Self>) {
        if !self.dragging && (self.value != value || self.disabled != disabled) {
            self.value = value.clamp(0., 1.);
            self.disabled = disabled;
            cx.notify();
        }
    }
    fn change(&mut self, value: f32, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        self.value = value.clamp(0., 1.);
        cx.emit(BarChange(self.value));
        cx.notify();
    }
    fn pointer(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let usable = self.bounds.size.width - size::bar_thumb();
        if usable > layout::zero() {
            self.change(
                (x - self.bounds.left() - size::bar_thumb() / 2.) / usable,
                cx,
            );
        }
    }
}
impl Render for Bar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let weak = entity.downgrade();
        let value = self.value;
        let label = self.label;
        div()
            .id("bar")
            .when(!self.disabled, |d| d.track_focus(&self.focus).tab_index(0))
            .relative()
            .w_full()
            .h(size::control())
            .flex_shrink_0()
            .cursor_pointer()
            .tooltip(move |w, cx| Tooltip::new(label).build(w, cx))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, w, cx| {
                    if this.disabled {
                        return;
                    }
                    this.focus.focus(w);
                    this.dragging = true;
                    this.pointer(e.position.x, cx);
                    cx.stop_propagation();
                }),
            )
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                let next = match e.keystroke.key.as_str() {
                    "left" | "down" => this.value - this.step,
                    "right" | "up" => this.value + this.step,
                    "home" => 0.,
                    "end" => 1.,
                    _ => return,
                };
                this.change(next, cx);
                cx.stop_propagation();
            }))
            .child(
                div()
                    .absolute()
                    .left(size::bar_thumb() / 2.)
                    .right(size::bar_thumb() / 2.)
                    .top((size::control() - size::bar_height()) / 2.)
                    .h(size::bar_height())
                    .bg(if self.disabled {
                        color::canvas()
                    } else {
                        color::white()
                    })
                    .border(border::strong())
                    .border_color(color::ink())
                    .child(
                        div()
                            .h_full()
                            .w(relative(value))
                            .bg(if self.disabled {
                                color::canvas()
                            } else {
                                color::lime()
                            })
                            .border_r(border::strong())
                            .border_color(color::ink()),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(size::bar_thumb() / 2.)
                    .right(size::bar_thumb() / 2.)
                    .top((size::control() - size::bar_thumb()) / 2.)
                    .child(
                        div()
                            .absolute()
                            .left(relative(value))
                            .ml(-size::bar_thumb() / 2.)
                            .size(size::bar_thumb())
                            .border(border::strong())
                            .border_color(color::ink())
                            .bg(if self.disabled {
                                color::canvas()
                            } else {
                                color::white()
                            })
                            .shadow(if self.disabled { vec![] } else { shadow::sm() }),
                    ),
            )
            .focus(|s| s.shadow(vec![shadow::focus_ring()]))
            .child(
                canvas(
                    move |bounds, _, cx| entity.update(cx, |bar, _| bar.bounds = bounds),
                    move |_, _, window, _| {
                        let weak_move = weak.clone();
                        window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble {
                                let _ = weak_move.update(cx, |bar, cx| {
                                    if bar.dragging {
                                        bar.pointer(e.position.x, cx);
                                    }
                                });
                            }
                        });
                        let weak_up = weak.clone();
                        window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble && e.button == MouseButton::Left {
                                let _ = weak_up.update(cx, |bar, cx| {
                                    if bar.dragging {
                                        bar.pointer(e.position.x, cx);
                                        bar.dragging = false;
                                        cx.notify();
                                    }
                                });
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}

pub fn track_row(index: usize, playing: bool) -> Stateful<Div> {
    div()
        .id(("track", index))
        .group("track-row")
        .tab_index(0)
        .focus(|s| s.bg(color::canvas()).shadow(vec![shadow::focus_ring()]))
        .min_h(size::row())
        .py(space::sm())
        .w_full()
        .flex()
        .items_center()
        .gap(space::md())
        .px(space::lg())
        .border_b(border::hairline())
        .border_color(color::ink())
        .bg(if playing {
            color::lime()
        } else {
            color::surface()
        })
}

/// Uppercase mono label with the specified tracking, supported without font fallback.
pub fn caps(label: &str) -> Div {
    div()
        .flex()
        .gap(text::tracking())
        .font_family(font::MONO)
        .text_size(text::label())
        .font_weight(FontWeight::BOLD)
        .text_color(color::muted())
        .children(label.chars().map(|c| {
            div().child(if c == ' ' {
                "\u{00a0}".to_owned()
            } else {
                c.to_string()
            })
        }))
}
pub fn disabled_button(label: &str) -> Div {
    div()
        .h(size::control())
        .px(space::md())
        .border(border::strong())
        .border_color(color::ink())
        .bg(color::canvas())
        .text_color(color::muted())
        .font_family(font::MONO)
        .text_size(text::body())
        .font_weight(FontWeight::BOLD)
        .flex()
        .items_center()
        .child(label.to_owned())
}

/// Shared provider badge for the landing page and sidebar accounts.
fn provider_badge(provider: &'static str) -> Div {
    div()
        .size(size::transport())
        .flex_shrink_0()
        .border(border::strong())
        .border_color(color::ink())
        .bg(color::white())
        .flex()
        .items_center()
        .justify_center()
        .child(img(format!("icons/{provider}.svg")).size(layout::icon()))
}

/// Provider identity remains visible when the action changes to Sign out.
pub fn account_button(
    id: &'static str,
    provider: &'static str,
    name: Option<&str>,
    busy: bool,
) -> Stateful<Div> {
    let connected = name.is_some();
    let label = name.unwrap_or(if busy {
        "Connecting…"
    } else {
        "Connect Last.fm"
    });
    button(id, "", color::surface())
        .group(id)
        .h(size::row())
        .px(space::sm())
        .justify_start()
        .min_w_0()
        .when(connected && !busy, |d| d.hover(|s| s.bg(color::pink())))
        .child(provider_badge(provider))
        .child(
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .child(label.to_owned())
                        .when(connected && !busy, |d| d.group_hover(id, |s| s.opacity(0.))),
                )
                .when(connected && !busy, |d| {
                    d.child(
                        div()
                            .absolute()
                            .inset_0()
                            .opacity(0.)
                            .group_hover(id, |s| s.opacity(1.))
                            .child("Sign out"),
                    )
                }),
        )
        .tooltip(move |w, cx| {
            Tooltip::new(if connected {
                "Sign out"
            } else {
                "Connect Last.fm"
            })
            .build(w, cx)
        })
}
pub fn google_sign_in(busy: bool) -> Stateful<Div> {
    button("google-sign-in", "", color::lime())
        .h(size::row())
        .w_full()
        .px(space::sm())
        .child(provider_badge("google"))
        .child(div().flex_1().text_center().child(if busy {
            "WAITING FOR GOOGLE…"
        } else {
            "SIGN IN WITH GOOGLE"
        }))
}
pub fn landing(sign_in: impl IntoElement, error: Option<&str>) -> Div {
    use crate::theme::landing;
    div()
        .size_full()
        .bg(color::canvas())
        .text_color(color::ink())
        .font_family(font::SANS)
        .font_weight(FontWeight::MEDIUM)
        .text_size(text::body())
        .flex()
        .items_center()
        .justify_center()
        .p(space::xxl())
        .child(
            div()
                .w(landing::width())
                .p(landing::padding())
                .border(border::strong())
                .border_color(color::ink())
                .bg(color::surface())
                .shadow(shadow::lg())
                .flex()
                .flex_col()
                .gap(space::xxl())
                .child(
                    div()
                        .size(landing::logo())
                        .flex_shrink_0()
                        .border(border::strong())
                        .border_color(color::ink())
                        .bg(color::lime())
                        .shadow(shadow::md())
                        .child(
                            img(ImageSource::Resource(Resource::Embedded(
                                "mousa.svg".into(),
                            )))
                            .size_full(),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(space::md())
                        .child(
                            div()
                                .text_size(landing::title())
                                .line_height(relative(1.))
                                .font_weight(FontWeight::BOLD)
                                .child("Mousa"),
                        )
                        .child(
                            div()
                                .text_size(text::track_title())
                                .line_height(relative(1.))
                                .text_color(color::muted())
                                .child("Just a fast music client."),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(space::sm())
                        .child(sign_in)
                        .child(
                            div()
                                .text_size(text::secondary())
                                .text_color(color::muted())
                                .child("Sign-in opens in your browser."),
                        ),
                )
                .when_some(error, |d, error| {
                    d.child(
                        div()
                            .p(space::md())
                            .bg(color::pink())
                            .child(error.to_owned()),
                    )
                }),
        )
}
