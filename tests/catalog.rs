use mousa::catalog::{self, Item};
use serde_json::json;

fn fixture(name: &str) -> catalog::Page {
    let raw = std::fs::read_to_string(format!(
        "{}/tests/fixtures/catalog/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    catalog::parse(
        &serde_json::from_str(&raw).unwrap(),
        if name.ends_with("detail") {
            "browse"
        } else {
            "search"
        },
    )
}

#[test]
fn live_response_fixtures_cover_every_search_category() {
    for (name, expected) in [
        ("songs", "Song"),
        ("videos", "Video"),
        ("albums", "Album"),
        ("artists", "Artist"),
        ("community_playlists", "Playlist"),
        ("featured_playlists", "Playlist"),
        ("podcasts", "Podcast"),
        ("episodes", "Episode"),
        ("profiles", "Profile"),
    ] {
        let page = fixture(name);
        assert!(!page.items.is_empty(), "{name}");
        assert!(
            page.items.iter().all(|i| i.kind == expected
                || (expected == "Album" && ["Single", "EP"].contains(&i.kind.as_str()))),
            "{name}: {:?}",
            page.items.iter().map(|i| &i.kind).collect::<Vec<_>>()
        );
        assert!(
            page.items.iter().all(|i| i.thumbnail.is_some()),
            "{name}: missing thumbnails"
        );
        assert!(
            page.items.iter().all(|i| i.destination.is_some()),
            "{name}: missing destinations"
        );
        if ["Song", "Video", "Episode"].contains(&expected) {
            assert!(
                page.items.iter().all(|i| i.video.is_some()),
                "{name}: missing playable ID"
            );
        }
    }
    let page = fixture("all");
    assert_eq!(page.filters.len(), 9);
    for (_, destination) in page.filters {
        assert!(destination.body["params"].is_string());
    }
}

#[test]
fn details_have_titles_and_navigable_content() {
    for name in [
        "albums_detail",
        "artists_detail",
        "community_playlists_detail",
        "podcasts_detail",
        "profiles_detail",
    ] {
        let page = fixture(name);
        assert!(!page.title.is_empty(), "{name}: missing title");
        assert!(!page.items.is_empty(), "{name}: missing contents");
        if [
            "albums_detail",
            "community_playlists_detail",
            "podcasts_detail",
        ]
        .contains(&name)
        {
            assert!(
                page.items.iter().any(|i| i.track().is_some()),
                "{name}: no playable contents"
            );
        }
    }
}

#[test]
fn unknown_types_and_missing_optional_metadata_remain_visible() {
    let page = catalog::parse(
        &json!({"musicShelfRenderer":{"title":{"runs":[{"text":"New category"}]},"contents":[{"musicTwoRowItemRenderer":{"title":{"runs":[{"text":"Future item"}]},"navigationEndpoint":{"browseEndpoint":{"browseId":"future"}}}}]}}),
        "search",
    );
    assert_eq!(page.items[0].kind, "New category");
    assert_eq!(page.items[0].duration, 0.);
    assert!(page.items[0].thumbnail.is_none());
    assert!(page.items[0].destination.is_some());
}

#[test]
fn transient_tracks_do_not_share_library_or_queue_identity() {
    let item = Item {
        video: Some("test".into()),
        title: "Title".into(),
        artist: "Full artist credit".into(),
        ..Default::default()
    };
    let a = item.track().unwrap();
    let b = item.track().unwrap();
    assert!(a.id < 0 && b.id < 0);
    assert_ne!(a.id, b.id);
    assert_eq!(a.artist, "Full artist credit");
    assert_eq!(a.url, "https://www.youtube.com/watch?v=test");
    assert!(
        Item {
            unavailable: true,
            ..item
        }
        .track()
        .is_none()
    );
}

#[test]
fn continuation_tokens_are_opaque_and_cancellation_does_not_fetch() {
    let page = catalog::parse(
        &json!({"musicShelfContinuation":{"contents":[],"continuations":[{"nextContinuationData":{"continuation":"opaque+/="}}]}}),
        "browse",
    );
    assert_eq!(page.more[0].1.body["continuation"], "opaque+/=");
    assert!(catalog::collection(&page, || true, |_| panic!("must not load")).is_err());
}

#[test]
fn collection_excludes_recommendations() {
    let page = fixture("albums_detail");
    let items = catalog::collection(&page, || false, |_| {}).unwrap();
    assert!(!items.is_empty());
    assert!(items.iter().all(|i| i.collection_member));
    assert!(items.len() < page.items.len());
}

#[test]
fn superseded_requests_stop_before_transport() {
    let result = catalog::fetch_cancellable(&catalog::search("unused"), || true);
    assert!(result.unwrap_err().to_string().contains("Superseded"));
}

#[test]
fn search_metadata_saves_locally_without_enrichment() {
    let page = fixture("songs");
    let track = page.items[0].track().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let mut db = mousa::db::Library::open(temp.path().join("library.db")).unwrap();
    let id = db.create("Catalog selection").unwrap();
    db.add(id, &[track.clone()]).unwrap();
    let saved = db.tracks(id, "", mousa::db::Sort::Position).unwrap();
    assert_eq!(saved[0].url, track.url);
    assert_eq!(saved[0].artist, track.artist);
    assert_eq!(saved[0].duration, track.duration);
    assert!(saved[0].id > 0);
}

#[test]
#[ignore = "read-only live YouTube Music compatibility probe"]
fn live_catalog_categories_and_pagination() {
    let all = catalog::fetch(&catalog::search("bach")).unwrap();
    assert!(!all.items.is_empty());
    for (label, mut destination) in all.filters {
        if label == "Podcasts" || label == "Episodes" {
            destination.body["query"] = "science".into();
        }
        if label == "Profiles" {
            destination.body["query"] = "TaylorSwift".into();
        }
        let page = catalog::fetch(&destination).unwrap();
        assert!(!page.items.is_empty(), "{label}");
        println!("{label}: {} items", page.items.len());
        if let Some((_, d)) = page
            .more
            .iter()
            .find(|(_, d)| d.body.get("continuation").is_some())
        {
            let next = catalog::fetch(d).unwrap();
            assert!(!next.items.is_empty(), "{label}: empty continuation");
        }
    }
}

#[test]
fn detail_metadata_and_creator_navigation_survive_parsing() {
    let page = fixture("albums_detail");
    assert!(page.metadata.contains("30 songs"));
    assert!(page.metadata.contains("2 hours"));
    let page = catalog::parse(
        &json!({"musicResponsiveHeaderRenderer":{
            "title":{"runs":[{"text":"Example album"}]},
            "straplineText":{"runs":[{"text":"Example artist","navigationEndpoint":{"browseEndpoint":{"browseId":"UCcreator"}}}]},
            "secondSubtitle":{"runs":[{"text":"12 tracks"}]}
        }}),
        "browse",
    );
    assert_eq!(page.byline, "Example artist");
    assert_eq!(page.creator.unwrap().body["browseId"], "UCcreator");
    assert_eq!(page.metadata, "12 tracks");
}

#[test]
fn song_results_navigate_to_their_album_not_the_watch_or_artist_endpoint() {
    for name in ["songs", "all"] {
        let page = fixture(name);
        let songs: Vec<_> = page.items.iter().filter(|i| i.kind == "Song").collect();
        assert!(!songs.is_empty());
        for song in songs {
            let Some(album) = song.album.as_ref() else {
                assert_eq!(name, "all", "filtered songs include album links");
                assert!(song.search_target().is_none());
                continue;
            };
            assert!(!album.title.is_empty());
            let target = song.search_target().unwrap();
            assert_eq!(target.kind, "Album");
            assert_eq!(target.title, album.title);
            assert_eq!(target.destination.as_ref().unwrap(), &album.destination);
            assert_eq!(album.destination.api, "browse");
            assert!(target.video.is_none());
            assert!(song.video.is_some(), "explicit Play remains available");
        }
    }
    let album = fixture("albums").items.remove(0);
    assert_eq!(
        album.search_target().unwrap().destination,
        album.destination
    );
}

#[test]
fn missing_album_metadata_never_falls_back_to_playback_navigation() {
    let song = Item {
        kind: "Song".into(),
        video: Some("song".into()),
        destination: Some(catalog::Destination {
            api: "next".into(),
            body: json!({"videoId":"song"}),
        }),
        ..Default::default()
    };
    assert!(song.search_target().is_none());
    assert!(song.track().is_some());
    let old_cache = serde_json::to_value(&song).unwrap();
    let mut old_cache = old_cache.as_object().unwrap().clone();
    old_cache.remove("album");
    assert!(
        serde_json::from_value::<Item>(old_cache.into())
            .unwrap()
            .album
            .is_none()
    );
}

#[test]
fn next_metadata_preserves_the_current_songs_album_link() {
    let page = catalog::parse(
        &json!({"playlistPanelVideoRenderer": {
            "title": {"runs":[{"text":"Song"}]}, "videoId":"current",
            "longBylineText":{"runs":[{"text":"Album title", "navigationEndpoint":{
                "browseEndpoint":{"browseId":"album-id", "browseEndpointContextSupportedConfigs":{
                    "browseEndpointContextMusicConfig":{"pageType":"MUSIC_PAGE_TYPE_ALBUM"}
                }}
            }}]}
        }}),
        "next",
    );
    let song = &page.items[0];
    assert_eq!(song.video.as_deref(), Some("current"));
    let album = song.album.as_ref().unwrap();
    assert_eq!(album.title, "Album title");
    assert_eq!(album.destination.body["browseId"], "album-id");
}
