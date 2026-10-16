//! Durable browsing context; intentionally excludes forms, errors and account operations.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BrowsePosition {
    pub query: String,
    pub item: usize,
    pub offset: f32,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewState {
    pub playlist: Option<i64>,
    pub queue: bool,
    pub library_query: String,
    pub library_scroll: f32,
    pub playlists: BTreeMap<i64, BrowsePosition>,
    pub queue_position: BrowsePosition,
    pub window_size: Option<[f32; 2]>,
    pub window_origin: Option<[f32; 2]>,
    pub maximized: bool,
}
impl ViewState {
    pub fn load() -> Self {
        crate::db::library()
            .ok()
            .and_then(|db| db.load("view-state"))
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) -> anyhow::Result<()> {
        crate::db::library()?.save("view-state", &serde_json::to_string(self)?)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_keeps_separate_playlist_and_queue_contexts() {
        let mut state = ViewState {
            playlist: Some(42),
            queue: true,
            ..Default::default()
        };
        state.playlists.insert(
            42,
            BrowsePosition {
                query: "Bach".into(),
                item: 2400,
                offset: 12.5,
            },
        );
        state.queue_position.item = 17;
        let json = serde_json::to_string(&state).unwrap();
        let restored: ViewState = serde_json::from_str(&json).unwrap();
        assert!(restored.queue);
        assert_eq!(restored.playlist, Some(42));
        assert_eq!(restored.playlists, state.playlists);
        assert_eq!(restored.queue_position.item, 17);
        assert!(!json.contains("form"));
        assert!(!json.contains("error"));
    }
    #[test]
    fn database_restart_preserves_browsing_context() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("library.db");
        let db = crate::db::Library::open(&path).unwrap();
        let state = ViewState {
            playlist: Some(7),
            queue: true,
            library_query: "favorites".into(),
            ..Default::default()
        };
        db.save("view-state", &serde_json::to_string(&state).unwrap())
            .unwrap();
        drop(db);
        let db = crate::db::Library::open(&path).unwrap();
        let restored: ViewState = serde_json::from_str(&db.load("view-state").unwrap()).unwrap();
        assert_eq!(restored.playlist, Some(7));
        assert!(restored.queue);
        assert_eq!(restored.library_query, "favorites");
    }
    #[test]
    fn missing_fields_use_defaults() {
        let state: ViewState = serde_json::from_str("{}").unwrap();
        assert!(!state.queue);
        assert!(state.playlist.is_none());
    }
}
