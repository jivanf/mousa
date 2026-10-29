pub mod artists;
pub mod catalog;
pub mod catalog_view;
mod channel_catalog;
pub mod db;
pub mod integrations;
#[cfg(target_os = "linux")]
pub mod mpris;
mod playback_metrics;
mod playback_start;
pub mod player;
pub mod playlist_edit;
pub mod playlist_editor;
mod queue_metadata;
mod stream_prepare;
pub mod sync;
pub mod sync_policy;
pub mod theme;
pub mod titles;
pub mod ui;
pub mod view_state;
#[cfg(target_os = "windows")]
pub mod windows_media;

pub mod track_selection;
