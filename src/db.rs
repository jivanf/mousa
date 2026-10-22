use anyhow::Result;
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Track {
    pub id: i64,
    /// One-based position in the complete playlist, independent of search and sort.
    #[serde(default)]
    pub playlist_index: usize,
    pub url: String,
    pub title: String,
    /// Complete artist credit, resolved centrally by `artists::resolve`.
    pub artist: String,
    pub duration: f64,
}
#[derive(Clone, Debug)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub count: usize,
    pub google: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Sort {
    #[default]
    Position,
    Title,
    Artist,
    Duration,
}
impl Sort {
    pub fn next(self) -> Self {
        match self {
            Self::Position => Self::Title,
            Self::Title => Self::Artist,
            Self::Artist => Self::Duration,
            Self::Duration => Self::Position,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Position => "Playlist order",
            Self::Title => "Title",
            Self::Artist => "Artist",
            Self::Duration => "Duration",
        }
    }
}
pub struct Library {
    pub conn: Connection,
}
impl Library {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        anyhow::ensure!(version <= 1, "Library was created by a newer Mousa version");
        if version == 0 {
            Self::initialize(&mut conn)?;
        }
        Ok(Self { conn })
    }
    fn initialize(conn: &mut Connection) -> Result<()> {
        // Serialize first-time setup within this process. Normal opens only read
        // user_version, so playback never runs DDL alongside a background writer.
        static INITIALIZE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = INITIALIZE.lock().unwrap_or_else(|e| e.into_inner());
        if conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? != 0 {
            return Ok(());
        }
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Another process may have completed initialization while we waited.
        if tx.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? != 0 {
            return Ok(());
        }
        tx.execute_batch("
        CREATE TABLE IF NOT EXISTS playlists(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS entries(id INTEGER PRIMARY KEY AUTOINCREMENT, playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE, url TEXT NOT NULL, title TEXT NOT NULL, artist TEXT NOT NULL, duration REAL NOT NULL DEFAULT 0, position INTEGER NOT NULL);
        CREATE INDEX IF NOT EXISTS entries_order ON entries(playlist_id,position,id);
        CREATE INDEX IF NOT EXISTS entries_title ON entries(playlist_id,title COLLATE NOCASE,id);
        CREATE INDEX IF NOT EXISTS entries_artist ON entries(playlist_id,artist COLLATE NOCASE,id);
        CREATE INDEX IF NOT EXISTS entries_duration ON entries(playlist_id,duration,id);
        CREATE VIRTUAL TABLE IF NOT EXISTS entry_fts USING fts5(title,artist,content='entries',content_rowid='id', tokenize='unicode61 remove_diacritics 2');
        CREATE TRIGGER IF NOT EXISTS entries_ai AFTER INSERT ON entries BEGIN INSERT INTO entry_fts(rowid,title,artist) VALUES(new.id,new.title,new.artist); END;
        CREATE TRIGGER IF NOT EXISTS entries_ad AFTER DELETE ON entries BEGIN INSERT INTO entry_fts(entry_fts,rowid,title,artist) VALUES('delete',old.id,old.title,old.artist); END;
        CREATE TRIGGER IF NOT EXISTS entries_au AFTER UPDATE OF title,artist ON entries BEGIN INSERT INTO entry_fts(entry_fts,rowid,title,artist) VALUES('delete',old.id,old.title,old.artist); INSERT INTO entry_fts(rowid,title,artist) VALUES(new.id,new.title,new.artist); END;
        CREATE TABLE IF NOT EXISTS google_playlists(playlist_id INTEGER PRIMARY KEY REFERENCES playlists(id) ON DELETE CASCADE, remote_id TEXT NOT NULL, account TEXT NOT NULL, UNIQUE(account,remote_id));
        CREATE TABLE IF NOT EXISTS google_entries(entry_id INTEGER PRIMARY KEY REFERENCES entries(id) ON DELETE CASCADE, remote_id TEXT NOT NULL UNIQUE);
        CREATE TABLE IF NOT EXISTS google_recent(playlist_id INTEGER PRIMARY KEY REFERENCES playlists(id) ON DELETE CASCADE, changed INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS google_deleted(account TEXT NOT NULL,remote_id TEXT NOT NULL,changed INTEGER NOT NULL,PRIMARY KEY(account,remote_id));
        CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS scrobbles(id INTEGER PRIMARY KEY,artist TEXT NOT NULL,track TEXT NOT NULL,started INTEGER NOT NULL);
        PRAGMA user_version=1;")?;
        tx.commit()?;
        Ok(())
    }
    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        Ok(self.conn.prepare("SELECT p.id,p.name,count(e.id),EXISTS(SELECT 1 FROM google_playlists g WHERE g.playlist_id=p.id) FROM playlists p LEFT JOIN entries e ON e.playlist_id=p.id GROUP BY p.id ORDER BY p.name COLLATE NOCASE")?.query_map([],|r|Ok(Playlist{id:r.get(0)?,name:r.get(1)?,count:r.get(2)?,google:r.get(3)?}))?.collect::<rusqlite::Result<_>>()?)
    }
    pub fn create(&self, name: &str) -> Result<i64> {
        anyhow::ensure!(!name.trim().is_empty(), "Enter a playlist name");
        self.conn
            .execute("INSERT INTO playlists(name) VALUES(?)", [name.trim()])?;
        Ok(self.conn.last_insert_rowid())
    }
    pub fn rename(&self, id: i64, name: &str) -> Result<()> {
        anyhow::ensure!(!name.trim().is_empty(), "Enter a playlist name");
        self.conn.execute(
            "UPDATE playlists SET name=? WHERE id=?",
            params![name.trim(), id],
        )?;
        Ok(())
    }
    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM playlists WHERE id=?", [id])?;
        Ok(())
    }
    pub fn remove(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM entries WHERE id=?", [id])?;
        Ok(())
    }
    pub fn add(&mut self, playlist: i64, tracks: &[Track]) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let start: i64 = tx.query_row(
            "SELECT coalesce(max(position),-1)+1 FROM entries WHERE playlist_id=?",
            [playlist],
            |r| r.get(0),
        )?;
        {
            let mut stmt=tx.prepare("INSERT INTO entries(playlist_id,url,title,artist,duration,position) VALUES(?,?,?,?,?,?)")?;
            for (i, t) in tracks.iter().enumerate() {
                stmt.execute(params![
                    playlist,
                    t.url,
                    t.title,
                    t.artist,
                    t.duration,
                    start + i as i64
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn move_entry(&mut self, id: i64, delta: i32) -> Result<()> {
        use rusqlite::OptionalExtension;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (playlist, position): (i64, i64) = tx.query_row(
            "SELECT playlist_id,position FROM entries WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let sql = if delta < 0 {
            "SELECT id,position FROM entries WHERE playlist_id=? AND position<? ORDER BY position DESC LIMIT 1"
        } else {
            "SELECT id,position FROM entries WHERE playlist_id=? AND position>? ORDER BY position LIMIT 1"
        };
        let other: Option<(i64, i64)> = tx
            .query_row(sql, params![playlist, position], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        if let Some((other_id, other_position)) = other {
            tx.execute("UPDATE entries SET position=CASE id WHEN ?1 THEN ?2 ELSE ?3 END WHERE id IN (?1,?4)",params![id,other_position,position,other_id])?;
        }
        tx.commit()?;
        Ok(())
    }
    /// Move an entry before another entry, or to the end, atomically.
    pub fn move_entry_before(&mut self, id: i64, before: Option<i64>) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let playlist: i64 =
            tx.query_row("SELECT playlist_id FROM entries WHERE id=?", [id], |r| {
                r.get(0)
            })?;
        let mut ids = {
            let mut stmt =
                tx.prepare("SELECT id FROM entries WHERE playlist_id=? ORDER BY position,id")?;
            stmt.query_map([playlist], |r| r.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        if before == Some(id) {
            return Ok(());
        }
        ids.retain(|entry| *entry != id);
        let target = match before {
            Some(anchor) => ids
                .iter()
                .position(|entry| *entry == anchor)
                .ok_or_else(|| anyhow::anyhow!("Drop target is no longer in this playlist"))?,
            None => ids.len(),
        };
        ids.insert(target, id);
        for (position, entry) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE entries SET position=? WHERE id=?",
                params![position, entry],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn tracks(&self, playlist: i64, query: &str, sort: Sort) -> Result<Vec<Track>> {
        // Quote every token: user text can never become FTS query syntax.
        let query = query
            .split_whitespace()
            .map(|s| format!("\"{}\"*", s.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
        let order = match sort {
            Sort::Position => "e.position,e.id",
            Sort::Title => "e.title COLLATE NOCASE,e.id",
            Sort::Artist => "e.artist COLLATE NOCASE,e.title COLLATE NOCASE,e.id",
            Sort::Duration => "e.duration,e.id",
        };
        let filter = if query.is_empty() {
            ""
        } else {
            " AND e.id IN (SELECT rowid FROM entry_fts WHERE entry_fts MATCH ?2)"
        };
        let source = if query.is_empty() {
            "entries e"
        } else {
            "entries e NOT INDEXED"
        };
        // Rank the full playlist before filtering, in the same read snapshot.
        // The ordering index covers this ID-only pass, including deletion gaps.
        let sql = format!(
            "WITH playlist_indexes AS (
                SELECT id,row_number() OVER (ORDER BY position,id) AS ordinal
                FROM entries WHERE playlist_id=?1
             )
             SELECT e.id,e.url,e.title,e.artist,e.duration,p.ordinal
             FROM {source} JOIN playlist_indexes p ON p.id=e.id
             WHERE e.playlist_id=?1{filter} ORDER BY {order}"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let map = |r: &rusqlite::Row| {
            Ok(Track {
                id: r.get(0)?,
                playlist_index: r.get(5)?,
                url: r.get(1)?,
                title: r.get(2)?,
                artist: r.get(3)?,
                duration: r.get(4)?,
            })
        };
        let rows = if query.is_empty() {
            stmt.query_map(params![playlist], map)?
        } else {
            stmt.query_map(params![playlist, query], map)?
        };
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
    /// Reconcile a complete catalog only after all remote pages have arrived.
    pub fn reconcile_catalog(
        &mut self,
        account: &str,
        lists: &[(String, String)],
    ) -> Result<Vec<(i64, String)>> {
        use rusqlite::OptionalExtension;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut result = Vec::new();
        for (remote, name) in lists {
            let deleted:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM google_deleted WHERE account=? AND remote_id=? AND changed>unixepoch()-60)",params![account,remote],|r|r.get(0))?;
            if deleted {
                continue;
            }
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT playlist_id FROM google_playlists WHERE account=? AND remote_id=?",
                    params![account, remote],
                    |r| r.get(0),
                )
                .optional()?;
            let id = match existing {
                Some(id) => {
                    tx.execute("UPDATE playlists SET name=? WHERE id=? AND NOT EXISTS(SELECT 1 FROM google_recent WHERE playlist_id=playlists.id AND changed>unixepoch()-60)", params![name, id])?;
                    id
                }
                None => {
                    tx.execute("INSERT INTO playlists(name) VALUES(?)", [name])?;
                    let id = tx.last_insert_rowid();
                    tx.execute(
                        "INSERT INTO google_playlists VALUES(?,?,?)",
                        params![id, remote, account],
                    )?;
                    id
                }
            };
            result.push((id, remote.clone()));
        }
        let old: Vec<(i64, String)> = tx
            .prepare("SELECT playlist_id,remote_id FROM google_playlists WHERE account=?")?
            .query_map([account], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (id, remote) in old {
            if !lists.iter().any(|(r, _)| *r == remote) && !tx.query_row("SELECT EXISTS(SELECT 1 FROM google_recent WHERE playlist_id=? AND changed>unixepoch()-60)",[id],|r|r.get::<_,bool>(0))? {
                tx.execute("DELETE FROM playlists WHERE id=?", [id])?;
            }
        }
        tx.commit()?;
        Ok(result)
    }
    /// Keep entry IDs stable by playlist-item ID, including duplicate videos.
    pub fn reconcile_entries(&mut self, playlist: i64, tracks: &[(String, Track)]) -> Result<()> {
        use rusqlite::OptionalExtension;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut keep = std::collections::HashSet::new();
        for (position, (remote, t)) in tracks.iter().enumerate() {
            let existing: Option<i64> = tx.query_row("SELECT entry_id FROM google_entries g JOIN entries e ON e.id=g.entry_id WHERE remote_id=? AND playlist_id=?",params![remote,playlist],|r|r.get(0)).optional()?;
            let id = if let Some(id) = existing {
                tx.execute("UPDATE entries SET url=?,title=?,artist=?,duration=CASE WHEN ?>0 THEN ? ELSE duration END,position=? WHERE id=?",params![t.url,t.title,t.artist,t.duration,t.duration,position,id])?;
                id
            } else {
                tx.execute("INSERT INTO entries(playlist_id,url,title,artist,duration,position) VALUES(?,?,?,?,?,?)",params![playlist,t.url,t.title,t.artist,t.duration,position])?;
                let id = tx.last_insert_rowid();
                tx.execute(
                    "INSERT INTO google_entries VALUES(?,?)",
                    params![id, remote],
                )?;
                id
            };
            anyhow::ensure!(
                keep.insert(id),
                "Duplicate remote playlist-item ID; cache left intact"
            );
        }
        let old: Vec<i64> = tx
            .prepare("SELECT id FROM entries WHERE playlist_id=?")?
            .query_map([playlist], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for id in old {
            if !keep.contains(&id) {
                tx.execute("DELETE FROM entries WHERE id=?", [id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn recently_written(&self, id: i64) -> Result<bool> {
        Ok(self.conn.query_row("SELECT EXISTS(SELECT 1 FROM google_recent WHERE playlist_id=? AND changed>unixepoch()-60)",[id],|r|r.get(0))?)
    }
    pub fn mark_written(&self, id: i64) -> Result<()> {
        self.conn.execute("INSERT INTO google_recent VALUES(?,unixepoch()) ON CONFLICT(playlist_id) DO UPDATE SET changed=excluded.changed",[id])?;
        Ok(())
    }
    pub fn append_google(&self, playlist: i64, remote: &str, t: &Track) -> Result<()> {
        let tx = rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        self.conn.execute("INSERT INTO entries(playlist_id,url,title,artist,duration,position) SELECT ?1,?2,?3,?4,?5,coalesce(max(position),-1)+1 FROM entries WHERE playlist_id=?1",params![playlist,t.url,t.title,t.artist,t.duration])?;
        self.conn.execute(
            "INSERT INTO google_entries VALUES(?,?)",
            params![self.conn.last_insert_rowid(), remote],
        )?;
        self.mark_written(playlist)?;
        tx.commit()?;
        Ok(())
    }
    pub fn google_playlist(&self, id: i64) -> Result<Option<(String, String)>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .conn
            .query_row(
                "SELECT remote_id,account FROM google_playlists WHERE playlist_id=?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }
    pub fn entry_playlist(&self, id: i64) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT playlist_id FROM entries WHERE id=?", [id], |r| {
                r.get(0)
            })?)
    }
    pub fn google_entry(&self, id: i64) -> Result<String> {
        Ok(self.conn.query_row(
            "SELECT remote_id FROM google_entries WHERE entry_id=?",
            [id],
            |r| r.get(0),
        )?)
    }
    pub fn save(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
    pub fn load(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
                r.get(0)
            })
            .ok()
    }
}
pub fn data_dir() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("MOUSA_DATA_DIR") {
        let p = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&p).expect("create data directory");
        return p;
    }
    let p = directories::ProjectDirs::from("app", "Mousa", "Mousa")
        .expect("home directory")
        .data_dir()
        .to_owned();
    std::fs::create_dir_all(&p).expect("create data directory");
    p
}
pub fn library() -> Result<Library> {
    Library::open(data_dir().join("library.db"))
}
