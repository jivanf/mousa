//! Authenticated local control protocol used by the Mousa MCP adapter.
use crate::{
    catalog::{self, Destination},
    db::{self, Playlist, Sort, Track},
    integrations,
    player::{Command, Player, Snapshot},
    playlist_edit::Fields,
    sync,
};
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Stream, prelude::*};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    sync::Arc,
};

pub const PROTOCOL_VERSION: u32 = 1;
const MAX_FRAME: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageRequest {
    pub offset: usize,
    pub limit: usize,
}
impl PageRequest {
    fn bounds(&self, len: usize) -> Result<(usize, usize), ControlError> {
        if self.limit == 0 || self.limit > 500 {
            return Err(ControlError::invalid("limit must be between 1 and 500"));
        }
        let start = self.offset.min(len);
        Ok((start, (start + self.limit).min(len)))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlaylistSummary {
    pub id: i64,
    pub name: String,
    pub count: usize,
    pub google: bool,
    pub revision: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlaylistDetails {
    pub playlist: PlaylistSummary,
    pub description: String,
    pub tracks: Vec<Track>,
    pub offset: usize,
    pub total: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Paged<T> {
    pub items: Vec<T>,
    pub offset: usize,
    pub total: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountState {
    pub google: Option<String>,
    pub lastfm: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppStatus {
    pub version: String,
    pub protocol_version: u32,
    pub accounts: AccountState,
    pub player_revision: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Request {
    Status,
    Accounts,
    ListPlaylists {
        query: Option<String>,
        page: PageRequest,
    },
    GetPlaylist {
        id: i64,
        query: Option<String>,
        page: PageRequest,
    },
    SearchLibrary {
        query: String,
        playlist_id: Option<i64>,
        page: PageRequest,
    },
    SearchCatalog {
        query: String,
    },
    GetCatalogPage {
        destination: Destination,
    },
    GetPlayer,
    GetQueue {
        page: PageRequest,
    },
    Player {
        action: PlayerAction,
    },
    Queue {
        action: QueueAction,
    },
    CreatePlaylist {
        name: String,
        google: bool,
    },
    UpdatePlaylist {
        id: i64,
        title: String,
        description: String,
        privacy: Option<String>,
        expected_revision: Option<String>,
    },
    DeletePlaylist {
        id: i64,
        expected_revision: Option<String>,
    },
    AddPlaylistTracks {
        id: i64,
        tracks: Vec<Track>,
        expected_revision: Option<String>,
    },
    RemovePlaylistEntries {
        ids: Vec<i64>,
        expected_revision: Option<String>,
    },
    MovePlaylistEntries {
        ids: Vec<i64>,
        before: Option<i64>,
        expected_revision: Option<String>,
    },
    RefreshLibrary,
    RefreshPlaylist {
        id: i64,
        force: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PlayerAction {
    Play,
    Pause,
    Stop,
    Next,
    Previous,
    Seek {
        position: f64,
    },
    SetVolume {
        volume: f64,
    },
    SetShuffle {
        enabled: bool,
    },
    SetRepeat {
        enabled: bool,
    },
    PlayTrack {
        track: Track,
    },
    PlayPlaylist {
        id: i64,
        start_entry_id: Option<i64>,
        shuffle: bool,
    },
    PlayQueueIndex {
        index: usize,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum QueueAction {
    Replace {
        tracks: Vec<Track>,
        start_index: usize,
    },
    Append {
        tracks: Vec<Track>,
    },
    Remove {
        indices: Vec<usize>,
    },
    Move {
        indices: Vec<usize>,
        before: Option<usize>,
    },
    Clear,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Envelope {
    pub protocol_version: u32,
    pub request_id: String,
    pub token: String,
    pub request: Request,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResponseEnvelope {
    pub protocol_version: u32,
    pub request_id: String,
    pub result: Result<Value, ControlError>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeDescriptor {
    pub protocol_version: u32,
    pub pid: u32,
    pub endpoint: String,
    pub token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ControlError {
    pub code: String,
    pub message: String,
}
impl ControlError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid_request", message)
    }
}

#[derive(Clone)]
pub struct Service {
    player: Player,
    changed: Option<Arc<dyn Fn() + Send + Sync>>,
}
impl Service {
    pub fn new(player: Player) -> Self {
        Self {
            player,
            changed: None,
        }
    }
    pub fn with_change_notifier(mut self, notify: impl Fn() + Send + Sync + 'static) -> Self {
        self.changed = Some(Arc::new(notify));
        self
    }
    fn notify(&self) {
        if let Some(n) = &self.changed {
            n();
        }
    }
    fn send(&self, command: Command) -> Result<(), ControlError> {
        let revision = self.snapshot().revision;
        self.player.tx.send(command).map_err(|_| {
            ControlError::new("playback_unavailable", "Playback worker is unavailable")
        })?;
        for _ in 0..100 {
            if self.snapshot().revision != revision {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Err(ControlError::new(
            "playback_unavailable",
            "Playback did not acknowledge the command",
        ))
    }
    fn snapshot(&self) -> Snapshot {
        self.player
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn handle(&self, request: Request) -> Result<Value, ControlError> {
        let value = match request {
            Request::Status => json!(AppStatus {
                version: env!("CARGO_PKG_VERSION").into(),
                protocol_version: PROTOCOL_VERSION,
                accounts: accounts(),
                player_revision: self.snapshot().revision
            }),
            Request::Accounts => json!(accounts()),
            Request::ListPlaylists { query, page } => {
                let db = library()?;
                let mut items = db.playlists().map_err(classify)?;
                if let Some(q) = query {
                    let q = q.to_lowercase();
                    items.retain(|p| p.name.to_lowercase().contains(&q));
                }
                let (a, b) = page.bounds(items.len())?;
                let summaries = items[a..b]
                    .iter()
                    .map(|p| summary(&db, p))
                    .collect::<Result<Vec<_>, _>>()?;
                json!(Paged {
                    items: summaries,
                    offset: a,
                    total: items.len()
                })
            }
            Request::GetPlaylist { id, query, page } => {
                json!(playlist_details(id, query.as_deref().unwrap_or(""), page)?)
            }
            Request::SearchLibrary {
                query,
                playlist_id,
                page,
            } => {
                let db = library()?;
                let playlists = if let Some(id) = playlist_id {
                    vec![playlist_by_id(&db, id)?]
                } else {
                    db.playlists().map_err(classify)?
                };
                let mut tracks = Vec::new();
                for p in playlists {
                    tracks.extend(db.tracks(p.id, &query, Sort::Position).map_err(classify)?);
                }
                let (a, b) = page.bounds(tracks.len())?;
                json!(Paged {
                    items: tracks[a..b].to_vec(),
                    offset: a,
                    total: tracks.len()
                })
            }
            Request::SearchCatalog { query } => json!(
                catalog::fetch(&catalog::search(&query))
                    .map_err(|e| ControlError::new("catalog_unavailable", e.to_string()))?
            ),
            Request::GetCatalogPage { destination } => json!(
                catalog::fetch(&destination)
                    .map_err(|e| ControlError::new("catalog_unavailable", e.to_string()))?
            ),
            Request::GetPlayer => json!(self.snapshot()),
            Request::GetQueue { page } => {
                let s = self.snapshot();
                let (a, b) = page.bounds(s.queue.len())?;
                json!(Paged {
                    items: s.queue[a..b].to_vec(),
                    offset: a,
                    total: s.queue.len()
                })
            }
            Request::Player { action } => {
                self.player_action(action)?;
                json!(self.snapshot())
            }
            Request::Queue { action } => {
                self.queue_action(action)?;
                json!(self.snapshot())
            }
            Request::CreatePlaylist { name, google } => {
                let id = sync::create(&name, google).map_err(classify)?;
                self.notify();
                json!({"id":id})
            }
            Request::UpdatePlaylist {
                id,
                title,
                description,
                privacy,
                expected_revision,
            } => {
                check_revision(id, expected_revision.as_deref())?;
                let original = sync::playlist_details(id).map_err(classify)?;
                sync::edit_playlist(
                    id,
                    &original,
                    &Fields {
                        title,
                        description,
                        privacy,
                    },
                )
                .map_err(classify)?;
                self.notify();
                json!({"id":id,"revision":revision(id)?})
            }
            Request::DeletePlaylist {
                id,
                expected_revision,
            } => {
                check_revision(id, expected_revision.as_deref())?;
                sync::delete(id).map_err(classify)?;
                self.notify();
                json!({"deleted":id})
            }
            Request::AddPlaylistTracks {
                id,
                tracks,
                expected_revision,
            } => {
                check_revision(id, expected_revision.as_deref())?;
                sync::add(id, &tracks).map_err(classify)?;
                self.notify();
                json!({"id":id,"added":tracks.len(),"revision":revision(id)?})
            }
            Request::RemovePlaylistEntries {
                ids,
                expected_revision,
            } => {
                let db = library()?;
                let playlist = entries_playlist(&db, &ids)?;
                check_revision(playlist, expected_revision.as_deref())?;
                let (removed, error) = sync::remove_many(&ids);
                if let Some(e) = error {
                    return Err(classify(anyhow::anyhow!(e)));
                }
                self.notify();
                json!({"removed":removed,"revision":revision(playlist)?})
            }
            Request::MovePlaylistEntries {
                ids,
                before,
                expected_revision,
            } => {
                let db = library()?;
                let playlist = entries_playlist(&db, &ids)?;
                if let Some(before) = before
                    && db.entry_playlist(before).map_err(classify)? != playlist
                {
                    return Err(ControlError::invalid(
                        "before entry belongs to another playlist",
                    ));
                }
                check_revision(playlist, expected_revision.as_deref())?;
                sync::move_entries_before(&ids, before).map_err(classify)?;
                self.notify();
                json!({"moved":ids.len(),"revision":revision(playlist)?})
            }
            Request::RefreshLibrary => {
                sync::refresh(|| {}).map_err(classify)?;
                self.notify();
                json!({"completed":true})
            }
            Request::RefreshPlaylist { id, force } => {
                sync::refresh_playlist(id, force, true).map_err(classify)?;
                self.notify();
                json!({"id":id,"completed":true,"revision":revision(id)?})
            }
        };
        Ok(value)
    }
    fn player_action(&self, action: PlayerAction) -> Result<(), ControlError> {
        match action {
            PlayerAction::Play => self.send(Command::Pause(false)),
            PlayerAction::Pause => self.send(Command::Pause(true)),
            PlayerAction::Stop => self.send(Command::Stop),
            PlayerAction::Next => self.send(Command::Next),
            PlayerAction::Previous => self.send(Command::Previous),
            PlayerAction::Seek { position } => self.send(Command::Seek(position)),
            PlayerAction::SetVolume { volume } if (0.0..=100.0).contains(&volume) => {
                self.send(Command::Volume(volume))
            }
            PlayerAction::SetVolume { .. } => {
                Err(ControlError::invalid("volume must be between 0 and 100"))
            }
            PlayerAction::SetShuffle { enabled } => self.send(Command::SetShuffle(enabled)),
            PlayerAction::SetRepeat { enabled } => self.send(Command::SetRepeat(enabled)),
            PlayerAction::PlayTrack { track } => self.send(Command::Queue(vec![track], 0)),
            PlayerAction::PlayPlaylist {
                id,
                start_entry_id,
                shuffle,
            } => {
                let tracks = library()?
                    .tracks(id, "", Sort::Position)
                    .map_err(classify)?;
                if tracks.is_empty() {
                    return Err(ControlError::invalid("playlist is empty"));
                }
                let index = start_entry_id
                    .and_then(|id| tracks.iter().position(|t| t.id == id))
                    .unwrap_or(0);
                self.send(Command::SetShuffle(shuffle))?;
                self.send(Command::Queue(tracks, index))
            }
            PlayerAction::PlayQueueIndex { index } => {
                if index >= self.snapshot().queue.len() {
                    Err(ControlError::invalid("queue index is out of range"))
                } else {
                    self.send(Command::Jump(index))
                }
            }
        }
    }
    fn queue_action(&self, action: QueueAction) -> Result<(), ControlError> {
        match action {
            QueueAction::Replace {
                tracks,
                start_index,
            } => {
                if tracks.is_empty() {
                    let count = self.snapshot().queue.len();
                    for i in (0..count).rev() {
                        self.send(Command::Remove(i))?;
                    }
                    return Ok(());
                }
                if start_index >= tracks.len() {
                    return Err(ControlError::invalid("start_index is out of range"));
                }
                self.send(Command::Queue(tracks, start_index))
            }
            QueueAction::Append { tracks } => {
                for t in tracks {
                    self.send(Command::Append(t))?;
                }
                Ok(())
            }
            QueueAction::Remove { mut indices } => {
                indices.sort_unstable();
                indices.dedup();
                if indices.iter().any(|i| *i >= self.snapshot().queue.len()) {
                    return Err(ControlError::invalid("queue index is out of range"));
                }
                for i in indices.into_iter().rev() {
                    self.send(Command::Remove(i))?;
                }
                Ok(())
            }
            QueueAction::Move { indices, before } => {
                let s = self.snapshot();
                let q = (*s.queue).clone();
                let reordered = reorder_queue(&q, &indices, before)?;
                let current = s.current().map(|t| t.id);
                let start = current
                    .and_then(|id| reordered.iter().position(|t| t.id == id))
                    .unwrap_or(0);
                self.send(Command::Queue(reordered, start))
            }
            QueueAction::Clear => {
                let count = self.snapshot().queue.len();
                for i in (0..count).rev() {
                    self.send(Command::Remove(i))?;
                }
                Ok(())
            }
        }
    }
}

fn reorder_queue(
    queue: &[Track],
    indices: &[usize],
    before: Option<usize>,
) -> Result<Vec<Track>, ControlError> {
    let mut selected = indices.to_vec();
    selected.sort_unstable();
    selected.dedup();
    if selected.is_empty() {
        return Err(ControlError::invalid("indices must not be empty"));
    }
    if selected.iter().any(|i| *i >= queue.len()) || before.is_some_and(|i| i > queue.len()) {
        return Err(ControlError::invalid("queue index is out of range"));
    }
    let moved = selected
        .iter()
        .map(|i| queue[*i].clone())
        .collect::<Vec<_>>();
    let mut remaining = queue
        .iter()
        .enumerate()
        .filter(|(i, _)| !selected.contains(i))
        .map(|(_, t)| t.clone())
        .collect::<Vec<_>>();
    let target = before.unwrap_or(queue.len());
    let insertion = target
        .saturating_sub(selected.iter().filter(|i| **i < target).count())
        .min(remaining.len());
    remaining.splice(insertion..insertion, moved);
    Ok(remaining)
}

fn accounts() -> AccountState {
    AccountState {
        google: integrations::get_secret("google-name").ok(),
        lastfm: integrations::get_secret("lastfm-name").ok(),
    }
}
fn library() -> Result<db::Library, ControlError> {
    db::library().map_err(classify)
}
fn playlist_by_id(db: &db::Library, id: i64) -> Result<Playlist, ControlError> {
    db.playlists()
        .map_err(classify)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| {
            ControlError::new("playlist_not_found", format!("Playlist {id} was not found"))
        })
}
fn entries_playlist(db: &db::Library, ids: &[i64]) -> Result<i64, ControlError> {
    let first = *ids
        .first()
        .ok_or_else(|| ControlError::invalid("ids must not be empty"))?;
    let playlist = db.entry_playlist(first).map_err(classify)?;
    for id in &ids[1..] {
        if db.entry_playlist(*id).map_err(classify)? != playlist {
            return Err(ControlError::invalid(
                "all entries must belong to the same playlist",
            ));
        }
    }
    Ok(playlist)
}
fn summary(db: &db::Library, p: &Playlist) -> Result<PlaylistSummary, ControlError> {
    Ok(PlaylistSummary {
        id: p.id,
        name: p.name.clone(),
        count: p.count,
        google: p.google,
        revision: revision_with(db, p.id)?,
    })
}
fn playlist_details(
    id: i64,
    query: &str,
    page: PageRequest,
) -> Result<PlaylistDetails, ControlError> {
    let db = library()?;
    let p = playlist_by_id(&db, id)?;
    let fields = db.playlist_fields(id).map_err(classify)?;
    let tracks = db.tracks(id, query, Sort::Position).map_err(classify)?;
    let (a, b) = page.bounds(tracks.len())?;
    Ok(PlaylistDetails {
        playlist: summary(&db, &p)?,
        description: fields.description,
        tracks: tracks[a..b].to_vec(),
        offset: a,
        total: tracks.len(),
    })
}
fn revision(id: i64) -> Result<String, ControlError> {
    revision_with(&library()?, id)
}
fn revision_with(db: &db::Library, id: i64) -> Result<String, ControlError> {
    let fields = db.playlist_fields(id).map_err(|_| {
        ControlError::new("playlist_not_found", format!("Playlist {id} was not found"))
    })?;
    let tracks = db.tracks(id, "", Sort::Position).map_err(classify)?;
    let bytes = serde_json::to_vec(&(fields.title, fields.description, fields.privacy, tracks))
        .map_err(|e| ControlError::invalid(e.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn check_revision(id: i64, expected: Option<&str>) -> Result<(), ControlError> {
    if let Some(e) = expected
        && revision(id)? != e
    {
        return Err(ControlError::new(
            "revision_conflict",
            "Playlist changed since it was read",
        ));
    }
    Ok(())
}
fn classify(error: anyhow::Error) -> ControlError {
    let message = error.to_string();
    let lower = message.to_lowercase();
    let code = if lower.contains("account") || lower.contains("sign in") {
        "account_not_connected"
    } else if lower.contains("removed") || lower.contains("not found") {
        "entry_not_found"
    } else {
        "invalid_request"
    };
    ControlError::new(code, message)
}

pub fn runtime_path() -> PathBuf {
    db::data_dir().join("mcp-runtime.json")
}
pub struct RuntimeGuard {
    path: PathBuf,
    token: String,
}
impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        let belongs_to_this_process = fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<RuntimeDescriptor>(&bytes).ok())
            .is_some_and(|descriptor| descriptor.token == self.token);
        if belongs_to_this_process {
            let _ = fs::remove_file(&self.path);
        }
    }
}
pub fn start(service: Service) -> anyhow::Result<RuntimeGuard> {
    fs::create_dir_all(db::data_dir())?;
    let endpoint = format!(
        "mousa-{}-{:016x}",
        std::process::id(),
        rand::random::<u64>()
    );
    let token = format!(
        "{:016x}{:016x}",
        rand::random::<u64>(),
        rand::random::<u64>()
    );
    let name = endpoint.clone().to_ns_name::<GenericNamespaced>()?;
    let listener = ListenerOptions::new().name(name).create_sync()?;
    let descriptor = RuntimeDescriptor {
        protocol_version: PROTOCOL_VERSION,
        pid: std::process::id(),
        endpoint,
        token: token.clone(),
    };
    let path = runtime_path();
    fs::write(&path, serde_json::to_vec(&descriptor)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut conn) = conn else { continue };
            let service = service.clone();
            let token = token.clone();
            std::thread::spawn(move || {
                let response = match read_frame::<Envelope>(&mut conn) {
                    Ok(e) if e.protocol_version != PROTOCOL_VERSION => ResponseEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        request_id: e.request_id,
                        result: Err(ControlError::new(
                            "protocol_mismatch",
                            "MCP control protocol version mismatch",
                        )),
                    },
                    Ok(e) if e.token != token => ResponseEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        request_id: e.request_id,
                        result: Err(ControlError::new(
                            "authentication_failed",
                            "Invalid local control token",
                        )),
                    },
                    Ok(e) => ResponseEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        request_id: e.request_id,
                        result: service.handle(e.request),
                    },
                    Err(e) => ResponseEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        request_id: String::new(),
                        result: Err(ControlError::invalid(e.to_string())),
                    },
                };
                let _ = write_frame(&mut conn, &response);
            });
        }
    });
    Ok(RuntimeGuard {
        path,
        token: descriptor.token,
    })
}
pub fn call(request: Request) -> Result<Value, ControlError> {
    let bytes = fs::read(runtime_path())
        .map_err(|_| ControlError::new("app_not_running", "Mousa is not running"))?;
    let d: RuntimeDescriptor = serde_json::from_slice(&bytes)
        .map_err(|_| ControlError::new("app_not_running", "Mousa runtime descriptor is invalid"))?;
    if d.protocol_version != PROTOCOL_VERSION {
        return Err(ControlError::new(
            "protocol_mismatch",
            "Mousa uses an incompatible control protocol",
        ));
    }
    let name = d
        .endpoint
        .to_ns_name::<GenericNamespaced>()
        .map_err(|e| ControlError::new("app_not_running", e.to_string()))?;
    let mut stream = Stream::connect(name)
        .map_err(|_| ControlError::new("app_not_running", "Mousa is not running"))?;
    let request_id = format!("{:016x}", rand::random::<u64>());
    write_frame(
        &mut stream,
        &Envelope {
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.clone(),
            token: d.token,
            request,
        },
    )
    .map_err(|e| ControlError::new("app_not_running", e.to_string()))?;
    let response: ResponseEnvelope =
        read_frame(&mut stream).map_err(|e| ControlError::new("app_not_running", e.to_string()))?;
    if response.request_id != request_id {
        return Err(ControlError::invalid("response request ID mismatch"));
    }
    response.result
}
fn write_frame<T: Serialize>(w: &mut impl Write, value: &T) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(std::io::Error::other)?;
    if bytes.len() > MAX_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    w.write_all(&(bytes.len() as u32).to_be_bytes())?;
    w.write_all(&bytes)?;
    w.flush()
}
fn read_frame<T: for<'a> Deserialize<'a>>(r: &mut impl Read) -> std::io::Result<T> {
    let mut size = [0; 4];
    r.read_exact(&mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    if size > MAX_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut bytes = vec![0; size];
    r.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64) -> Track {
        Track {
            id,
            playlist_index: id as usize,
            url: format!("https://example.test/{id}"),
            title: format!("Track {id}"),
            artist: "Artist".into(),
            duration: 1.0,
            thumbnail: None,
            unavailable: None,
        }
    }

    #[test]
    fn pagination_is_bounded() {
        assert!(
            PageRequest {
                offset: 0,
                limit: 0
            }
            .bounds(2)
            .is_err()
        );
        assert!(
            PageRequest {
                offset: 0,
                limit: 501
            }
            .bounds(2)
            .is_err()
        );
        assert_eq!(
            PageRequest {
                offset: 8,
                limit: 10
            }
            .bounds(2)
            .unwrap(),
            (2, 2)
        );
    }

    #[test]
    fn queue_reordering_preserves_selected_order() {
        let queue = (0..5).map(track).collect::<Vec<_>>();
        let result = reorder_queue(&queue, &[3, 1], Some(4)).unwrap();
        assert_eq!(
            result.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![0, 2, 1, 3, 4]
        );
    }

    #[test]
    fn frames_round_trip() {
        let envelope = Envelope {
            protocol_version: PROTOCOL_VERSION,
            request_id: "r".into(),
            token: "secret".into(),
            request: Request::Status,
        };
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &envelope).unwrap();
        let decoded: Envelope = read_frame(&mut bytes.as_slice()).unwrap();
        assert_eq!(decoded.request_id, "r");
        assert_eq!(decoded.token, "secret");
    }

    #[test]
    fn oversized_frames_are_rejected() {
        let bytes = ((MAX_FRAME as u32) + 1).to_be_bytes().to_vec();
        assert!(read_frame::<Envelope>(&mut bytes.as_slice()).is_err());
    }
}
