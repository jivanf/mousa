//! Deterministic component review; never touches playback, accounts, or the library.
use gpui::{prelude::*, *};
use gpui_component::{Root, input::InputState};
use mousa::{
    theme::{color, font, layout, press, shadow, size as control_size, space, text},
    ui::{self, Bar, BarChange, button, icon_button, step_button},
};
pub struct Gallery {
    state: String,
    track_list: ListState,
    scrollbar: Entity<ui::TrackScrollbar>,
    seek: Entity<Bar>,
    volume: Entity<Bar>,
    field: Entity<InputState>,
    playing: bool,
    shuffle: bool,
    repeat: bool,
    volume_value: f32,
    elapsed: f32,
    subscriptions: Vec<Subscription>,
    only_player: bool,
}
impl Gallery {
    pub fn new(state: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let seek = cx.new(|cx| Bar::new("Seek", 0.01, cx));
        seek.update(cx, |s, cx| s.sync(14. / 240., false, cx));
        let volume = cx.new(|cx| Bar::new("Volume", 0.05, cx));
        volume.update(cx, |s, cx| s.sync(0.48, false, cx));
        let field = cx.new(|cx| InputState::new(window, cx).placeholder("Search this playlist…"));
        let subscriptions = vec![
            cx.subscribe(&volume, |this, _, e: &BarChange, cx| {
                this.volume_value = e.0;
                cx.notify();
            }),
            cx.subscribe(&seek, |this, _, e: &BarChange, cx| {
                this.elapsed = e.0 * 240.;
                cx.notify();
            }),
        ];
        let short = state == "scrollbar-short";
        let track_list = ListState::new(if short { 2 } else { 1230 }, ListAlignment::Top, layout::list_overscan());
        track_list.scroll_to(ListOffset {
            item_ix: if short { 0 } else { 154 },
            offset_in_item: px(0.),
        });
        let scrollbar = cx.new(|cx| ui::TrackScrollbar::new(track_list.clone(), cx));
        Self {
            track_list,
            scrollbar,
            seek,
            volume,
            field,
            playing: false,
            shuffle: false,
            repeat: true,
            volume_value: 0.48,
            elapsed: 14.,
            subscriptions,
            only_player: state == "player",
            state: state.into(),
        }
    }
}
impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if matches!(self.state.as_str(), "scrollbar" | "scrollbar-short") {
            return div().size_full().p(space::xxl()).bg(color::canvas()).font_family(font::SANS).text_size(text::body()).text_color(color::ink())
                .child(div().flex().w_full().h(px(228.)).border(mousa::theme::border::strong()).border_color(color::ink())
                    .child(div().flex_1().min_w_0().h_full().child(list(self.track_list.clone(), |i, _, _| {
                        ui::track_row(i, false).child(format!("{}", i + 1))
                            .child(div().flex_1().child("J.S. Bach: Jesu, der du meine Seele, Cantata BWV 78")
                                .child(div().text_size(text::secondary()).text_color(color::muted()).child("Münchener Bach-Orchester · Karl Richter · Münchener Bach-Chor")))
                            .child("5:12").into_any_element()
                    }).size_full()))
                    .child(self.scrollbar.clone())).into_any_element();
        }
        if self.state == "landing" {
            return ui::landing(ui::google_sign_in(false), None).into_any_element();
        }
        if self.state == "accounts" {
            return div()
                .size_full()
                .bg(color::canvas())
                .p(space::xxl())
                .font_family(font::SANS)
                .flex()
                .flex_col()
                .gap(space::xl())
                .child(ui::account_button(
                    "google-preview",
                    "google",
                    Some("Alex Morgan"),
                    false,
                ))
                .child(ui::account_button(
                    "lastfm-preview",
                    "lastfm",
                    Some("alexmusic"),
                    false,
                ))
                .child(ui::account_button(
                    "lastfm-disconnected",
                    "lastfm",
                    None,
                    false,
                ))
                .child(ui::account_button(
                    "account-busy",
                    "google",
                    Some("Alex Morgan"),
                    true,
                ))
                .into_any_element();
        }
        let _ = &self.subscriptions;
        let controls = div()
            .flex()
            .items_center()
            .justify_center()
            .gap(space::md())
            .child(
                icon_button(
                    "shuffle",
                    "arrows-shuffle",
                    "Shuffle",
                    self.shuffle,
                    false,
                    false,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.shuffle = !this.shuffle;
                    cx.notify();
                })),
            )
            .child(
                icon_button(
                    "previous",
                    "player-skip-back",
                    "Previous",
                    false,
                    false,
                    false,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.elapsed = 0.;
                    this.seek.update(cx, |s, cx| s.sync(0., false, cx));
                    cx.notify();
                })),
            )
            .child(
                step_button("back", "−10").on_click(cx.listener(|this, _, _, cx| {
                    this.elapsed = (this.elapsed - 10.).max(0.);
                    this.seek
                        .update(cx, |s, cx| s.sync(this.elapsed / 240., false, cx));
                    cx.notify();
                })),
            )
            .child(
                icon_button(
                    "play",
                    if self.playing {
                        "player-pause"
                    } else {
                        "player-play"
                    },
                    "Play / pause",
                    false,
                    true,
                    false,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.playing = !this.playing;
                    cx.notify();
                })),
            )
            .child(
                step_button("forward", "+10").on_click(cx.listener(|this, _, _, cx| {
                    this.elapsed = (this.elapsed + 10.).min(240.);
                    this.seek
                        .update(cx, |s, cx| s.sync(this.elapsed / 240., false, cx));
                    cx.notify();
                })),
            )
            .child(
                icon_button("next", "player-skip-forward", "Next", false, false, false).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.elapsed = 0.;
                        this.seek.update(cx, |s, cx| s.sync(0., false, cx));
                        cx.notify();
                    }),
                ),
            )
            .child(
                icon_button("repeat", "repeat", "Repeat", self.repeat, false, false).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.repeat = !this.repeat;
                        cx.notify();
                    }),
                ),
            );
        let info = div()
            .child(ui::caps("NOW PLAYING").mb(space::sm()))
            .child(
                div()
                    .text_size(text::track_title())
                    .font_weight(FontWeight::BOLD)
                    .child("Ach Gott, vom Himmel sieh darein, BWV 2: No. 1, Chor"),
            )
            .child(
                div()
                    .mt(space::sm())
                    .text_color(color::muted())
                    .child("Nikolaus Harnoncourt"),
            );
        let timeline = div()
            .flex()
            .items_center()
            .gap(space::sm())
            .font_family(font::MONO)
            .font_weight(FontWeight::BOLD)
            .child(div().w(layout::time_label()).child(format!(
                "{}:{:02}",
                self.elapsed as u32 / 60,
                self.elapsed as u32 % 60
            )))
            .child(div().flex_1().child(self.seek.clone()))
            .child(div().w(layout::time_label()).text_right().child("4:00"));
        let volume = div()
            .w(layout::player_volume())
            .flex()
            .items_center()
            .gap(space::sm())
            .child(
                icon_button(
                    "mute",
                    if self.volume_value == 0. {
                        "volume-off"
                    } else {
                        "volume"
                    },
                    "Mute",
                    self.volume_value == 0.,
                    false,
                    false,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.volume_value = if this.volume_value == 0. { 0.48 } else { 0. };
                    this.volume
                        .update(cx, |s, cx| s.sync(this.volume_value, false, cx));
                    cx.notify();
                })),
            )
            .child(div().flex_1().min_w_0().child(self.volume.clone()))
            .child(
                div()
                    .w(layout::time_label())
                    .font_family(font::MONO)
                    .font_weight(FontWeight::BOLD)
                    .child(format!("{}%", (self.volume_value * 100.).round() as u32)),
            );
        let mut root = div()
            .size_full()
            .tab_group()
            .bg(color::canvas())
            .text_color(color::ink())
            .font_family(font::SANS)
            .text_size(text::body())
            .font_weight(FontWeight::MEDIUM)
            .flex()
            .flex_col();
        if !self.only_player {
            root = root.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p(space::xxl())
                    .flex()
                    .flex_col()
                    .gap(space::xl())
                    .child(
                        div()
                            .text_size(text::page_title())
                            .child("Mousa / component gallery"),
                    )
                    .child(div().text_color(color::muted()).child(
                        "Press and hold to animate · Release to return · Enter/Space also work · 150ms each way",
                    ))
                    .child(
                        div()
                            .flex()
                            .gap(space::lg())
                            .child(button("default", "DEFAULT", color::surface()))
                            .child(button("hover", "HOVER", color::white()))
                            .child(
                                button("pressed", "PRESSED", color::surface())
                                    .top(press::sm())
                                    .left(press::sm())
                                    .shadow(vec![]),
                            )
                            .child(
                                button("focused", "FOCUSED", color::surface())
                                    .shadow(vec![shadow::focus_ring()]),
                            )
                            .child(ui::disabled_button("DISABLED"))
                            .child(button("on", "ON", color::ink()))
                            .child(button("delete", "DELETE", color::pink())),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(space::xl())
                            .child(
                                div()
                                    .w(control_size::sidebar())
                                    .flex()
                                    .flex_col()
                                    .gap(space::sm())
                                    .child(ui::search_input(&self.field, false, cx))
                                    .child(ui::playlist_item(1, "Bach / Choral works".into(), true))
                                    .child(ui::playlist_item(2, "Evening listening".into(), false)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        ui::track_row(0, false)
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .child("Quiet row · hover to reveal actions"),
                                            )
                                            .child(icon_button(
                                                "row-up", "arrow-up", "Move up", false, false, true,
                                            ))
                                            .child(icon_button(
                                                "row-add",
                                                "playlist-add",
                                                "Add to queue",
                                                false,
                                                false,
                                                true,
                                            )),
                                    )
                                    .child(
                                        ui::track_row(1, true)
                                            .child(div().flex_1().child("Playing row / lime"))
                                            .child("3:42"),
                                    )
                                    .child(
                                        ui::track_row(2, false)
                                            .bg(color::ink())
                                            .text_color(color::lime())
                                            .child("Selected row / inverted"),
                                    ),
                            ),
                    ),
            );
        } else {
            root = root.child(div().flex_1());
        }
        root.child(ui::player_panel(info, controls, timeline, volume, false))
            .into_any_element()
    }
}
fn main() {
    let state = std::env::args().nth(1).unwrap_or_default().trim_start_matches("--").replace("player-only", "player");
    let only_player = state == "player";
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
                    size(px(1200.), px(if only_player { 250. } else { 800. })),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Mousa Design Gallery".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |window, cx| {
                let view = cx.new(|cx| Gallery::new(&state, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("open gallery");
    });
}
