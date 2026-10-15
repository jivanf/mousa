//! Remote-first edits: never report an edit saved before YouTube accepts it.
use crate::{
    db::{self, Library, Sort, Track},
    integrations,
};
use anyhow::{Context, Result, bail, ensure};
use reqwest::{Method, blocking::Client};
use serde_json::{Value, json};
use std::{sync::Mutex, time::Duration};

static OPERATIONS: Mutex<()> = Mutex::new(());
const API: &str = "https://www.googleapis.com/youtube/v3";
struct Google {
    client: Client,
    token: String,
    account: String,
}
impl Google {
    fn connect() -> Result<Self> {
        let mut g = Self {
            client: Client::builder().timeout(Duration::from_secs(30)).build()?,
            token: integrations::google_access()?,
            account: String::new(),
        };
        let v = g.request(
            Method::GET,
            "channels",
            &[("part", "id"), ("mine", "true")],
            None,
        )?;
        g.account = v["items"][0]["id"]
            .as_str()
            .context("This Google account has no YouTube channel")?
            .into();
        Ok(g)
    }
    fn request(
        &self,
        method: Method,
        resource: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<Value> {
        let mut request = self
            .client
            .request(method, format!("{API}/{resource}"))
            .bearer_auth(&self.token)
            .query(query);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response=request.send().context("YouTube is unreachable. Cached playlists are still available; edits were not confirmed.")?;
        let status = response.status();
        let text = response.text()?;
        let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if !status.is_success() {
            let reason = value["error"]["errors"][0]["reason"].as_str().unwrap_or("");
            if reason == "insufficientPermissions" {
                bail!("Reconnect Google to grant playlist editing permission");
            }
            bail!(
                "YouTube {status}: {}",
                value["error"]["message"]
                    .as_str()
                    .unwrap_or("Request failed")
            );
        }
        Ok(value)
    }
    fn pages(&self, resource: &str, query: &[(&str, &str)]) -> Result<Vec<Value>> {
        let mut result = Vec::new();
        let mut page = String::new();
        let mut seen = std::collections::HashSet::new();
        loop {
            let mut q = query.to_vec();
            q.push(("maxResults", "50"));
            q.push(("pageToken", &page));
            let v = self.request(Method::GET, resource, &q, None)?;
            result.extend(
                v["items"]
                    .as_array()
                    .context("YouTube returned no item list")?
                    .iter()
                    .cloned(),
            );
            page = v["nextPageToken"].as_str().unwrap_or("").into();
            if page.is_empty() {
                break;
            }
            ensure!(
                seen.insert(page.clone()),
                "YouTube repeated a page; cache left intact"
            );
        }
        Ok(result)
    }
    fn check(&self, account: &str) -> Result<()> {
        ensure!(
            self.account == account,
            "This playlist belongs to another Google account. Reconnect its owner to edit it."
        );
        Ok(())
    }
    fn refresh(&self, db: &mut Library, id: i64, remote: &str) -> Result<()> {
        if db.recently_written(id)? {
            return Ok(());
        }
        let items = self.pages(
            "playlistItems",
            &[("part", "snippet"), ("playlistId", remote)],
        )?;
        let mut tracks = Vec::new();
        for item in items {
            let s = &item["snippet"];
            let video = s["resourceId"]["videoId"]
                .as_str()
                .context("Missing YouTube video ID; cache left intact")?;
            tracks.push((
                item["id"]
                    .as_str()
                    .context("Missing playlist item ID")?
                    .into(),
                Track {
                    id: 0,
                    url: format!("https://www.youtube.com/watch?v={video}"),
                    title: s["title"].as_str().unwrap_or("Unavailable video").into(),
                    artist: s["videoOwnerChannelTitle"]
                        .as_str()
                        .unwrap_or("")
                        .trim_end_matches(" - Topic")
                        .into(),
                    duration: 0.,
                },
            ));
        }
        db.reconcile_entries(id, &tracks)
    }
}
/// Progress is emitted after the catalog, then after each complete playlist.
/// Existing cache remains immediately usable while this runs off the UI thread.
pub fn refresh(mut progress: impl FnMut()) -> Result<()> {
    let Ok(_guard) = OPERATIONS.try_lock() else {
        return Ok(());
    };
    if integrations::get_secret("google-refresh").is_err() {
        return Ok(());
    }
    let g = Google::connect()?;
    let lists = g.pages("playlists", &[("part", "snippet"), ("mine", "true")])?;
    let catalog: Vec<_> = lists
        .iter()
        .map(|p| {
            Ok((
                p["id"].as_str().context("Missing playlist ID")?.to_owned(),
                p["snippet"]["title"]
                    .as_str()
                    .context("Missing playlist title")?
                    .to_owned(),
            ))
        })
        .collect::<Result<_>>()?;
    let mut db = db::library()?;
    let ids = db.reconcile_catalog(&g.account, &catalog)?;
    progress();
    let mut failures = Vec::new();
    for (id, remote) in ids {
        if let Err(e) = g.refresh(&mut db, id, &remote) {
            failures.push(e.to_string());
        } else {
            progress();
        }
    }
    ensure!(
        failures.is_empty(),
        "Some playlists could not refresh: {}",
        failures.join("; ")
    );
    Ok(())
}
pub fn create(name: &str, google: bool) -> Result<i64> {
    ensure!(!name.trim().is_empty(), "Enter a playlist name");
    let _guard = OPERATIONS.lock().unwrap();
    let db = db::library()?;
    if !google {
        return db.create(name);
    }
    let g = Google::connect()?;
    let p = g.request(
        Method::POST,
        "playlists",
        &[("part", "snippet,status")],
        Some(json!({"snippet":{"title":name.trim()},"status":{"privacyStatus":"private"}})),
    )?;
    let remote = p["id"].as_str().context("Missing new playlist ID")?;
    let tx = db.conn.unchecked_transaction()?;
    let id = db.create(name)?;
    db.conn.execute(
        "INSERT INTO google_playlists VALUES(?,?,?)",
        rusqlite::params![id, remote, g.account],
    )?;
    db.mark_written(id)?;
    tx.commit()?;
    Ok(id)
}
pub fn rename(id: i64, name: &str) -> Result<()> {
    ensure!(!name.trim().is_empty(), "Enter a playlist name");
    let _guard = OPERATIONS.lock().unwrap();
    let db = db::library()?;
    if let Some((remote, account)) = db.google_playlist(id)? {
        let g = Google::connect()?;
        g.check(&account)?;
        // A newly created playlist can briefly be absent from list responses.
        // Retry only this read, never a mutation.
        let mut playlist = None;
        for delay in [0, 1, 2, 4] {
            if delay > 0 {
                std::thread::sleep(Duration::from_secs(delay));
            }
            let v = g.request(
                Method::GET,
                "playlists",
                &[("part", "snippet"), ("id", &remote)],
                None,
            )?;
            playlist = v["items"]
                .as_array()
                .and_then(|items| items.first())
                .cloned();
            if playlist.is_some() {
                break;
            }
        }
        let p = playlist.context("Playlist is not available on YouTube. Refresh and retry.")?;
        // Preserve optional writable snippet fields; do not send read-only fields.
        let mut snippet = json!({"title":name.trim()});
        for key in ["description", "tags", "defaultLanguage"] {
            if let Some(value) = p["snippet"].get(key) {
                snippet[key] = value.clone();
            }
        }
        let response = g
            .client
            .put(format!("{API}/playlists"))
            .bearer_auth(&g.token)
            .query(&[("part", "snippet")])
            .header(
                "If-Match",
                p["etag"].as_str().context("Missing playlist version")?,
            )
            .json(&json!({"id":remote,"snippet":snippet}))
            .send()?;
        ensure!(
            response.status().is_success(),
            "YouTube rename failed ({}). Refresh and retry; no local rename was applied.",
            response.status()
        );
    }
    db.rename(id, name)?;
    if db.google_playlist(id)?.is_some() {
        db.mark_written(id)?;
    }
    Ok(())
}
pub fn delete(id: i64) -> Result<()> {
    let _guard = OPERATIONS.lock().unwrap();
    let db = db::library()?;
    if let Some((remote, account)) = db.google_playlist(id)? {
        let g = Google::connect()?;
        g.check(&account)?;
        g.request(Method::DELETE, "playlists", &[("id", &remote)], None)?;
        db.conn.execute("INSERT INTO google_deleted VALUES(?,?,unixepoch()) ON CONFLICT(account,remote_id) DO UPDATE SET changed=excluded.changed",rusqlite::params![account,remote])?;
    }
    db.delete(id)
}
pub fn add(id: i64, tracks: &[Track]) -> Result<()> {
    let _guard = OPERATIONS.lock().unwrap();
    let mut db = db::library()?;
    let Some((remote, account)) = db.google_playlist(id)? else {
        return db.add(id, tracks);
    };
    let g = Google::connect()?;
    g.check(&account)?;
    for (added, t) in tracks.iter().enumerate() {
        let url = url::Url::parse(&t.url)?;
        let video = url
            .query_pairs()
            .find(|(k, _)| k == "v")
            .map(|(_, v)| v.into_owned())
            .context("Track has no YouTube video ID")?;
        let result = g.request(Method::POST,"playlistItems",&[("part","snippet")],Some(json!({"snippet":{"playlistId":remote,"resourceId":{"kind":"youtube#video","videoId":video}}})));
        let value = match result {
            Ok(v) => v,
            Err(e) => bail!(
                "Added {added} of {} tracks before an error: {e}. Refresh before retrying to avoid duplicates.",
                tracks.len()
            ),
        };
        let remote_item = value["id"]
            .as_str()
            .context("YouTube accepted an addition without an item ID. Refresh before retrying.")?;
        db.append_google(id, remote_item, t)
            .context("Track added on YouTube but cache update failed. Refresh before retrying.")?;
    }
    Ok(())
}

pub fn remove(id: i64) -> Result<()> {
    let _guard = OPERATIONS.lock().unwrap();
    let db = db::library()?;
    let playlist = db.entry_playlist(id)?;
    if let Some((_, account)) = db.google_playlist(playlist)? {
        let g = Google::connect()?;
        g.check(&account)?;
        g.request(
            Method::DELETE,
            "playlistItems",
            &[("id", &db.google_entry(id)?)],
            None,
        )?;
    }
    db.remove(id)?;
    if db.google_playlist(playlist)?.is_some() {
        db.mark_written(playlist)?;
    }
    Ok(())
}
pub fn move_entry(id: i64, delta: i32) -> Result<()> {
    ensure!(matches!(delta, -1 | 1), "Move by one position at a time");
    let _guard = OPERATIONS.lock().unwrap();
    let mut db = db::library()?;
    let playlist = db.entry_playlist(id)?;
    let Some((remote, account)) = db.google_playlist(playlist)? else {
        return db.move_entry(id, delta);
    };
    let g = Google::connect()?;
    g.check(&account)?;
    g.refresh(&mut db, playlist, &remote)?;
    let tracks = db.tracks(playlist, "", Sort::Position)?;
    let index = tracks
        .iter()
        .position(|t| t.id == id)
        .context("Track was removed on YouTube")?;
    let target = (index as i64 + delta as i64).clamp(0, tracks.len().saturating_sub(1) as i64);
    if target == index as i64 {
        return Ok(());
    }
    let video = url::Url::parse(&tracks[index].url)?
        .query_pairs()
        .find(|(k, _)| k == "v")
        .context("Missing video ID")?
        .1
        .into_owned();
    g.request(Method::PUT,"playlistItems",&[("part","snippet")],Some(json!({"id":db.google_entry(id)?,"snippet":{"playlistId":remote,"position":target,"resourceId":{"kind":"youtube#video","videoId":video}}})))?;
    db.move_entry(id, delta)?;
    db.mark_written(playlist)?;
    Ok(())
}
