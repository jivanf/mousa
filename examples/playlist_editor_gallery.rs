//! Offline production editor: pristine, dirty, saving, error, empty, discard, local.
use gpui::{prelude::*, *};
use gpui_component::Root;
use mousa::{
    playlist_editor::PlaylistEditor,
    theme::{color, font, text},
    ui,
};
struct Gallery {
    editor: Option<Entity<PlaylistEditor>>,
    _subscription: Subscription,
}
impl Render for Gallery {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .bg(color::canvas())
            .font_family(font::SANS)
            .text_size(text::body())
            .child(div().p_8().child("Mousa · Playlist editor preview"))
            .children(self.editor.clone())
    }
}
fn main() {
    let state = std::env::args().nth(1).unwrap_or_else(|| "dirty".into());
    Application::new().with_assets(ui::Assets).run(move |cx| {
        ui::init(cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1200.), px(700.)),
                    cx,
                ))),
                ..Default::default()
            },
            move |w, cx| {
                w.set_window_title(&format!("Playlist editor · {state}"));
                let editor = cx.new(|cx| PlaylistEditor::preview(&state, w, cx));
                let gallery = cx.new(|cx| {
                    let subscription = cx.subscribe(
                        &editor,
                        |this: &mut Gallery, _, _: &mousa::playlist_editor::Event, cx| {
                            this.editor = None;
                            cx.notify();
                        },
                    );
                    Gallery {
                        editor: Some(editor),
                        _subscription: subscription,
                    }
                });
                cx.new(|cx| Root::new(gallery, w, cx))
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
