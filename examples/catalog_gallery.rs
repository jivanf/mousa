//! Offline visual review of the production catalog components.
//! Usage: cargo run --release --example catalog_gallery -- [search|album|playlist|artist|empty|first-use|loading|no-results|error]
use gpui::{prelude::*, *};
use gpui_component::Root;
use mousa::{
    catalog::{self, Item, Page},
    catalog_view::CatalogView,
    player::{Player, Snapshot},
    theme::{color, font, text},
    ui,
};
use std::sync::{Arc, Mutex, mpsc};
struct Gallery {
    catalog: Entity<CatalogView>,
}
impl Render for Gallery {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(color::canvas())
            .font_family(font::SANS)
            .font_weight(FontWeight::MEDIUM)
            .text_size(text::body())
            .text_color(color::ink())
            .child(self.catalog.clone())
    }
}
fn sample(state: &str) -> (Page, Option<Item>) {
    let raw: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/catalog/all.json")).unwrap();
    let live = catalog::parse(&raw, "search");
    let cover = live
        .items
        .iter()
        .find(|i| i.kind == "Album")
        .and_then(|i| i.thumbnail.clone());
    let thumb = live
        .items
        .iter()
        .find(|i| i.kind == "Artist")
        .and_then(|i| i.thumbnail.clone());
    let mut page = Page {
        filters: live.filters,
        ..Default::default()
    };
    if ["empty", "first-use", "loading", "no-results", "error"].contains(&state) {
        return (page, None);
    }
    let names = [
        ("Johann Sebastian Bach", "2.1M subscribers", "Artist", 0.),
        (
            "Bach: Sacred Cantatas",
            "Karl Richter · 1978 · 24 tracks",
            "Album",
            0.,
        ),
        (
            "Jesu, meine Freude, BWV 227: I. Chorus",
            "Karl Richter · Bach: Motets",
            "Song",
            312.,
        ),
        (
            "Air on the G String",
            "Neville Marriner · Bach: Orchestral Suites",
            "Song",
            333.,
        ),
        (
            "J.S. Bach: Toccata and Fugue in D minor, BWV 565",
            "Organ Channel · 4.2M views",
            "Video",
            543.,
        ),
        (
            "Bach for Studying",
            "Classical Focus · 214 tracks",
            "Playlist",
            0.,
        ),
        (
            "Understanding Bach",
            "Classical Conversations · Oct 12",
            "Episode",
            3130.,
        ),
        (
            "Brandenburg Concertos",
            "Trevor Pinnock · 1982 · 18 tracks",
            "Album",
            0.,
        ),
    ];
    page.items = names
        .into_iter()
        .map(|(title, subtitle, kind, duration)| Item {
            title: title.into(),
            subtitle: subtitle.into(),
            kind: kind.into(),
            duration,
            thumbnail: if kind == "Artist" {
                thumb.clone()
            } else {
                cover.clone()
            },
            video: (duration > 0.).then(|| "preview-only".into()),
            collection_member: true,
            ..Default::default()
        })
        .collect();
    if state == "search" {
        return (page, None);
    }
    let kind = match state {
        "album" => "Album",
        "playlist" => "Playlist",
        _ => "Artist",
    };
    let title = match state {
        "album" => "Bach: Easter Cantatas",
        "playlist" => "Bach for Studying",
        _ => "Johann Sebastian Bach",
    };
    page.title = title.into();
    page.thumbnail = if state == "artist" {
        thumb
    } else {
        cover.clone()
    };
    page.byline = match state {
        "album" => "Karl Richter · Münchener Bach-Orchester",
        "playlist" => "Classical Focus",
        _ => "2.1M subscribers",
    }
    .into();
    page.metadata = match state {
        "album" => "1978 · 24 tracks · 1 h 12 min",
        "playlist" => "214 tracks · 14 h 2 min",
        _ => "",
    }
    .into();
    let tracks = if state == "album" {
        vec![
            ("Sinfonia", "", 122.),
            ("Chorus: Christ lag in Todesbanden", "", 281.),
            ("Aria: Den Tod niemand zwingen kunnt", "", 210.),
            ("Chorus: Jesus Christus, Gottes Sohn", "", 133.),
            ("Chorus: Es war ein wunderlich Krieg", "", 104.),
            ("Aria: Hier ist das rechte Osterlamm", "", 260.),
            ("Chorale: Wir essen und leben wohl", "", 72.),
        ]
    } else if state == "playlist" {
        vec![
            ("Air on the G String", "Neville Marriner", 333.),
            ("Goldberg Variations, BWV 988: Aria", "Glenn Gould", 184.),
            ("Cello Suite No. 1: I. Prélude", "Yo-Yo Ma", 141.),
            ("Jesu, Joy of Man’s Desiring", "Myra Hess", 226.),
            (
                "Brandenburg Concerto No. 3: I. Allegro",
                "Trevor Pinnock",
                362.,
            ),
            ("Toccata and Fugue in D minor", "Helmut Walcha", 543.),
            ("Partita No. 2: Sarabande", "Murray Perahia", 251.),
        ]
    } else {
        vec![
            (
                "Jesu, meine Freude, BWV 227: I. Chorus",
                "Bach: Motets",
                312.,
            ),
            ("Air on the G String", "Bach: Orchestral Suites", 333.),
            (
                "Toccata and Fugue in D minor, BWV 565",
                "Bach: Organ Works",
                543.,
            ),
            ("Cello Suite No. 1: I. Prélude", "Bach: Cello Suites", 141.),
            (
                "Goldberg Variations: Aria",
                "Bach: Goldberg Variations",
                184.,
            ),
        ]
    };
    page.items = tracks
        .into_iter()
        .cycle()
        .take(match state {
            "album" => 24,
            "playlist" => 214,
            _ => 5,
        })
        .map(|(title, artist, duration)| Item {
            title: title.into(),
            artist: artist.into(),
            subtitle: artist.into(),
            kind: "Song".into(),
            category: "Songs".into(),
            duration,
            video: Some("preview-only".into()),
            collection_member: true,
            ..Default::default()
        })
        .collect();
    if state == "artist" {
        for (title, year) in [
            ("Mass in B minor", "1985"),
            ("St Matthew Passion", "1980"),
            ("Goldberg Variations", "1981"),
            ("Brandenburg Concertos", "1982"),
        ] {
            page.items.push(Item {
                title: title.into(),
                subtitle: format!("Album · {year}"),
                kind: "Album".into(),
                category: "Albums".into(),
                thumbnail: cover.clone(),
                ..Default::default()
            });
        }
        page.more = vec![
            ("More Songs".into(), catalog::search("bach")),
            ("More Albums".into(), catalog::search("bach")),
        ];
    }
    (
        page,
        Some(Item {
            title: title.into(),
            kind: kind.into(),
            ..Default::default()
        }),
    )
}
fn main() {
    let state = std::env::args().nth(1).unwrap_or("search".into());
    let (page, item) = sample(&state);
    // Use cached thumbnail files only; no player, account, or network workers are started.
    let (tx, _rx) = mpsc::channel();
    let player = Player {
        tx,
        state: Arc::new(Mutex::new(Snapshot::default())),
    };
    Application::new().with_assets(ui::Assets).run(move |cx| {
        ui::init(cx);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let height = match state.as_str() {
            "album" => 760.,
            "playlist" => 820.,
            "artist" => 980.,
            _ => 750.,
        };
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1000.), px(height)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some(format!("Mousa Catalog — {state}").into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |window, cx| {
                let view =
                    cx.new(|cx| CatalogView::preview(player, page, item, &state, window, cx));
                let gallery = cx.new(|_| Gallery { catalog: view });
                cx.new(|cx| Root::new(gallery, window, cx))
            },
        )
        .unwrap();
    });
}
