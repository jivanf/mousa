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
            "icons/world.svg" => include_bytes!("../assets/icons/world.svg"),
            "icons/link.svg" => include_bytes!("../assets/icons/link.svg"),
            "icons/lock.svg" => include_bytes!("../assets/icons/lock.svg"),
            "icons/search.svg" => include_bytes!("../assets/icons/search.svg"),
            "icons/plus.svg" => include_bytes!("../assets/icons/plus.svg"),
            "icons/arrow-left.svg" => include_bytes!("../assets/icons/arrow-left.svg"),
            "icons/clock.svg" => include_bytes!("../assets/icons/clock.svg"),
            "icons/plug-connected-x.svg" => include_bytes!("../assets/icons/plug-connected-x.svg"),
            "icons/disc.svg" => include_bytes!("../assets/icons/disc.svg"),
            "icons/user.svg" => include_bytes!("../assets/icons/user.svg"),
            "icons/playlist.svg" => include_bytes!("../assets/icons/playlist.svg"),
            "icons/microphone.svg" => include_bytes!("../assets/icons/microphone.svg"),
            "icons/video.svg" => include_bytes!("../assets/icons/video.svg"),
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
#[derive(Default)]
struct ButtonPress {
    started: Option<std::time::Instant>,
    from: f32,
    held: bool,
    pointer_focus: bool,
}
impl ButtonPress {
    fn amount(&self, now: std::time::Instant) -> f32 {
        let target = if self.held { 1. } else { 0. };
        let progress = self
            .started
            .map(|start| press::progress(now.saturating_duration_since(start)))
            .unwrap_or(1.);
        self.from + (target - self.from) * progress
    }
    fn set_held(&mut self, held: bool, now: std::time::Instant) -> bool {
        if self.held == held {
            return false;
        }
        self.from = self.amount(now);
        self.started = Some(now);
        self.held = held;
        true
    }
    fn animating(&self, now: std::time::Instant) -> bool {
        self.started
            .is_some_and(|start| now.saturating_duration_since(start) < press::duration())
    }
}
fn set_button_held(state: &Entity<ButtonPress>, held: bool, cx: &mut App) {
    state.update(cx, |motion, cx| {
        if motion.set_held(held, std::time::Instant::now()) {
            cx.notify();
        }
    });
}

#[cfg(test)]
mod button_press_tests {
    use super::{ButtonPress, press};
    use std::time::{Duration, Instant};

    #[test]
    fn stays_pressed_until_release() {
        let now = Instant::now();
        let mut motion = ButtonPress::default();
        motion.set_held(true, now);
        let held = now + Duration::from_secs(30);
        assert_eq!(motion.amount(held), 1.);
        assert!(!motion.animating(held));
        assert!(!motion.set_held(true, held)); // Key repeat must not restart it.
        motion.set_held(false, held);
        assert_eq!(motion.amount(held), 1.);
        assert_eq!(motion.amount(held + press::duration()), 0.);
    }

    #[test]
    fn early_release_and_repress_remain_continuous() {
        let now = Instant::now();
        let mut motion = ButtonPress::default();
        motion.set_held(true, now);
        let release = now + press::duration() / 2;
        let amount = motion.amount(release);
        assert!(amount > 0. && amount < 1.);
        motion.set_held(false, release);
        assert_eq!(motion.amount(release), amount);
        let repress = release + press::duration() / 2;
        let amount = motion.amount(repress);
        motion.set_held(true, repress);
        assert_eq!(motion.amount(repress), amount);
        assert_eq!(motion.amount(repress + press::duration()), 1.);
    }
}

/// Fluent shared controls with press/release motion scoped to their stable ID.
#[derive(IntoElement)]
pub struct Button {
    inner: Stateful<Div>,
}
impl Styled for Button {
    fn style(&mut self) -> &mut StyleRefinement {
        self.inner.style()
    }
}
impl ParentElement for Button {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.inner.extend(elements);
    }
}
impl InteractiveElement for Button {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.inner.interactivity()
    }
}
impl StatefulInteractiveElement for Button {}
impl RenderOnce for Button {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = Element::id(&self.inner).expect("buttons have stable IDs");
        let state = window.use_keyed_state(id, cx, |_, _| ButtonPress::default());
        let now = std::time::Instant::now();
        if !window.is_window_active() {
            set_button_held(&state, false, cx);
        }
        let amount = state.read(cx).amount(now);
        let mut shadows = self.inner.style().box_shadow.clone().unwrap_or_default();
        if let Some(depth) = shadows.first().map(|shadow| shadow.offset) {
            if amount > 0. {
                for shadow in &mut shadows {
                    shadow.offset = shadow.offset * (1. - amount);
                }
                self.inner = self
                    .inner
                    .top(depth.y * amount)
                    .left(depth.x * amount)
                    .shadow(shadows.clone());
            }
            // Mouse focus must not cover the collapsing shadow with a ring.
            // Keyboard navigation retains a visible focus indicator.
            if !state.read(cx).pointer_focus {
                shadows.push(shadow::focus_ring());
            }
            self.inner = self.inner.focus(move |s| s.shadow(shadows));
            if state.read(cx).animating(now) {
                window.request_animation_frame();
            }
            let down = state.clone();
            let up = state.clone();
            let outside = state.clone();
            let key_down = state.clone();
            self.inner = self
                .inner
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    down.update(cx, |motion, cx| {
                        motion.pointer_focus = true;
                        motion.set_held(true, std::time::Instant::now());
                        cx.notify();
                    });
                })
                .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                    set_button_held(&outside, false, cx)
                })
                .on_key_down(move |event, _, cx| {
                    key_down.update(cx, |motion, cx| {
                        if motion.pointer_focus {
                            motion.pointer_focus = false;
                            cx.notify();
                        }
                    });
                    if matches!(event.keystroke.key.as_str(), "space" | "enter")
                        && !event.keystroke.modifiers.modified()
                    {
                        set_button_held(&key_down, true, cx);
                    }
                })
                .on_key_up(move |event, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                        set_button_held(&state, false, cx);
                    }
                });
            // Release during capture, before an action can stop bubbling.
            self.inner
                .interactivity()
                .capture_any_mouse_up(move |event, _, cx| {
                    if event.button == MouseButton::Left {
                        set_button_held(&up, false, cx);
                    }
                });
        }
        self.inner
    }
}
pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, fill: Hsla) -> Button {
    let hover_fill = if fill == color::ink() {
        color::ink()
    } else {
        color::white()
    };
    button_with_hover(id, label, fill, hover_fill)
}

// GPUI accepts one hover registration. Specialized buttons supply their fill
// here instead of adding a second `.hover()` to the shared button.
fn button_with_hover(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    fill: Hsla,
    hover_fill: Hsla,
) -> Button {
    let on = fill == color::ink();
    let label: SharedString = label.into();
    Button {
        inner: div()
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
            .hover(move |s| s.bg(hover_fill))
            .focus(|s| s.shadow(vec![shadow::focus_ring()]))
            .when(!label.is_empty(), |d| d.child(label)),
    }
}
pub fn icon(name: &'static str, hero: bool) -> Svg {
    svg()
        .path(format!("icons/{name}.svg"))
        .text_color(color::ink())
        .size(if hero {
            layout::hero_icon()
        } else {
            layout::icon()
        })
}
/// Only ambiguous icon actions need an explanatory hover label.
pub fn icon_needs_tooltip(name: &str) -> bool {
    matches!(name, "playlist-add" | "plus" | "external-link")
}

pub fn icon_button(
    id: impl Into<ElementId>,
    name: &'static str,
    label: &'static str,
    on: bool,
    hero: bool,
    row: bool,
) -> Button {
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
            d.shadow(shadow::md()).mr(press::md()).mb(press::md())
        })
        .when(row, track_action)
        .when(row, |d| {
            d.shadow(vec![]).mr(layout::zero()).mb(layout::zero())
        })
        .when(row, |d| {
            d.opacity(0.)
                .group_hover("track-row", |s| s.opacity(1.))
                .focus(|s| s.opacity(1.).shadow(vec![shadow::focus_ring()]))
        })
        .when(row && icon_needs_tooltip(name), |d| {
            d.tooltip(move |w, cx| Tooltip::new(label).build(w, cx))
        })
}
pub fn step_button(id: &'static str, label: &'static str) -> Button {
    button(id, label, color::surface())
        .size(size::transport())
        .px(layout::zero())
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
/// The existing application error banner, shared with modal forms.
pub fn error_alert(message: impl Into<SharedString>, dismiss: impl IntoElement) -> Div {
    alert(message, color::pink(), dismiss)
}
/// Full-width action feedback, sharing the application's error-banner geometry.
pub fn feedback_alert(
    message: impl Into<SharedString>,
    info: bool,
    dismiss: impl IntoElement,
) -> Div {
    alert(
        message,
        if info {
            color::surface()
        } else {
            color::lime()
        },
        dismiss,
    )
}
fn alert(message: impl Into<SharedString>, background: Hsla, dismiss: impl IntoElement) -> Div {
    div()
        .flex()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .items_center()
        .gap(space::md())
        .p(space::md())
        .border(border::strong())
        .border_color(color::ink())
        .bg(background)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .whitespace_normal()
                .child(message.into()),
        )
        .child(dismiss)
}
/// Search and filter fields share an inline, conditional clear affordance.
pub fn search_input(state: &Entity<InputState>, large: bool, cx: &App) -> Stateful<Div> {
    let field = state.clone();
    let height = if large {
        crate::theme::catalog::field()
    } else {
        size::control()
    };
    div()
        .id(("search-field", state.entity_id()))
        .track_focus(&state.read(cx).focus_handle(cx))
        .tab_index(0)
        .h(height)
        .flex_shrink_0()
        .flex()
        .items_center()
        .min_w_0()
        .bg(color::white())
        .border(border::strong())
        .border_color(color::ink())
        .focus(|s| s.shadow(vec![shadow::focus_ring()]))
        .when(large, |d| d.pl(space::lg()).child(icon("search", false)))
        .child(
            div().flex_1().min_w_0().child(
                Input::new(state)
                    .appearance(false)
                    .h(height - border::strong() * 2.)
                    .text_size(if large {
                        text::track_title()
                    } else {
                        text::secondary()
                    })
                    .rounded_none(),
            ),
        )
        .when(!state.read(cx).value().is_empty(), |d| {
            d.child(
                button(("clear-search", state.entity_id()), "", color::surface())
                    .size(if large {
                        size::row_action()
                    } else {
                        layout::icon()
                    })
                    .px(layout::zero())
                    .mr(space::sm())
                    .mb(layout::zero())
                    .shadow(vec![])
                    .child(icon("x", false).size(crate::theme::catalog::icon()))
                    .on_click(move |_, w, cx| {
                        field.update(cx, |s, cx| {
                            s.set_value("", w, cx);
                            s.focus(w, cx);
                        });
                        cx.stop_propagation();
                    }),
            )
        })
}

pub fn playlist_item(id: i64, name: String, on: bool) -> Button {
    // Wait for the final available width before laying out text. GPUI's
    // intrinsic-size passes mutate truncated font runs, which can leave later
    // passes rendering the remainder of the name with the fallback font.
    let label = canvas(
        move |bounds, window, cx| {
            let text = StyledText::new(name.clone());
            let text_layout = text.layout().clone();
            let mut label = div()
                .w(bounds.size.width)
                .truncate()
                .child(
                    InteractiveText::new(("playlist-name", id as usize), text).tooltip(
                        move |_, window, cx| {
                            (text_layout.text() != name)
                                .then(|| Tooltip::new(name.clone()).build(window, cx))
                        },
                    ),
                )
                .into_any_element();
            let measured =
                label.layout_as_root(bounds.size.map(AvailableSpace::Definite), window, cx);
            label.prepaint_at(
                point(
                    bounds.left(),
                    bounds.top() + (bounds.size.height - measured.height) / 2.,
                ),
                window,
                cx,
            );
            label
        },
        |_, mut label, window, cx| label.paint(window, cx),
    )
    .flex_1()
    .min_w_0()
    .h_full();
    button(
        ("playlist", id as usize),
        "",
        if on { color::ink() } else { color::surface() },
    )
    .shadow(vec![])
    .mr(layout::zero())
    .mb(layout::zero())
    .min_w_0()
    .font_family(font::SANS)
    .font_weight(FontWeight::MEDIUM)
    .child(label)
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
                .child(div().id("player-volume-controls").child(volume)),
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
        let value = value.clamp(0., 1.);
        if self.value == value {
            return;
        }
        self.value = value;
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let weak = entity.downgrade();
        let value = self.value;
        let label = self.label;
        div()
            .id("bar")
            .occlude()
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

pub fn selection_action(
    id: impl Into<ElementId>,
    label: &str,
    icon_name: &'static str,
    destructive: bool,
) -> Button {
    button(
        id,
        "",
        if destructive {
            color::pink()
        } else {
            color::surface()
        },
    )
    .text_size(text::secondary())
    .mb(px(0.))
    .child(icon(icon_name, false).size(crate::theme::catalog::icon()))
    .child(label.to_owned())
}

/// The same neutral action strip is used by catalog, library and queue tables.
pub fn selection_bar(
    count: usize,
    unavailable: usize,
    pinned: bool,
    actions: Div,
    clear: Button,
) -> Div {
    div()
        .min_h(crate::theme::selection::bar_height(unavailable))
        .w_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(space::md())
        .px(space::lg())
        .py(space::md())
        .bg(color::canvas())
        .border_color(color::ink())
        .border_b(border::strong())
        .when(!pinned, |d| {
            d.border_t(border::strong()).border_x(border::strong())
        })
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .flex_col()
                .items_start()
                .gap(space::xs())
                .child(
                    caps(&format!("{count} SELECTED"))
                        .h(size::control())
                        .flex()
                        .items_center()
                        .px(space::sm())
                        .bg(color::ink())
                        .text_color(color::lime()),
                )
                .when(unavailable > 0, |d| {
                    d.w(crate::theme::selection::summary_width()).child(
                        div()
                            .font_family(font::MONO)
                            .text_size(text::label())
                            .font_weight(FontWeight::BOLD)
                            .child(format!("{unavailable} UNAVAILABLE, SKIPPED")),
                    )
                }),
        )
        .child(actions.flex_1().min_w_0().flex_wrap())
        .child(clear)
}

/// Lay out a placeholder in the document, but paint and hit-test the strip at
/// the viewport's top once its natural position scrolls out of view. Measuring
/// in prepaint also follows scrollbar dragging and programmatic scrolling.
pub fn sticky_selection_bar(
    scroll: ScrollHandle,
    unavailable: usize,
    build: impl FnOnce(bool) -> Div + 'static,
) -> impl IntoElement {
    deferred(
        canvas(
            move |bounds, window, cx| {
                let viewport = scroll.bounds();
                let pinned = bounds.top() <= viewport.top();
                let origin = if pinned {
                    viewport.origin
                } else {
                    bounds.origin
                };
                let width = if pinned {
                    viewport.size.width
                } else {
                    bounds.size.width
                };
                let mut element = build(pinned).into_any_element();
                element.layout_as_root(
                    size(
                        AvailableSpace::Definite(width),
                        AvailableSpace::Definite(crate::theme::selection::bar_height(unavailable)),
                    ),
                    window,
                    cx,
                );
                element.prepaint_at(origin, window, cx);
                element
            },
            |_, mut element, window, cx| element.paint(window, cx),
        )
        .h(crate::theme::selection::bar_height(unavailable))
        .w_full()
        .flex_shrink_0(),
    )
}

/// Shared selection surface; layout and optional drag handling belong to the caller.
pub fn selectable_track_row(
    row: Stateful<Div>,
    selected: bool,
    playing: bool,
    on_press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    row.when(selected, |d| {
        d.border_l(crate::theme::selection::marker())
            .when(!playing, |d| d.bg(color::canvas()))
    })
    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
        if !window.default_prevented() {
            on_press(event, window, cx);
        }
    })
}

/// GPUI's focusable child claims the press with prevent_default before the row
/// receives it. Keep mouse/keyboard focus and the action's click handler intact.
pub fn track_action<E: StatefulInteractiveElement>(row: E) -> E {
    row.tab_index(0)
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

/// Provider identity remains visible when the account action is revealed.
pub fn account_button(
    id: &'static str,
    provider: &'static str,
    name: Option<&str>,
    busy: bool,
) -> Button {
    let connected = name.is_some();
    let disconnect_label = if provider == "lastfm" {
        "Disconnect"
    } else {
        "Sign out"
    };
    let label = name.unwrap_or(if busy {
        "Connecting…"
    } else {
        "Connect Last.fm"
    });
    let hover_fill = if connected && !busy {
        color::pink()
    } else {
        color::white()
    };
    button_with_hover(id, "", color::surface(), hover_fill)
        .group(id)
        .h(size::row())
        .px(space::sm())
        .justify_start()
        .min_w_0()
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
                            .child(disconnect_label),
                    )
                }),
        )
}

#[cfg(test)]
mod account_tests {
    #[test]
    fn account_variants_register_each_interaction_style_once() {
        // GPUI debug assertions reject a second hover registration during construction.
        for provider in ["google", "lastfm"] {
            for name in [None, Some("Preview account")] {
                for busy in [false, true] {
                    let _ = super::account_button("account-preview", provider, name, busy);
                }
            }
        }
    }
}
pub fn google_sign_in(busy: bool) -> Button {
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
                            .w_full()
                            .min_w_0()
                            .flex_shrink_0()
                            .whitespace_normal()
                            .p(space::md())
                            .bg(color::pink())
                            .child(error.to_owned()),
                    )
                }),
        )
}

/// A persistent scrollbar for GPUI's variable-height virtualized track lists.
pub struct TrackScrollbar {
    list: ListState,
    grab: Option<Pixels>,
    show_rail: bool,
    bounds: Bounds<Pixels>,
    focus: FocusHandle,
}
impl TrackScrollbar {
    pub fn new(list: ListState, cx: &mut Context<Self>) -> Self {
        Self {
            list,
            grab: None,
            show_rail: false,
            bounds: Bounds::default(),
            focus: cx.focus_handle(),
        }
    }
    // Unmeasured GPUI rows have zero pixel height. Use logical positions so
    // dragging can reach tracks that have never been rendered.
    fn visible_span(&self) -> f32 {
        let viewport = self.list.viewport_bounds();
        let first = self.list.logical_scroll_top().item_ix;
        let mut span = 0.;
        for i in first..self.list.item_count() {
            let Some(bounds) = self.list.bounds_for_item(i) else {
                break;
            };
            if bounds.top() >= viewport.bottom() {
                break;
            }
            if bounds.size.height > px(0.) {
                span += ((bounds.bottom().min(viewport.bottom())
                    - bounds.top().max(viewport.top()))
                    / bounds.size.height)
                    .max(0.);
            }
        }
        span.max(1.)
    }
    fn overflowing(&self) -> bool {
        self.list.item_count() as f32 > self.visible_span()
    }
    fn geometry(&self) -> (Pixels, Pixels) {
        let height = self.bounds.size.height;
        let count = self.list.item_count() as f32;
        let span = self.visible_span();
        let thumb = (height * (span / count.max(1.)).min(1.))
            .max(crate::theme::scrollbar::min_thumb())
            .min(height);
        let offset = self.list.logical_scroll_top();
        let row_height = self
            .list
            .bounds_for_item(offset.item_ix)
            .map(|b| b.size.height)
            .unwrap_or(size::row())
            .max(px(1.));
        let position = offset.item_ix as f32 + offset.offset_in_item / row_height;
        let fraction = (position / (count - span).max(1.)).clamp(0., 1.);
        ((height - thumb) * fraction, thumb)
    }
    fn pointer(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let (_, thumb) = self.geometry();
        let travel = self.bounds.size.height - thumb;
        if travel > px(0.) {
            let fraction =
                ((y - self.bounds.top() - self.grab.unwrap_or(thumb / 2.)) / travel).clamp(0., 1.);
            let index = (fraction * (self.list.item_count() as f32 - self.visible_span()).max(0.))
                .round() as usize;
            self.list.scroll_to(ListOffset {
                item_ix: if fraction == 1. {
                    self.list.item_count().saturating_sub(1)
                } else {
                    index
                },
                offset_in_item: px(0.),
            });
            cx.notify();
        }
    }
}
impl Render for TrackScrollbar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let weak = entity.downgrade();
        div().id("track-scrollbar").relative().w(if self.show_rail { crate::theme::scrollbar::width() } else { px(0.) }).h_full().flex_shrink_0()
            .when(self.show_rail, |s| s.bg(color::white()).border_l(border::strong()).border_color(color::ink())
                .track_focus(&self.focus).tab_index(0).focus(|s| s.shadow(vec![shadow::focus_ring()])))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| {
                if !this.overflowing() { return; }
                this.focus.focus(window);
                let (top, thumb) = this.geometry();
                let y = e.position.y - this.bounds.top();
                this.grab = Some(if y >= top && y <= top + thumb { y - top } else { thumb / 2. });
                this.list.scrollbar_drag_started();
                this.pointer(e.position.y, cx);
                cx.notify();
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                let current = this.list.logical_scroll_top().item_ix;
                let page = this.visible_span().floor().max(1.) as usize;
                let next = match e.keystroke.key.as_str() {
                    "up" => current.saturating_sub(1), "down" => current.saturating_add(1),
                    "pageup" => current.saturating_sub(page), "pagedown" => current.saturating_add(page),
                    "home" => 0, "end" => this.list.item_count().saturating_sub(1),
                    _ => return,
                };
                this.list.scroll_to(ListOffset { item_ix: next.min(this.list.item_count().saturating_sub(1)), offset_in_item: px(0.) });
                cx.notify(); cx.stop_propagation();
            }))
            .child(canvas(move |bounds, window, cx| {
                entity.update(cx, |this, cx| {
                    this.bounds = bounds;
                    let overflowing = this.overflowing();
                    if this.show_rail != overflowing {
                        this.show_rail = overflowing;
                        if !overflowing && this.grab.take().is_some() { this.list.scrollbar_drag_ended(); }
                        cx.notify();
                    }
                    let (top, height) = this.geometry();
                    let mut content = div().relative().size_full();
                    if this.overflowing() {
                        content = content.child(div().absolute().top(top).left(border::strong()).right(border::strong()).h(height)
                            .border(border::strong()).border_color(color::ink())
                            .bg(if this.grab.is_some() { color::white() } else { color::ink() }));
                        if this.grab.is_some() {
                            content = content.child(div().absolute().right(bounds.size.width + space::sm())
                                .top((top + height / 2. - crate::theme::scrollbar::readout_height() / 2.).clamp(px(0.), (bounds.size.height - crate::theme::scrollbar::readout_height()).max(px(0.))))
                                .h(crate::theme::scrollbar::readout_height()).px(space::sm()).flex().items_center()
                                .bg(color::ink()).text_color(color::lime()).font_family(font::MONO).text_size(text::label()).font_weight(FontWeight::BOLD)
                                .whitespace_nowrap().child(format!("{} / {}", (this.list.logical_scroll_top().item_ix + 1).min(this.list.item_count()), this.list.item_count())));
                        }
                    }
                    let mut element = content.into_any_element();
                    element.layout_as_root(bounds.size.map(AvailableSpace::Definite), window, cx);
                    element.prepaint_at(bounds.origin, window, cx);
                    element
                })
            }, move |_, mut element, window, cx| {
                element.paint(window, cx);
                let moving = weak.clone();
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble { let _ = moving.update(cx, |this, cx| {
                        if this.grab.is_some() { this.pointer(e.position.y, cx); }
                    }); }
                });
                let ending = weak.clone();
                window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble && e.button == MouseButton::Left { let _ = ending.update(cx, |this, cx| {
                        if this.grab.take().is_some() { this.list.scrollbar_drag_ended(); cx.notify(); }
                    }); }
                });
            }).size_full())
    }
}

/// A stationary play button covering the row number on hover or keyboard focus.
pub fn track_play_button(id: impl Into<ElementId>) -> Stateful<Div> {
    track_action(div().id(id))
        .tab_index(0)
        .size(size::row_action())
        .flex()
        .items_center()
        .justify_center()
        .border(border::strong())
        .border_color(color::ink())
        .bg(color::surface())
        .cursor_pointer()
        .opacity(0.)
        .group_hover("track-row", |s| s.opacity(1.))
        .hover(|s| s.bg(color::white()))
        .active(|s| s.bg(color::canvas()))
        .focus(|s| s.opacity(1.).shadow(vec![shadow::focus_ring()]))
        .child(icon("player-play", false).text_color(color::ink()))
}

/// Persistent ruled scrollbar for the catalog's non-virtualized sections.
pub struct CatalogScrollbar {
    pub scroll: ScrollHandle,
    bounds: Bounds<Pixels>,
    grab: Option<Pixels>,
    show_rail: bool,
}
impl CatalogScrollbar {
    pub fn new() -> Self {
        Self {
            scroll: ScrollHandle::new(),
            bounds: Bounds::default(),
            grab: None,
            show_rail: false,
        }
    }
    fn geometry(&self) -> (Pixels, Pixels) {
        let height = self.bounds.size.height;
        let viewport = self.scroll.bounds().size.height;
        let max = self.scroll.max_offset().height;
        let thumb = (height * (viewport / (viewport + max).max(px(1.))))
            .max(crate::theme::scrollbar::min_thumb())
            .min(height);
        (
            (height - thumb) * (-self.scroll.offset().y / max.max(px(1.))).clamp(0., 1.),
            thumb,
        )
    }
    fn pointer(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let (_, thumb) = self.geometry();
        let travel = self.bounds.size.height - thumb;
        if travel > px(0.) {
            let f =
                ((y - self.bounds.top() - self.grab.unwrap_or(thumb / 2.)) / travel).clamp(0., 1.);
            self.scroll
                .set_offset(point(px(0.), -self.scroll.max_offset().height * f));
            cx.notify();
        }
    }
}
impl Default for CatalogScrollbar {
    fn default() -> Self {
        Self::new()
    }
}
impl Render for CatalogScrollbar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let weak = entity.downgrade();
        div()
            .id("catalog-scrollbar")
            .relative()
            .w(if self.show_rail { crate::theme::scrollbar::width() } else { px(0.) })
            .h_full()
            .flex_shrink_0()
            .when(self.show_rail, |s| s.bg(color::white()).border_l(border::strong()).border_color(color::ink()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, cx| {
                    let (top, thumb) = this.geometry();
                    let y = e.position.y - this.bounds.top();
                    this.grab = Some(if y >= top && y <= top + thumb {
                        y - top
                    } else {
                        thumb / 2.
                    });
                    this.pointer(e.position.y, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                canvas(
                    move |bounds, window, cx| {
                        entity.update(cx, |this, cx| {
                            this.bounds = bounds;
                            let overflowing = this.scroll.max_offset().height > px(0.);
                            if this.show_rail != overflowing {
                                this.show_rail = overflowing;
                                if !overflowing { this.grab = None; }
                                cx.notify();
                            }
                            let (top, height) = this.geometry();
                            let mut d = div().relative().size_full();
                            if this.scroll.max_offset().height > px(0.) {
                                d = d.child(
                                    div()
                                        .absolute()
                                        .top(top)
                                        .left(border::strong())
                                        .right(border::strong())
                                        .h(height)
                                        .bg(color::ink()),
                                );
                            }
                            let mut el = d.into_any_element();
                            el.layout_as_root(
                                bounds.size.map(AvailableSpace::Definite),
                                window,
                                cx,
                            );
                            el.prepaint_at(bounds.origin, window, cx);
                            el
                        })
                    },
                    move |_, mut el, window, cx| {
                        el.paint(window, cx);
                        let moving = weak.clone();
                        window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble {
                                let _ = moving.update(cx, |this, cx| {
                                    if this.grab.is_some() {
                                        this.pointer(e.position.y, cx);
                                    }
                                });
                            }
                        });
                        let ending = weak.clone();
                        window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble && e.button == MouseButton::Left {
                                let _ = ending.update(cx, |this, cx| {
                                    this.grab = None;
                                    cx.notify();
                                });
                            }
                        });
                    },
                )
                .size_full(),
            )
    }
}
