use mousa::db::{Library, Sort, Track};
fn track(title: &str) -> Track {
    Track {
        id: 0,
        playlist_index: 0,
        unavailable: None,
        url: "https://www.youtube.com/watch?v=test".into(),
        title: title.into(),
        artist: "Test artist".into(),
        duration: 180.,
    }
}
#[test]
fn bulk_removal_preserves_survivor_order_and_rolls_back_on_failure() {
    let mut db = Library::open(":memory:").unwrap();
    let playlist = db.create("Bulk").unwrap();
    db.add(
        playlist,
        &[
            track("Same"),
            track("Same"),
            track("Third"),
            track("Fourth"),
        ],
    )
    .unwrap();
    let rows = db.tracks(playlist, "", Sort::Position).unwrap();
    db.conn.execute_batch(&format!("CREATE TRIGGER fail_delete BEFORE DELETE ON entries WHEN old.id={} BEGIN SELECT RAISE(ABORT, 'test failure'); END;", rows[2].id)).unwrap();
    assert!(db.remove_many(&[rows[0].id, rows[2].id]).is_err());
    assert_eq!(db.tracks(playlist, "", Sort::Position).unwrap(), rows);
    db.conn.execute_batch("DROP TRIGGER fail_delete").unwrap();
    db.remove_many(&[rows[2].id, rows[0].id, rows[0].id])
        .unwrap();
    let remaining = db.tracks(playlist, "", Sort::Position).unwrap();
    assert_eq!(
        remaining.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![rows[1].id, rows[3].id]
    );
    assert_eq!(
        db.tracks(playlist, "Same", Sort::Position).unwrap().len(),
        1
    );
    db.remove_many(&[]).unwrap();
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

#[test]
fn google_refresh_preserves_identity_duplicates_and_local_playlists() {
    let mut db = Library::open(":memory:").unwrap();
    let local = db.create("Local").unwrap();
    db.add(local, &[track("Offline")]).unwrap();
    let catalog = vec![("PLone".into(), "Cloud".into())];
    let cloud = db.reconcile_catalog("owner", &catalog).unwrap()[0].0;
    db.reconcile_entries(
        cloud,
        &[
            ("item-a".into(), track("Same")),
            ("item-b".into(), track("Same")),
        ],
    )
    .unwrap();
    let original = db.tracks(cloud, "", Sort::Position).unwrap();
    assert_ne!(original[0].id, original[1].id);
    assert_eq!(db.reconcile_catalog("owner", &catalog).unwrap()[0].0, cloud);
    db.reconcile_entries(
        cloud,
        &[
            ("item-b".into(), track("Changed")),
            ("item-a".into(), track("Same")),
        ],
    )
    .unwrap();
    let reordered = db.tracks(cloud, "", Sort::Position).unwrap();
    assert_eq!(reordered[0].id, original[1].id);
    assert_eq!(reordered[1].id, original[0].id);
    assert_eq!(db.tracks(cloud, "Changed", Sort::Title).unwrap().len(), 1);
    db.reconcile_entries(cloud, &[("item-b".into(), track("Changed"))])
        .unwrap();
    assert!(db.tracks(cloud, "Same", Sort::Title).unwrap().is_empty());
    db.reconcile_catalog("owner", &[]).unwrap();
    assert_eq!(db.playlists().unwrap().len(), 1);
    assert_eq!(db.tracks(local, "", Sort::Position).unwrap().len(), 1);
    assert_eq!(
        db.conn
            .query_row("SELECT count(*) FROM google_entries", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn google_accounts_are_isolated_and_failed_snapshot_rolls_back() {
    let mut db = Library::open(":memory:").unwrap();
    let one = db
        .reconcile_catalog("one", &[("PLone".into(), "One".into())])
        .unwrap()[0]
        .0;
    let two = db
        .reconcile_catalog("two", &[("PLtwo".into(), "Two".into())])
        .unwrap()[0]
        .0;
    db.reconcile_entries(one, &[("a".into(), track("Original"))])
        .unwrap();
    db.reconcile_entries(two, &[("b".into(), track("Other"))])
        .unwrap();
    assert!(
        db.reconcile_entries(
            one,
            &[
                ("a".into(), track("Changed")),
                ("b".into(), track("Conflict"))
            ]
        )
        .is_err()
    );
    assert_eq!(
        db.tracks(one, "", Sort::Position).unwrap()[0].title,
        "Original"
    );
    assert!(
        db.reconcile_entries(
            one,
            &[
                ("a".into(), track("Changed")),
                ("a".into(), track("Duplicate"))
            ]
        )
        .is_err()
    );
    assert_eq!(
        db.tracks(one, "", Sort::Position).unwrap()[0].title,
        "Original"
    );
    db.reconcile_catalog("one", &[]).unwrap();
    assert_eq!(db.google_playlist(two).unwrap().unwrap().1, "two");
    assert_eq!(db.tracks(two, "", Sort::Position).unwrap().len(), 1);
}

#[test]
fn google_5000_track_refresh_keeps_ids() {
    let mut db = Library::open(":memory:").unwrap();
    let id = db
        .reconcile_catalog("owner", &[("PLlarge".into(), "Large".into())])
        .unwrap()[0]
        .0;
    let mut items: Vec<_> = (0..5000)
        .map(|i| (format!("item-{i}"), track(&format!("Song {i:04}"))))
        .collect();
    db.reconcile_entries(id, &items).unwrap();
    let before = db.tracks(id, "", Sort::Position).unwrap();
    items.reverse();
    db.reconcile_entries(id, &items).unwrap();
    let after = db.tracks(id, "", Sort::Position).unwrap();
    assert_eq!(before.len(), after.len());
    assert_eq!(before[4999].id, after[0].id);
    assert_eq!(
        db.tracks(id, "4999", Sort::Position).unwrap()[0].id,
        after[0].id
    );
}

#[test]
fn recently_confirmed_writes_survive_stale_google_catalog() {
    let mut db = Library::open(":memory:").unwrap();
    let id = db
        .reconcile_catalog("owner", &[("PLnew".into(), "New".into())])
        .unwrap()[0]
        .0;
    db.rename(id, "Renamed").unwrap();
    db.mark_written(id).unwrap();
    assert!(db.recently_written(id).unwrap());
    db.reconcile_catalog("owner", &[("PLnew".into(), "Stale name".into())])
        .unwrap();
    assert_eq!(db.playlists().unwrap()[0].name, "Renamed");
    db.reconcile_catalog("owner", &[]).unwrap();
    assert_eq!(db.playlists().unwrap().len(), 1);
    db.append_google(id, "new-item", &track("New entry"))
        .unwrap();
    assert_eq!(db.tracks(id, "New", Sort::Position).unwrap().len(), 1);
    db.conn
        .execute(
            "INSERT INTO google_deleted VALUES('owner','PLnew',unixepoch())",
            [],
        )
        .unwrap();
    db.delete(id).unwrap();
    db.reconcile_catalog(
        "owner",
        &[("PLnew".into(), "Stale deleted playlist".into())],
    )
    .unwrap();
    assert!(db.playlists().unwrap().is_empty());
    db.conn
        .execute("UPDATE google_deleted SET changed=unixepoch()-61", [])
        .unwrap();
    assert_eq!(
        db.reconcile_catalog("owner", &[("PLnew".into(), "Visible again".into())])
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn google_refresh_backfills_duration_without_changing_entry_identity() {
    let mut db = Library::open(":memory:").unwrap();
    let id = db.create("Durations").unwrap();
    let mut unknown = track("Previously synced");
    unknown.duration = 0.;
    db.reconcile_entries(id, &[("remote-item".into(), unknown.clone())])
        .unwrap();
    let original = db.tracks(id, "", Sort::Position).unwrap()[0].id;
    let mut known = unknown.clone();
    known.duration = 253.;
    db.reconcile_entries(id, &[("remote-item".into(), known)])
        .unwrap();
    let updated = db.tracks(id, "", Sort::Position).unwrap();
    assert_eq!(updated[0].id, original);
    assert_eq!(updated[0].duration, 253.);
    db.reconcile_entries(id, &[("remote-item".into(), unknown)])
        .unwrap();
    assert_eq!(db.tracks(id, "", Sort::Position).unwrap()[0].duration, 253.);
}

#[test]
fn opening_and_searching_during_refresh_reads_committed_playlist() {
    use std::{sync::mpsc, time::Duration};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("concurrent.db");
    let mut writer = Library::open(&path).unwrap();
    let id = writer.create("Cached playlist").unwrap();
    writer.add(id, &[track("Cached song")]).unwrap();
    let tx = writer
        .conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute(
        "UPDATE entries SET title='Refreshed song' WHERE playlist_id=?",
        [id],
    )
    .unwrap();
    let (send, receive) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let result = (|| -> anyhow::Result<_> {
            let db = Library::open(path)?;
            Ok((
                db.tracks(id, "", Sort::Position)?,
                db.tracks(id, "cached", Sort::Position)?,
            ))
        })();
        send.send(result).unwrap();
    });
    // The writer remains uncommitted until the playback/search reader finishes.
    let result = receive.recv_timeout(Duration::from_secs(2));
    tx.commit().unwrap();
    reader.join().unwrap();
    let (playlist, search) = result
        .expect("reads must not wait for a refresh writer")
        .unwrap();
    assert_eq!(playlist[0].title, "Cached song");
    assert_eq!(search.len(), 1);
    assert_eq!(
        writer
            .tracks(id, "refreshed", Sort::Position)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn competing_read_then_write_operations_wait_before_taking_a_snapshot() {
    use std::{sync::mpsc, time::Duration};
    for operation in ["add", "catalog", "entries", "append"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("writers.db");
        let mut first = Library::open(&path).unwrap();
        let id = first.create("Existing").unwrap();
        let mut second = Library::open(&path).unwrap();
        let tx = first
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute("INSERT INTO settings VALUES('session','pending')", [])
            .unwrap();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            let result = match operation {
                "add" => second.add(id, &[track("Added")]),
                "catalog" => second
                    .reconcile_catalog("account", &[("remote".into(), "Remote".into())])
                    .map(|_| ()),
                "entries" => second.reconcile_entries(id, &[("entry".into(), track("Synced"))]),
                "append" => second.append_google(id, "entry", &track("Appended")),
                _ => unreachable!(),
            };
            done_tx.send(result).unwrap();
        });
        ready_rx.recv().unwrap();
        let early = done_rx.recv_timeout(Duration::from_millis(100));
        tx.commit().unwrap();
        worker.join().unwrap();
        assert!(
            matches!(early, Err(mpsc::RecvTimeoutError::Timeout)),
            "{operation} should wait for the writer instead of failing a read-to-write upgrade: {early:?}"
        );
        done_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        assert_eq!(first.load("session").as_deref(), Some("pending"));
        if operation == "catalog" {
            assert_eq!(first.playlists().unwrap().len(), 2);
        } else {
            assert_eq!(first.tracks(id, "", Sort::Position).unwrap().len(), 1);
        }
    }
}

#[test]
fn legacy_library_is_versioned_without_losing_entries_or_session() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let id;
    {
        let mut db = Library::open(&path).unwrap();
        id = db.create("Keep me").unwrap();
        db.add(id, &[track("Preserved")]).unwrap();
        db.save("session", "saved queue").unwrap();
        db.conn
            .execute_batch("ALTER TABLE entries DROP COLUMN unavailable; ALTER TABLE playlists DROP COLUMN description; PRAGMA user_version=0;")
            .unwrap();
    }
    let db = Library::open(&path).unwrap();
    assert_eq!(
        db.conn
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(db.tracks(id, "preserved", Sort::Position).unwrap().len(), 1);
    assert_eq!(db.load("session").as_deref(), Some("saved queue"));
}

#[test]
fn simultaneous_first_connections_initialize_safely() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fresh.db");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let workers: Vec<_> = (0..4)
        .map(|i| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                Library::open(path)
                    .unwrap()
                    .create(&format!("Playlist {i}"))
                    .unwrap();
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(Library::open(path).unwrap().playlists().unwrap().len(), 4);
}

#[test]
fn search_preserves_playlist_indexes_after_reorder_and_deletion() {
    let mut db = Library::open(":memory:").unwrap();
    let id = db.create("Indexes").unwrap();
    db.add(
        id,
        &[
            track("Other"),
            track("Match Zulu"),
            track("Gap"),
            track("Match Alpha"),
        ],
    )
    .unwrap();
    let all = db.tracks(id, "", Sort::Position).unwrap();
    let indexes = |rows: Vec<Track>| rows.iter().map(|t| t.playlist_index).collect::<Vec<_>>();
    assert_eq!(
        indexes(db.tracks(id, "match", Sort::Position).unwrap()),
        vec![2, 4]
    );
    assert_eq!(
        indexes(db.tracks(id, "match", Sort::Title).unwrap()),
        vec![4, 2]
    );
    db.remove(all[2].id).unwrap();
    assert_eq!(
        indexes(db.tracks(id, "match", Sort::Position).unwrap()),
        vec![2, 3]
    );
    db.move_entry(all[3].id, -1).unwrap();
    let found = db.tracks(id, "alpha", Sort::Position).unwrap();
    assert_eq!(found[0].id, all[3].id);
    assert_eq!(found[0].playlist_index, 2);
}

#[test]
fn full_artist_credit_survives_storage_search_and_queue_serialization() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("artists.db");
    let mut db = Library::open(&path).unwrap();
    let playlist = db.create("Music").unwrap();
    let mut song = track("Cantata");
    song.artist = mousa::artists::resolve(&serde_json::json!({
        "artists": ["Orchestra", "Karl Richter", "Choir", "Johann Sebastian Bach"],
        "album": "Album must not be credited"
    }));
    db.add(playlist, &[song.clone()]).unwrap();
    drop(db);
    let db = Library::open(&path).unwrap();
    for query in ["Orchestra", "Richter", "Choir", "Bach"] {
        let found = db.tracks(playlist, query, Sort::Artist).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].artist, song.artist);
        let saved = serde_json::to_string(&found[0]).unwrap();
        let restored: Track = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.artist, song.artist);
    }
    assert!(
        db.tracks(playlist, "Album", Sort::Artist)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn arbitrary_reorder_persists_duplicate_identity_and_handles_gaps() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    let mut db = Library::open(path.to_str().unwrap()).unwrap();
    let playlist = db.create("Reorder").unwrap();
    db.add(
        playlist,
        &[
            track("Same"),
            track("Deleted"),
            track("Same"),
            track("Last"),
        ],
    )
    .unwrap();
    let original = db.tracks(playlist, "", Sort::Position).unwrap();
    let ids: Vec<_> = original.iter().map(|t| t.id).collect();
    db.remove(ids[1]).unwrap();
    db.move_entry_before(ids[3], Some(ids[0])).unwrap();
    assert_eq!(
        db.tracks(playlist, "", Sort::Position)
            .unwrap()
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>(),
        vec![ids[3], ids[0], ids[2]]
    );
    db.move_entry_before(ids[0], None).unwrap();
    db.move_entry_before(ids[2], Some(ids[2])).unwrap();
    drop(db);
    let mut db = Library::open(path.to_str().unwrap()).unwrap();
    let reordered = db.tracks(playlist, "", Sort::Position).unwrap();
    assert_eq!(
        reordered.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![ids[3], ids[2], ids[0]]
    );
    assert_eq!(
        reordered
            .iter()
            .map(|t| t.playlist_index)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    let other = db.create("Other").unwrap();
    db.add(other, &[track("Elsewhere")]).unwrap();
    let foreign = db.tracks(other, "", Sort::Position).unwrap()[0].id;
    assert!(db.move_entry_before(ids[0], Some(foreign)).is_err());
    assert!(db.move_entry_before(ids[0], Some(ids[1])).is_err());
    assert_eq!(
        db.tracks(playlist, "", Sort::Position)
            .unwrap()
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>(),
        vec![ids[3], ids[2], ids[0]]
    );
}

#[test]
fn unavailable_tracks_preserve_metadata_and_recover() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("availability.db");
    let mut db = Library::open(&path).unwrap();
    let playlist = db.create("Availability").unwrap();
    let original = track("Original song");
    db.reconcile_entries(playlist, &[("remote".into(), original.clone())])
        .unwrap();
    let mut private = track("Private video");
    private.artist.clear();
    private.duration = 0.;
    private.unavailable = Some("PRIVATE".into());
    db.reconcile_entries(playlist, &[("remote".into(), private)])
        .unwrap();
    drop(db);
    let mut db = Library::open(&path).unwrap();
    let saved = db.tracks(playlist, "", Sort::Position).unwrap().remove(0);
    assert_eq!(saved.title, original.title);
    assert_eq!(saved.artist, original.artist);
    assert_eq!(saved.duration, original.duration);
    assert_eq!(saved.unavailable.as_deref(), Some("PRIVATE"));
    db.reconcile_entries(playlist, &[("remote".into(), original)])
        .unwrap();
    assert!(
        db.tracks(playlist, "", Sort::Position).unwrap()[0]
            .unavailable
            .is_none()
    );
}

#[test]
fn old_library_migrates_explicit_unavailable_titles_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    let mut db = Library::open(&path).unwrap();
    let playlist = db.create("Old").unwrap();
    let mut unknown = track("Duration unknown");
    unknown.duration = 0.;
    db.add(
        playlist,
        &[track("Private video"), track("Deleted video"), unknown],
    )
    .unwrap();
    db.conn
        .execute_batch("ALTER TABLE entries DROP COLUMN unavailable; ALTER TABLE playlists DROP COLUMN description; PRAGMA user_version=1;")
        .unwrap();
    drop(db);
    let db = Library::open(path).unwrap();
    let tracks = db.tracks(playlist, "", Sort::Position).unwrap();
    assert_eq!(tracks[0].unavailable.as_deref(), Some("PRIVATE"));
    assert_eq!(tracks[1].unavailable.as_deref(), Some("REMOVED"));
    assert_eq!(tracks[2].unavailable, None);
}

#[test]
fn multiselect_reorder_is_atomic_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("multiselect.db");
    let mut db = Library::open(&path).unwrap();
    let playlist = db.create("Selection").unwrap();
    db.add(
        playlist,
        &[
            track("Duplicate"),
            track("Gap"),
            track("Duplicate"),
            track("Fourth"),
            track("Fifth"),
        ],
    )
    .unwrap();
    let ids: Vec<_> = db
        .tracks(playlist, "", Sort::Position)
        .unwrap()
        .iter()
        .map(|t| t.id)
        .collect();
    db.remove(ids[1]).unwrap();
    db.move_entries_before(&[ids[4], ids[0]], Some(ids[3]))
        .unwrap();
    let expected = vec![ids[2], ids[0], ids[4], ids[3]];
    let read = |db: &Library| {
        db.tracks(playlist, "", Sort::Position)
            .unwrap()
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(read(&db), expected);
    assert!(db.move_entries_before(&[ids[0], ids[1]], None).is_err());
    assert_eq!(read(&db), expected);
    let other = db.create("Other").unwrap();
    db.add(other, &[track("Other")]).unwrap();
    let foreign = db.tracks(other, "", Sort::Position).unwrap()[0].id;
    assert!(db.move_entries_before(&[ids[0], foreign], None).is_err());
    assert_eq!(read(&db), expected);
    drop(db);
    let mut db = Library::open(&path).unwrap();
    assert_eq!(read(&db), expected);
    db.move_entries_before(&[ids[0], ids[4]], None).unwrap();
    assert_eq!(read(&db), [ids[2], ids[3], ids[0], ids[4]]);
    db.move_entries_before(&[ids[0], ids[4]], Some(ids[2]))
        .unwrap();
    assert_eq!(read(&db), [ids[0], ids[4], ids[2], ids[3]]);
}
