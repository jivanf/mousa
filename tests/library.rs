use mousa::db::{Library, Sort, Track};
fn track(title: &str) -> Track {
    Track {
        id: 0,
        url: "https://www.youtube.com/watch?v=test".into(),
        title: title.into(),
        artist: "Test artist".into(),
        duration: 180.,
    }
}
#[test]
fn crud_search_sort_and_duplicate_identity() {
    let mut db = Library::open(":memory:").unwrap();
    let id = db.create("Library").unwrap();
    db.add(id, &[track("Zulu"), track("Alpha"), track("Alpha")])
        .unwrap();
    let full = db.tracks(id, "", Sort::Position).unwrap();
    let found = db.tracks(id, "alp", Sort::Position).unwrap();
    assert_eq!(found.len(), 2);
    assert_ne!(found[0].id, found[1].id);
    let start = full.iter().position(|t| t.id == found[1].id).unwrap();
    assert_eq!(start, 2);
    assert_eq!(db.tracks(id, "", Sort::Title).unwrap()[0].title, "Alpha");
    for query in ["\"", "OR", "*", "foo:bar", "alpha OR zulu", "-alpha"] {
        assert!(db.tracks(id, query, Sort::Title).is_ok(), "{query}");
    }
    db.rename(id, "Renamed").unwrap();
    assert_eq!(db.playlists().unwrap()[0].name, "Renamed");
    db.remove(found[0].id).unwrap();
    assert_eq!(db.tracks(id, "alpha", Sort::Title).unwrap().len(), 1);
    db.delete(id).unwrap();
    assert!(db.tracks(id, "alpha", Sort::Title).unwrap().is_empty());
    assert!(db.playlists().unwrap().is_empty());
}
#[test]
fn transactional_import_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let id;
    {
        let mut db = Library::open(&path).unwrap();
        id = db.create("Persisted").unwrap();
        db.add(id, &[track("One")]).unwrap();
        assert!(db.add(999, &[track("Invalid")]).is_err());
    }
    let db = Library::open(&path).unwrap();
    assert_eq!(db.tracks(id, "", Sort::Position).unwrap().len(), 1);
}
#[test]
#[ignore = "100,000-track performance probe; run explicitly"]
fn large_playlist() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Library::open(dir.path().join("bench.db")).unwrap();
    let id = db.create("100k").unwrap();
    let tracks: Vec<_> = (0..100_000)
        .map(|i| track(&format!("Song {i:06}")))
        .collect();
    let start = std::time::Instant::now();
    db.add(id, &tracks).unwrap();
    println!("Import 100k: {:?}", start.elapsed());
    for (query, sort) in [
        ("", Sort::Position),
        ("", Sort::Title),
        ("099999", Sort::Position),
    ] {
        let start = std::time::Instant::now();
        let result = db.tracks(id, query, sort).unwrap();
        println!(
            "Query {query:?} {sort:?}: {:?}, {} rows",
            start.elapsed(),
            result.len()
        );
        assert_eq!(result.len(), if query.is_empty() { 100_000 } else { 1 });
    }
}

#[test]
fn reorder_preserves_entry_identity() {
    let mut db = Library::open(":memory:").unwrap();
    let id = db.create("Order").unwrap();
    db.add(id, &[track("A"), track("B"), track("C")]).unwrap();
    let rows = db.tracks(id, "", Sort::Position).unwrap();
    db.move_entry(rows[2].id, -1).unwrap();
    let moved = db.tracks(id, "", Sort::Position).unwrap();
    assert_eq!(
        moved.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
        ["A", "C", "B"]
    );
    assert_eq!(moved[1].id, rows[2].id);
    db.move_entry(rows[0].id, -1).unwrap();
    assert_eq!(db.tracks(id, "", Sort::Position).unwrap()[0].title, "A");
}

#[test]
fn representative_5000_track_playlist() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Library::open(dir.path().join("library.db")).unwrap();
    let id = db.create("5,000 tracks").unwrap();
    let tracks: Vec<_> = (0..5000).map(|i| track(&format!("Song {i:04}"))).collect();
    db.add(id, &tracks).unwrap();
    for (query, sort) in [
        ("", Sort::Position),
        ("", Sort::Title),
        ("4999", Sort::Position),
    ] {
        let start = std::time::Instant::now();
        let result = db.tracks(id, query, sort).unwrap();
        println!(
            "5k query {query:?} {sort:?}: {:?}, {} rows",
            start.elapsed(),
            result.len()
        );
        assert_eq!(result.len(), if query.is_empty() { 5000 } else { 1 });
    }
}

#[test]
fn deleted_entry_ids_are_never_reused() {
    let mut db = Library::open(":memory:").unwrap();
    let id = db.create("Identity").unwrap();
    db.add(id, &[track("Old")]).unwrap();
    let old = db.tracks(id, "", Sort::Position).unwrap()[0].id;
    db.remove(old).unwrap();
    db.add(id, &[track("New")]).unwrap();
    assert_ne!(old, db.tracks(id, "", Sort::Position).unwrap()[0].id);
}
