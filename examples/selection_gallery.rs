//! Shared production selection bars at the minimum desktop content width.
use gpui::{prelude::*, *};
use gpui_component::Root;
use mousa::{
    theme::{color, font, space, text},
    ui,
};
pub struct Gallery;
impl Render for Gallery {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut page = div()
            .size_full()
            .bg(color::canvas())
            .p(space::lg())
            .flex()
            .flex_col()
            .gap(space::xl())
            .font_family(font::SANS)
            .text_size(text::body())
            .text_color(color::ink());
        for (label, count, unavailable, owned) in [
            ("ALBUM", 4, 0, false),
            ("YOUR PLAYLIST", 3, 0, true),
            ("UNAVAILABLE", 4, 1, true),
            ("SINGLE", 1, 0, false),
        ] {
            let mut actions = div().flex().items_center().gap(space::md());
            for (name, icon) in [
                ("PLAY", "player-play"),
                ("QUEUE", "playlist-add"),
                ("SAVE TO PLAYLIST", "plus"),
            ] {
                actions = actions.child(ui::selection_action(name, name, icon, false));
            }
            if owned {
                actions = actions
                    .child(ui::selection_action("remove", "REMOVE", "x", true).ml(space::xl()));
            }
            page = page.child(
                div()
                    .id(label)
                    .child(ui::caps(label))
                    .child(ui::selection_bar(
                        count,
                        unavailable,
                        true,
                        actions,
                        ui::selection_action("clear", "CLEAR", "x", false),
                    )),
            );
        }
        page
    }
}
fn main() {
    Application::new().with_assets(ui::Assets).run(|cx| {
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
                    size(px(800.), px(600.)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Mousa Selection Variants".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let gallery = cx.new(|_| Gallery);
                cx.new(|cx| Root::new(gallery, window, cx))
            },
        )
        .unwrap();
    });
}
