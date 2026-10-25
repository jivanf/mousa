//! Playlist metadata and validation shared by the editor and persistence layer.
use anyhow::{Result, ensure};
use serde_json::{Value, json};

pub const TITLE_LIMIT: usize = 150;
pub const DESCRIPTION_LIMIT: usize = 5000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fields {
    pub title: String,
    pub description: String,
    /// None for local playlists, which have no sharing setting.
    pub privacy: Option<String>,
}
impl Fields {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.title.trim().is_empty(),
            "Give the playlist a title to save it."
        );
        ensure!(
            self.title.chars().count() <= TITLE_LIMIT,
            "Title must be 150 characters or fewer."
        );
        ensure!(
            self.description.chars().count() <= DESCRIPTION_LIMIT,
            "Description must be 5000 characters or fewer."
        );
        ensure!(
            self.privacy
                .as_deref()
                .is_none_or(|p| matches!(p, "public" | "unlisted" | "private")),
            "Choose a valid privacy setting."
        );
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Details {
    pub fields: Fields,
    pub remote: Option<Remote>,
}
#[derive(Clone, Debug)]
pub struct Remote {
    pub id: String,
    pub account: String,
    pub etag: String,
    pub snippet: Value,
}
/// Only send writable properties. YouTube deletes omitted snippet properties.
pub fn update_body(remote: &Remote, fields: &Fields) -> Result<Value> {
    fields.validate()?;
    let privacy = fields
        .privacy
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Missing YouTube privacy setting"))?;
    let mut snippet = json!({"title":fields.title.trim(), "description":fields.description});
    for key in ["tags", "defaultLanguage"] {
        if let Some(value) = remote.snippet.get(key) {
            snippet[key] = value.clone();
        }
    }
    Ok(json!({"id":remote.id,"snippet":snippet,"status":{"privacyStatus":privacy}}))
}
