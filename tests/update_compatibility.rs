use mousa::db::{Library, NewerLibrary};

#[test]
fn future_library_is_rejected_without_modifying_data() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    let db = Library::open(&path).unwrap();
    db.create("Preserve me").unwrap();
    db.conn.execute_batch("PRAGMA user_version=999").unwrap();
    drop(db);
    let error = Library::open(&path).err().unwrap();
    assert!(error.is::<NewerLibrary>());
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        999
    );
    assert_eq!(
        db.query_row("SELECT name FROM playlists", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "Preserve me"
    );
}

#[test]
fn supported_library_does_not_require_an_update() {
    assert!(Library::open(":memory:").is_ok());
}
