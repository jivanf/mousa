use mousa::{
    db::Library,
    playlist_edit::{Fields, Remote, update_body},
};
use serde_json::json;
fn fields() -> Fields {
    Fields {
        title: "A playlist".into(),
        description: "First line\n\n# Notes\nhttps://example.com".into(),
        privacy: None,
    }
}
#[test]
fn metadata_survives_reopen_and_invalid_edits_do_not_write() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("library.db");
    let id;
    {
        let db = Library::open(&path).unwrap();
        id = db.create("Before").unwrap();
        db.edit_playlist(id, &fields()).unwrap();
    }
    let db = Library::open(&path).unwrap();
    assert_eq!(db.playlist_fields(id).unwrap(), fields());
    let mut invalid = fields();
    invalid.title = " \n ".into();
    assert!(db.edit_playlist(id, &invalid).is_err());
    assert_eq!(db.playlist_fields(id).unwrap(), fields());
    let mut cleared = fields();
    cleared.description.clear();
    db.edit_playlist(id, &cleared).unwrap();
    assert!(db.playlist_fields(id).unwrap().description.is_empty());
    db.delete(id).unwrap();
    assert!(db.edit_playlist(id, &fields()).is_err());
}
#[test]
fn version_two_migration_preserves_playlist_names() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("library.db");
    {
        let db = Library::open(&path).unwrap();
        db.create("Existing").unwrap();
        db.conn
            .execute_batch("ALTER TABLE playlists DROP COLUMN description; PRAGMA user_version=2;")
            .unwrap();
    }
    let db = Library::open(&path).unwrap();
    let p = &db.playlists().unwrap()[0];
    assert_eq!(p.name, "Existing");
    assert_eq!(db.playlist_fields(p.id).unwrap().description, "");
}
#[test]
fn unicode_limits_and_privacy_validation() {
    let mut f = fields();
    f.title = "é".repeat(150);
    f.description = "🎵".repeat(5000);
    assert!(f.validate().is_ok());
    f.title.push('a');
    assert!(f.validate().is_err());
    f.title = "OK".into();
    f.description.push('a');
    assert!(f.validate().is_err());
    f.description.clear();
    f.privacy = Some("friends".into());
    assert!(f.validate().is_err());
    for p in ["public", "unlisted", "private"] {
        f.privacy = Some(p.into());
        assert!(f.validate().is_ok());
    }
}
#[test]
fn youtube_update_preserves_optional_writable_fields_and_clears_description() {
    let remote = Remote {
        id: "PL123".into(),
        account: "account".into(),
        etag: "version".into(),
        snippet: json!({"title":"Old","description":"Old notes","tags":["music"],"defaultLanguage":"en","channelId":"read-only","thumbnails":{}}),
    };
    let mut f = fields();
    f.description.clear();
    f.privacy = Some("unlisted".into());
    assert_eq!(
        update_body(&remote, &f).unwrap(),
        json!({"id":"PL123","snippet":{"title":"A playlist","description":"","tags":["music"],"defaultLanguage":"en"},"status":{"privacyStatus":"unlisted"}})
    );
}
