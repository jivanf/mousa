//! Remote-first edits: never report an edit saved before YouTube accepts it.
use crate::{
    db::{self, Library, Sort, Track},
    integrations, sync_policy as policy,
};
use anyhow::{Context, Result, bail, ensure};
use reqwest::{Method, blocking::Client};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
    time::Duration,
};

static OPERATIONS: Mutex<()> = Mutex::new(());
type SubscriptionCache = HashMap<(String, String), (std::time::Instant, Option<String>)>;
static SUBSCRIPTIONS: std::sync::OnceLock<Mutex<SubscriptionCache>> = std::sync::OnceLock::new();

pub fn clear_subscription_cache() {
    if let Some(cache) = SUBSCRIPTIONS.get() {
        cache.lock().unwrap().clear();
    }
}

/// Only called when an artist page is opened, never for search result rows.
pub fn artist_subscription(channel: &str) -> Result<Option<String>> {
    let _guard = OPERATIONS.lock().unwrap();
    let token = integrations::google_access()?;
    // The refresh-token fingerprint scopes cached state without storing credentials.
    use sha2::{Digest, Sha256};
    let account = format!(
        "{:x}",
        Sha256::digest(integrations::get_secret("google-refresh")?.as_bytes())
    );
    let key = (account, channel.to_owned());
    let cache = SUBSCRIPTIONS.get_or_init(Default::default);
    if let Some((at, value)) = cache.lock().unwrap().get(&key)
        && at.elapsed() < Duration::from_secs(300)
    {
        return Ok(value.clone());
    }
    let g = Google {
        client: Client::builder().timeout(Duration::from_secs(30)).build()?,
        token,
        account: String::new(),
        background: false,
    };
    let v = g.request(
        Method::GET,
        "subscriptions",
        &[("part", "id"), ("mine", "true"), ("forChannelId", channel)],
        None,
    )?;
    let id = v["items"][0]["id"].as_str().map(str::to_owned);
    cache
        .lock()
        .unwrap()
        .insert(key, (std::time::Instant::now(), id.clone()));
    Ok(id)
}

pub fn set_artist_subscription(channel: &str, existing: Option<&str>) -> Result<Option<String>> {
    let _guard = OPERATIONS.lock().unwrap();
    let g = Google::connect()?;
    let id = if let Some(id) = existing {
        g.request(Method::DELETE, "subscriptions", &[("id", id)], None)?;
        None
    } else {
        let v = g.request(
            Method::POST,
            "subscriptions",
            &[("part", "snippet")],
            Some(json!({"snippet":{"resourceId":{"kind":"youtube#channel","channelId":channel}}})),
        )?;
        Some(
            v["id"]
                .as_str()
                .context("Subscription saved without an ID; reopen this artist to refresh")?
                .to_owned(),
        )
    };
    clear_subscription_cache();
    Ok(id)
}
const API: &str = "https://www.googleapis.com/youtube/v3";
fn decode_response(response: reqwest::blocking::Response, quota_db: &Library) -> Result<Value> {
    let status = response.status();
    let text = response.text()?;
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    decode_value(status, value, quota_db)
}
fn decode_value(status: reqwest::StatusCode, value: Value, quota_db: &Library) -> Result<Value> {
    if !status.is_success() {
        let reason = value["error"]["errors"][0]["reason"].as_str().unwrap_or("");
        if reason == "quotaExceeded" || reason == "dailyLimitExceeded" {
            policy::pause(quota_db, integrations::now())?;
            bail!(
                "YouTube’s daily quota is exhausted. Sync and YouTube edits will resume after midnight Pacific time. Cached playlists remain available."
            );
        }
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

struct Google {
    client: Client,
    token: String,
    account: String,
    background: bool,
}
impl Google {
    fn connect() -> Result<Self> {
        Self::connect_with_budget(false)
    }
    fn connect_with_budget(background: bool) -> Result<Self> {
        let mut g = Self {
            client: Client::builder().timeout(Duration::from_secs(30)).build()?,
            token: integrations::google_access()?,
            account: String::new(),
            background,
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
        let quota_db = db::library()?;
        policy::init(&quota_db)?;
        self.request_with_db(&quota_db, method, resource, query, body)
    }

    /// Bulk operations reuse their initialized database as well as the HTTP client.
    fn request_with_db(
        &self,
        quota_db: &Library,
        method: Method,
        resource: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<Value> {
        policy::reserve(
            quota_db,
            integrations::now(),
            &format!("{} {resource}", method),
            if method == Method::GET { 1 } else { 50 },
            self.background,
        )?;
        let mut request = self
            .client
            .request(method, format!("{API}/{resource}"))
            .bearer_auth(&self.token)
            .query(query);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response=request.send().context("YouTube is unreachable. Cached playlists are still available; edits were not confirmed.")?;
        decode_response(response, quota_db)
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
    fn refresh(
        &self,
        db: &mut Library,
        id: i64,
        remote: &str,
        durations: &mut HashMap<String, f64>,
    ) -> Result<()> {
        if db.recently_written(id)? {
            return Ok(());
        }
        self.refresh_after_write(db, id, remote, durations)
    }

    fn refresh_after_write(
        &self,
        db: &mut Library,
        id: i64,
        remote: &str,
        durations: &mut HashMap<String, f64>,
    ) -> Result<()> {
        policy::init(db)?;
        let now = integrations::now();
        policy::attempted(db, id, now)?;
        let items = self.pages(
            "playlistItems",
            &[("part", "snippet"), ("playlistId", remote)],
        )?;
        let videos = items
            .iter()
            .filter_map(|item| item["snippet"]["resourceId"]["videoId"].as_str())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        persistent_durations(db, &videos, durations, now, |ids| {
            self.request(
                Method::GET,
                "videos",
                &[("part", "contentDetails"), ("id", ids)],
                None,
            )
        })?;
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
                    playlist_index: 0,
                    unavailable: crate::db::unavailable_status(s["title"].as_str(), None),
                    url: format!("https://www.youtube.com/watch?v={video}"),
                    title: crate::titles::from_snippet(s),
                    artist: crate::artists::resolve(s),
                    duration: durations.get(video).copied().unwrap_or(0.),
                },
            ));
        }
        use sha2::{Digest, Sha256};
        let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&tracks)?));
        db.reconcile_entries(id, &tracks)?;
        policy::completed(db, id, integrations::now(), &fingerprint)
    }
}
/// Catalog only: never download contents of unopened playlists.
pub fn refresh(progress: impl FnMut()) -> Result<()> {
    refresh_catalog(false, progress)
}
pub fn refresh_catalog(force: bool, mut progress: impl FnMut()) -> Result<()> {
    let Ok(_guard) = OPERATIONS.try_lock() else {
        return Ok(());
    };
    if integrations::get_secret("google-refresh").is_err() {
        return Ok(());
    }
    let mut db = db::library()?;
    policy::init(&db)?;
    let now = integrations::now();
    let checked = db
        .load("youtube-catalog-checked")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    if !force
        && (now - checked < policy::HOUR
            || policy::paused(&db, now)
            || !policy::background_available(&db, now)?)
    {
        return Ok(());
    }
    let g = Google::connect_with_budget(!force)?;
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
    db.reconcile_catalog(&g.account, &catalog)?;
    db.save("youtube-catalog-checked", &now.to_string())?;
    progress();
    Ok(())
}
/// User visits are recorded independently of whether another sync owns the lock.
pub fn refresh_playlist(id: i64, force: bool, opened: bool) -> Result<()> {
    let mut db = db::library()?;
    policy::init(&db)?;
    let Some((remote, account)) = db.google_playlist(id)? else {
        return Ok(());
    };
    let now = integrations::now();
    if opened {
        policy::opened(&db, id, now)?;
    }
    let _guard = OPERATIONS.lock().unwrap();
    let now = integrations::now();
    if (!force && policy::paused(&db, now)) || !policy::due(&db, id, now, force)? {
        return Ok(());
    }
    if !force && !opened && !policy::background_available(&db, now)? {
        return Ok(());
    }
    policy::attempted(&db, id, now)?;
    let g = Google::connect_with_budget(!force && !opened)?;
    g.check(&account)?;
    g.refresh(&mut db, id, &remote, &mut HashMap::new())
}
/// One previously used playlist per pass; never a full-library sweep.
pub fn refresh_background(active: Option<i64>) -> Result<()> {
    let db = db::library()?;
    policy::init(&db)?;
    if policy::paused(&db, integrations::now()) {
        return Ok(());
    }
    if let Some(id) = policy::candidate(&db, integrations::now(), active)? {
        refresh_playlist(id, false, false)?;
    }
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
/// Fetch authoritative metadata before enabling the editor. Never substitute an
/// empty description/private visibility for a failed remote read.
pub fn playlist_details(id: i64) -> Result<crate::playlist_edit::Details> {
    use crate::playlist_edit::{Details, Fields, Remote};
    let _guard = OPERATIONS.lock().unwrap();
    let db = db::library()?;
    let fields = db.playlist_fields(id)?;
    let Some((remote, account)) = db.google_playlist(id)? else {
        return Ok(Details {
            fields,
            remote: None,
        });
    };
    let g = Google::connect()?;
    g.check(&account)?;
    let mut playlist = None;
    for delay in [0, 1, 2, 4] {
        if delay > 0 {
            std::thread::sleep(Duration::from_secs(delay));
        }
        let v = g.request(
            Method::GET,
            "playlists",
            &[("part", "snippet,status"), ("id", &remote)],
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
    let fields = Fields {
        title: p["snippet"]["title"]
            .as_str()
            .context("Missing playlist title")?
            .into(),
        description: p["snippet"]["description"]
            .as_str()
            .unwrap_or_default()
            .into(),
        privacy: Some(
            p["status"]["privacyStatus"]
                .as_str()
                .context("Missing playlist privacy")?
                .into(),
        ),
    };
    Ok(Details {
        fields,
        remote: Some(Remote {
            id: remote,
            account,
            etag: p["etag"]
                .as_str()
                .context("Missing playlist version")?
                .into(),
            snippet: p["snippet"].clone(),
        }),
    })
}

pub fn edit_playlist(
    id: i64,
    original: &crate::playlist_edit::Details,
    fields: &crate::playlist_edit::Fields,
) -> Result<()> {
    fields.validate()?;
    let _guard = OPERATIONS.lock().unwrap();
    let db = db::library()?;
    let mapping = db.google_playlist(id)?;
    if let Some(remote) = &original.remote {
        ensure!(
            mapping.as_ref() == Some(&(remote.id.clone(), remote.account.clone())),
            "Playlist account changed. Close and reopen the editor."
        );
        let g = Google::connect()?;
        g.check(&remote.account)?;
        let body = crate::playlist_edit::update_body(remote, fields)?;
        policy::init(&db)?;
        policy::reserve(&db, integrations::now(), "PUT playlists", 50, false)?;
        let response = g
            .client
            .put(format!("{API}/playlists"))
            .bearer_auth(&g.token)
            .query(&[("part", "snippet,status")])
            .header("If-Match", &remote.etag)
            .json(&body)
            .send()?;
        if response.status() == reqwest::StatusCode::PRECONDITION_FAILED {
            bail!(
                "This playlist changed on YouTube. Your edits are still here; copy them before reopening the editor to load the latest version."
            );
        }
        decode_response(response, &db)?;
    } else {
        ensure!(
            mapping.is_none() && fields.privacy.is_none(),
            "Playlist source changed. Close and reopen the editor."
        );
    }
    let tx = db.conn.unchecked_transaction()?;
    db.edit_playlist(id, fields)?;
    if original.remote.is_some() {
        db.mark_written(id)?;
    }
    tx.commit()?;
    Ok(())
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
        policy::init(&db)?;
        policy::reserve(&db, integrations::now(), "PUT playlists", 50, false)?;
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
        decode_response(response, &db)?;
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
    if tracks.is_empty() {
        return Ok(());
    }
    let mut db = db::library()?;
    let Some((remote, account)) = db.google_playlist(id)? else {
        return db.add(id, tracks);
    };
    let _guard = OPERATIONS.lock().unwrap();
    let g = Google::connect()?;
    g.check(&account)?;
    policy::init(&db)?;
    let mut needs_refresh = !cache_fresh_for_append(&db, id, integrations::now())?;
    let mut expected_position: i64 = db.conn.query_row(
        "SELECT count(*) FROM entries WHERE playlist_id=?",
        [id],
        |row| row.get(0),
    )?;
    let addition = (|| -> Result<()> {
        for (added, t) in tracks.iter().enumerate() {
            let url = url::Url::parse(&t.url)?;
            let video = url
                .query_pairs()
                .find(|(k, _)| k == "v")
                .map(|(_, v)| v.into_owned())
                .context("Track has no YouTube video ID")?;
            let result = g.request_with_db(&db,Method::POST,"playlistItems",&[("part","snippet")],Some(json!({"snippet":{"playlistId":remote,"resourceId":{"kind":"youtube#video","videoId":video}}})));
            let value = match result {
                Ok(v) => v,
                Err(e) => bail!(
                    "Added {added} of {} tracks before an error: {e}. Refresh before retrying to avoid duplicates.",
                    tracks.len()
                ),
            };
            // Detect incomplete caches and external changes using the confirmed position.
            needs_refresh |= value["snippet"]["position"].as_i64() != Some(expected_position);
            expected_position += 1;
            let remote_item = value["id"].as_str().context(
                "YouTube accepted an addition without an item ID. Refresh before retrying.",
            )?;
            db.append_google(id, remote_item, t).context(
                "Track added on YouTube but cache update failed. Refresh before retrying.",
            )?;
        }
        Ok(())
    })();
    finish_addition(addition, needs_refresh, || {
        // The normal refresh skips recently written playlists, but additions
        // must also fetch entries that were never present in the local cache.
        g.refresh_after_write(&mut db, id, &remote, &mut HashMap::new())
    })
}

fn cache_fresh_for_append(db: &Library, id: i64, now: i64) -> Result<bool> {
    Ok(db.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM youtube_refresh WHERE playlist_id=? AND checked>0 AND checked>=? AND attempted<=checked)",
        rusqlite::params![id, now - 60], |row| row.get(0))?)
}

fn finish_addition(
    addition: Result<()>,
    needs_refresh: bool,
    refresh: impl FnOnce() -> Result<()>,
) -> Result<()> {
    if addition.is_ok() && !needs_refresh {
        return Ok(());
    }
    let refreshed = refresh();
    match (addition, refreshed) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error.context(
            "Tracks added, but playlist refresh failed. Refresh before retrying to avoid duplicates",
        )),
        (Err(addition), Err(refresh)) => {
            bail!("{addition}. Playlist refresh also failed: {refresh}")
        }
    }
}

pub fn remove(id: i64) -> Result<()> {
    let (_, error) = remove_many(&[id]);
    match error {
        Some(error) => Err(anyhow::anyhow!(error)),
        None => Ok(()),
    }
}

/// Reuse one connection; stop on failure and report durably confirmed removals.
pub fn remove_many(ids: &[i64]) -> (usize, Option<String>) {
    let mut removed = 0;
    let result = (|| -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let _guard = OPERATIONS.lock().unwrap();
        let mut db = db::library()?;
        let mut seen = HashSet::new();
        let ids: Vec<_> = ids.iter().copied().filter(|id| seen.insert(*id)).collect();
        let entries = ids
            .iter()
            .map(|id| {
                let playlist = db.entry_playlist(*id)?;
                let remote = db
                    .google_playlist(playlist)?
                    .map(|(_, account)| Ok::<_, anyhow::Error>((account, db.google_entry(*id)?)))
                    .transpose()?;
                Ok((*id, playlist, remote))
            })
            .collect::<Result<Vec<_>>>()?;
        if entries.iter().all(|(_, _, remote)| remote.is_none()) {
            db.remove_many(&ids)?;
            removed = ids.len();
            return Ok(());
        }
        let g = Google::connect()?;
        policy::init(&db)?;
        for (_, _, remote) in &entries {
            if let Some((account, _)) = remote {
                g.check(account)?;
            }
        }
        let (count, error) = confirmed_prefix(&entries, |(id, playlist, remote)| {
            if let Some((_, remote_id)) = remote {
                g.request_with_db(
                    &db,
                    Method::DELETE,
                    "playlistItems",
                    &[("id", remote_id)],
                    None,
                )
                .context("Removal was not confirmed. Refresh before retrying")?;
            }
            let tx = db.conn.unchecked_transaction()?;
            db.remove(*id)
                .context("Removal cache update failed. Refresh before retrying")?;
            if remote.is_some() {
                db.mark_written(*playlist)?;
            }
            tx.commit()
                .context("Removal cache commit failed. Refresh before retrying")?;
            Ok(())
        });
        removed = count;
        if let Some(error) = error {
            return Err(error);
        }
        Ok(())
    })();
    (removed, result.err().map(|error| format!("{error:#}")))
}

fn confirmed_prefix<T>(
    items: &[T],
    mut apply: impl FnMut(&T) -> Result<()>,
) -> (usize, Option<anyhow::Error>) {
    for (count, item) in items.iter().enumerate() {
        if let Err(error) = apply(item) {
            return (count, Some(error));
        }
    }
    (items.len(), None)
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
    g.refresh(&mut db, playlist, &remote, &mut HashMap::new())?;
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

/// Use entry identity as the drop anchor so remote refreshes cannot shift the target.
pub fn move_entry_before(id: i64, before: Option<i64>) -> Result<()> {
    move_entries_before(&[id], before)
}

/// YouTube has no batch reorder API. Persist each confirmed move so a partial
/// failure leaves the cache consistent with the successful remote writes.
pub fn move_entries_before(selected: &[i64], before: Option<i64>) -> Result<()> {
    let _guard = OPERATIONS.lock().unwrap();
    let mut db = db::library()?;
    let id = *selected.first().context("No tracks selected")?;
    let playlist = db.entry_playlist(id)?;
    let Some((remote, account)) = db.google_playlist(playlist)? else {
        return db.move_entries_before(selected, before);
    };
    let g = Google::connect()?;
    g.check(&account)?;
    g.refresh(&mut db, playlist, &remote, &mut HashMap::new())?;
    let tracks = db.tracks(playlist, "", Sort::Position)?;
    let mut order: Vec<_> = tracks.iter().map(|t| t.id).collect();
    let desired = crate::track_selection::moved_order(&order, selected, before)?;
    if order == desired {
        return Ok(());
    }
    let moving: Vec<_> = order
        .iter()
        .filter(|id| selected.contains(id))
        .copied()
        .collect();
    let end = desired
        .iter()
        .rposition(|id| selected.contains(id))
        .unwrap();
    let mut anchor = desired.get(end + 1).copied();
    for id in moving.into_iter().rev() {
        let next = crate::track_selection::moved_order(&order, &[id], anchor)?;
        if next != order {
            let target = next.iter().position(|entry| *entry == id).unwrap();
            let source = tracks.iter().find(|t| t.id == id).unwrap();
            let video = url::Url::parse(&source.url)?
                .query_pairs()
                .find(|(k, _)| k == "v")
                .context("Missing video ID")?
                .1
                .into_owned();
            g.request(Method::PUT, "playlistItems", &[("part", "snippet")],
                Some(json!({"id":db.google_entry(id)?,"snippet":{"playlistId":remote,"position":target,
                "resourceId":{"kind":"youtube#video","videoId":video}}})))?;
            db.move_entry_before(id, anchor)?;
            db.mark_written(playlist)?;
        }
        order = next;
        anchor = Some(id);
    }
    Ok(())
}

/// Persist each successful metadata batch, even if a later batch fails.
fn persistent_durations(
    db: &Library,
    videos: &[String],
    cache: &mut HashMap<String, f64>,
    now: i64,
    mut fetch: impl FnMut(&str) -> Result<Value>,
) -> Result<()> {
    for row in db.conn.prepare("SELECT video,duration FROM youtube_metadata WHERE fetched>? AND (duration>0 OR fetched>?)")?.query_map(rusqlite::params![now-7*policy::DAY,now-6*policy::HOUR],|r|Ok((r.get::<_,String>(0)?,r.get::<_,f64>(1)?)))? {
        let (id,duration)=row?;cache.insert(id,duration);
    }
    fetch_durations(videos, cache, |ids| {
        let response = fetch(ids)?;
        let items = response["items"]
            .as_array()
            .context("YouTube returned no video details")?;
        let resolved: HashMap<_, _> = items
            .iter()
            .filter_map(|item| {
                Some((
                    item["id"].as_str()?,
                    youtube_duration(item["contentDetails"]["duration"].as_str()?)?,
                ))
            })
            .collect();
        let tx = db.conn.unchecked_transaction()?;
        for id in ids.split(',') {
            tx.execute("INSERT INTO youtube_metadata VALUES(?,?,?) ON CONFLICT(video) DO UPDATE SET duration=excluded.duration,fetched=excluded.fetched",rusqlite::params![id,resolved.get(id).copied().unwrap_or(0.),now])?;
        }
        tx.commit()?;
        Ok(response)
    })
}

/// Video durations are absent from playlistItems; resolve unique IDs in batches.
/// In-memory deduplication supplements persistent metadata caching. Missing IDs
/// are cached for six hours; known durations for seven days.
fn fetch_durations(
    videos: &[String],
    cache: &mut HashMap<String, f64>,
    mut fetch: impl FnMut(&str) -> Result<Value>,
) -> Result<()> {
    let mut seen = HashSet::new();
    let missing: Vec<_> = videos
        .iter()
        .filter(|id| !cache.contains_key(*id) && seen.insert(id.as_str()))
        .cloned()
        .collect();
    for batch in missing.chunks(50) {
        let response = fetch(&batch.join(","))?;
        let items = response["items"]
            .as_array()
            .context("YouTube returned no video details; cache left intact")?;
        let mut resolved = HashMap::new();
        for item in items {
            if let (Some(id), Some(duration)) = (
                item["id"].as_str(),
                item["contentDetails"]["duration"].as_str(),
            ) && let Some(seconds) = youtube_duration(duration)
            {
                resolved.insert(id, seconds);
            }
        }
        for id in batch {
            cache.insert(id.clone(), resolved.get(id.as_str()).copied().unwrap_or(0.));
        }
    }
    Ok(())
}

/// Parse YouTube's ISO 8601 day/hour/minute/second durations without treating
/// absent, malformed, or calendar-based durations as a real zero-length track.
fn youtube_duration(value: &str) -> Option<f64> {
    let mut total = 0.;
    let mut number = String::new();
    let mut in_time = false;
    let mut last_rank = 0;
    let mut units = 0;
    let mut time_units = 0;
    for c in value.strip_prefix('P')?.chars() {
        if c == 'T' {
            if in_time || !number.is_empty() {
                return None;
            }
            in_time = true;
        } else if c.is_ascii_digit() || c == '.' {
            number.push(c);
        } else {
            let (rank, factor) = match (in_time, c) {
                (false, 'D') => (1, 86400.),
                (true, 'H') => (2, 3600.),
                (true, 'M') => (3, 60.),
                (true, 'S') => (4, 1.),
                _ => return None,
            };
            if rank <= last_rank || number.is_empty() {
                return None;
            }
            let n: f64 = number.parse().ok()?;
            if !n.is_finite() || n < 0. {
                return None;
            }
            total += n * factor;
            last_rank = rank;
            units += 1;
            time_units += usize::from(in_time);
            number.clear();
        }
    }
    (units > 0 && (!in_time || time_units > 0) && number.is_empty() && total.is_finite())
        .then_some(total)
}

#[cfg(test)]
mod duration_tests {
    #[test]
    fn confirmed_removals_stop_at_each_failure_in_selection_order() {
        for failure in 0..=4 {
            let mut visited = Vec::new();
            let (count, error) = super::confirmed_prefix(&[3, 1, 4, 2], |id| {
                if visited.len() == failure {
                    anyhow::bail!("offline");
                }
                visited.push(*id);
                Ok(())
            });
            assert_eq!(count, failure);
            assert_eq!(visited, [3, 1, 4, 2][..failure]);
            assert_eq!(error.is_some(), failure < 4);
        }
    }

    #[test]
    fn fresh_confirmed_additions_skip_refresh_but_failures_do_not() {
        super::finish_addition(Ok(()), false, || panic!("redundant refresh")).unwrap();
        let mut refreshed = false;
        assert!(
            super::finish_addition(Err(anyhow::anyhow!("uncertain write")), false, || {
                refreshed = true;
                Ok(())
            })
            .is_err()
        );
        assert!(refreshed);
    }

    #[test]
    fn append_fast_path_requires_recent_successful_snapshot() {
        let db = crate::db::Library::open(":memory:").unwrap();
        super::policy::init(&db).unwrap();
        let id = db.create("test").unwrap();
        assert!(!super::cache_fresh_for_append(&db, id, 1000).unwrap());
        super::policy::completed(&db, id, 1000, "snapshot").unwrap();
        assert!(super::cache_fresh_for_append(&db, id, 1030).unwrap());
        assert!(!super::cache_fresh_for_append(&db, id, 1061).unwrap());
        super::policy::attempted(&db, id, 1001).unwrap();
        assert!(!super::cache_fresh_for_append(&db, id, 1030).unwrap());
    }

    #[test]
    fn additions_refresh_on_success_and_partial_failure() {
        for failed in [false, true] {
            let mut refreshed = false;
            let addition = if failed {
                Err(anyhow::anyhow!("Partial addition"))
            } else {
                Ok(())
            };
            let result = super::finish_addition(addition, true, || {
                refreshed = true;
                Ok(())
            });
            assert!(refreshed);
            assert_eq!(result.is_err(), failed);
        }
    }

    #[test]
    fn addition_refresh_failures_warn_against_duplicate_retries() {
        let error = super::finish_addition(Ok(()), true, || anyhow::bail!("Offline")).unwrap_err();
        assert!(error.to_string().contains("avoid duplicates"));
        let error = super::finish_addition(Err(anyhow::anyhow!("Partial addition")), false, || {
            anyhow::bail!("Offline")
        })
        .unwrap_err();
        assert!(error.to_string().contains("Partial addition"));
        assert!(error.to_string().contains("Offline"));
    }

    #[test]
    fn quota_error_is_readable_and_persistently_stops_requests() {
        let db = crate::db::Library::open(":memory:").unwrap();
        super::policy::init(&db).unwrap();
        let error=super::decode_value(reqwest::StatusCode::FORBIDDEN,serde_json::json!({"error":{"message":"exceeded <a href='quota'>quota</a>","errors":[{"reason":"quotaExceeded"}]}}),&db).unwrap_err().to_string();
        assert!(error.contains("midnight Pacific"));
        assert!(!error.contains("<a"));
        assert!(super::policy::paused(&db, crate::integrations::now()));
    }

    #[test]
    fn metadata_survives_restart_and_retries_only_expired_unknowns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let db = crate::db::Library::open(&path).unwrap();
        super::policy::init(&db).unwrap();
        let videos = vec!["known".to_string(), "unknown".to_string()];
        let now = 10 * super::policy::DAY;
        super::persistent_durations(&db,&videos,&mut std::collections::HashMap::new(),now, |_|Ok(serde_json::json!({"items":[{"id":"known","contentDetails":{"duration":"PT3M"}}]}))).unwrap();
        drop(db);
        let db = crate::db::Library::open(&path).unwrap();
        super::policy::init(&db).unwrap();
        super::persistent_durations(
            &db,
            &videos,
            &mut std::collections::HashMap::new(),
            now + 1,
            |_| panic!("cached across restart"),
        )
        .unwrap();
        super::persistent_durations(
            &db,
            &videos,
            &mut std::collections::HashMap::new(),
            now + 6 * super::policy::HOUR,
            |ids| {
                assert_eq!(ids, "unknown");
                Ok(serde_json::json!({"items":[]}))
            },
        )
        .unwrap();
        assert!(
            super::persistent_durations(
                &db,
                &["failed".into()],
                &mut std::collections::HashMap::new(),
                now,
                |_| anyhow::bail!("network failed")
            )
            .is_err()
        );
        let n: i64 = db
            .conn
            .query_row(
                "SELECT count(*) FROM youtube_metadata WHERE video='failed'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
    }

    use super::*;

    #[test]
    fn parses_video_lengths_and_rejects_unknown_formats() {
        for (value, seconds) in [
            ("PT19S", 19.),
            ("PT4M13S", 253.),
            ("PT2H3M4S", 7384.),
            ("P1DT2H", 93600.),
            ("PT0S", 0.),
            ("PT1.5S", 1.5),
        ] {
            assert_eq!(youtube_duration(value), Some(seconds), "{value}");
        }
        for value in [
            "",
            "P",
            "PT",
            "P1M",
            "PT-1S",
            "PT1H2H",
            "PT1S2M",
            "PT1",
            "P1DT",
            "not a duration",
        ] {
            assert_eq!(youtube_duration(value), None, "{value}");
        }
    }

    #[test]
    fn batches_deduplicates_and_matches_response_ids_not_positions() {
        let mut videos: Vec<_> = (0..5000).map(|i| format!("v{i}")).collect();
        videos.extend(videos.clone());
        let mut cache = HashMap::new();
        let mut calls = 0;
        fetch_durations(&videos, &mut cache, |ids| {
            calls += 1;
            let ids: Vec<_> = ids.split(',').collect();
            assert!(ids.len() <= 50);
            Ok(json!({"items": ids.into_iter().rev().filter(|id| *id != "v1").map(|id| json!({"id":id,"contentDetails":{"duration":"PT4M13S"}})).collect::<Vec<_>>()}))
        }).unwrap();
        assert_eq!(calls, 100);
        assert_eq!(cache["v1"], 0.); // unavailable video omitted by YouTube
        assert_eq!(cache["v4999"], 253.);
        fetch_durations(&videos, &mut cache, |_| panic!("duplicates must use cache")).unwrap();
    }

    #[test]
    fn failed_video_request_is_not_cached_as_unknown() {
        let mut cache = HashMap::new();
        assert!(
            fetch_durations(&["video".into()], &mut cache, |_| anyhow::bail!("offline")).is_err()
        );
        assert!(cache.is_empty());
    }
}
