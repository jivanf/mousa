//! Persistent refresh scheduling. All times are Unix seconds; no network work here.
use crate::db::Library;
use anyhow::{Result, ensure};
use chrono::{TimeZone, Utc};
use chrono_tz::America::Los_Angeles;
use rusqlite::{OptionalExtension, params};

pub const HOUR: i64 = 3600;
pub const DAY: i64 = 24 * HOUR;
const BACKGROUND_BUDGET: i64 = 2000;

pub fn init(db: &Library) -> Result<()> {
    db.conn.execute_batch("CREATE TABLE IF NOT EXISTS youtube_refresh(
        playlist_id INTEGER PRIMARY KEY REFERENCES playlists(id) ON DELETE CASCADE,
        opened INTEGER NOT NULL DEFAULT 0, visits INTEGER NOT NULL DEFAULT 0,
        checked INTEGER NOT NULL DEFAULT 0, attempted INTEGER NOT NULL DEFAULT 0,
        unchanged INTEGER NOT NULL DEFAULT 0, fingerprint TEXT NOT NULL DEFAULT '');
        CREATE TABLE IF NOT EXISTS youtube_metadata(video TEXT PRIMARY KEY,duration REAL NOT NULL,fetched INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS youtube_usage(day TEXT NOT NULL,endpoint TEXT NOT NULL,units INTEGER NOT NULL DEFAULT 0,background INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(day,endpoint));")?;
    Ok(())
}

pub fn opened(db: &Library, id: i64, now: i64) -> Result<()> {
    db.conn.execute("INSERT INTO youtube_refresh(playlist_id,opened,visits) VALUES(?,?,1)
        ON CONFLICT(playlist_id) DO UPDATE SET visits=visits+CASE WHEN opened<? THEN 1 ELSE 0 END,opened=excluded.opened", params![id,now,now-600])?;
    Ok(())
}
pub fn due(db: &Library, id: i64, now: i64, force: bool) -> Result<bool> {
    let state = db
        .conn
        .query_row(
            "SELECT checked,attempted,unchanged FROM youtube_refresh WHERE playlist_id=?",
            [id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, u32>(2)?,
                ))
            },
        )
        .optional()?;
    Ok(match state {
        None => true,
        Some((checked, attempted, unchanged)) => {
            (force && now - attempted >= 5 && now - checked >= 5)
                || (!force
                    && now - attempted >= 600
                    && (checked == 0 || now - checked >= interval(unchanged)))
        }
    })
}
fn interval(unchanged: u32) -> i64 {
    (HOUR * (1_i64 << unchanged.min(5))).min(DAY)
}
pub fn attempted(db: &Library, id: i64, now: i64) -> Result<()> {
    db.conn.execute("INSERT INTO youtube_refresh(playlist_id,attempted) VALUES(?,?) ON CONFLICT(playlist_id) DO UPDATE SET attempted=excluded.attempted",params![id,now])?;
    Ok(())
}
pub fn completed(db: &Library, id: i64, now: i64, fingerprint: &str) -> Result<()> {
    db.conn.execute("INSERT INTO youtube_refresh(playlist_id,checked,fingerprint) VALUES(?,?,?)
        ON CONFLICT(playlist_id) DO UPDATE SET checked=excluded.checked,
        unchanged=CASE WHEN fingerprint=excluded.fingerprint THEN min(unchanged+1,5) ELSE 0 END,fingerprint=excluded.fingerprint",params![id,now,fingerprint])?;
    Ok(())
}
pub fn candidate(db: &Library, now: i64, active: Option<i64>) -> Result<Option<i64>> {
    Ok(db.conn.query_row("SELECT r.playlist_id FROM youtube_refresh r JOIN google_playlists g ON g.playlist_id=r.playlist_id
        WHERE visits>=3 AND opened>? AND checked<? AND attempted<? AND (? IS NULL OR r.playlist_id<>?)
        ORDER BY opened DESC LIMIT 1",params![now-7*DAY,now-DAY,now-600,active,active],|r|r.get(0)).optional()?)
}
pub fn next_reset(now: i64) -> i64 {
    let local = Utc
        .timestamp_opt(now, 0)
        .unwrap()
        .with_timezone(&Los_Angeles);
    let tomorrow = local
        .date_naive()
        .succ_opt()
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    Los_Angeles
        .from_local_datetime(&tomorrow)
        .single()
        .unwrap()
        .timestamp()
}
pub fn paused(db: &Library, now: i64) -> bool {
    db.load("youtube-quota-until")
        .and_then(|s| s.parse::<i64>().ok())
        .is_some_and(|until| until > now)
}
pub fn pause(db: &Library, now: i64) -> Result<()> {
    db.save("youtube-quota-until", &next_reset(now).to_string())
}
pub fn background_available(db: &Library, now: i64) -> Result<bool> {
    let day = Utc
        .timestamp_opt(now, 0)
        .unwrap()
        .with_timezone(&Los_Angeles)
        .format("%F")
        .to_string();
    let used: i64 = db.conn.query_row(
        "SELECT coalesce(sum(background),0) FROM youtube_usage WHERE day=?",
        [day],
        |r| r.get(0),
    )?;
    Ok(used < BACKGROUND_BUDGET)
}

/// Reserve estimated units before each request, including failed requests.
pub fn reserve(db: &Library, now: i64, endpoint: &str, units: i64, background: bool) -> Result<()> {
    ensure!(
        !paused(db, now),
        "YouTube's daily quota is exhausted. Sync and YouTube edits will resume after midnight Pacific time. Cached playlists remain available."
    );
    let day = Utc
        .timestamp_opt(now, 0)
        .unwrap()
        .with_timezone(&Los_Angeles)
        .format("%F")
        .to_string();
    let tx = db.conn.unchecked_transaction()?;
    let used: i64 = tx.query_row(
        "SELECT coalesce(sum(background),0) FROM youtube_usage WHERE day=?",
        [&day],
        |r| r.get(0),
    )?;
    ensure!(
        !background || used + units <= BACKGROUND_BUDGET,
        "Automatic refresh budget reached for today. Use Refresh for the current playlist, or wait until tomorrow."
    );
    tx.execute("INSERT INTO youtube_usage VALUES(?,?,?,?) ON CONFLICT(day,endpoint) DO UPDATE SET units=units+excluded.units,background=background+excluded.background", params![day,endpoint,units,if background {units} else {0}])?;
    tx.commit()?;
    Ok(())
}

/// Read-only display of cache freshness, including a truly unloaded playlist.
pub fn freshness(db: &Library, id: i64, now: i64) -> Result<String> {
    if db.google_playlist(id)?.is_none() {
        return Ok(String::new());
    }
    init(db)?;
    let checked: Option<i64> = db
        .conn
        .query_row(
            "SELECT checked FROM youtube_refresh WHERE playlist_id=?",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(match checked.filter(|v| *v > 0) {
        None => "Not refreshed yet".into(),
        Some(t) if now - t < 60 => "Updated just now".into(),
        Some(t) if now - t < HOUR => format!("Updated {} minutes ago", (now - t) / 60),
        Some(t) if now - t < DAY => format!("Updated {} hours ago", (now - t) / HOUR),
        Some(t) => format!("Updated {} days ago", (now - t) / DAY),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Library {
        let db = Library::open(":memory:").unwrap();
        init(&db).unwrap();
        db
    }
    #[test]
    fn scheduling_and_quota_pause_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = Library::open(&path).unwrap();
        init(&db).unwrap();
        let id = db.create("test").unwrap();
        let now = 1_700_000_000;
        attempted(&db, id, now).unwrap();
        completed(&db, id, now, "same").unwrap();
        pause(&db, now).unwrap();
        drop(db);
        let db = Library::open(&path).unwrap();
        init(&db).unwrap();
        assert!(!due(&db, id, now + 1, false).unwrap());
        assert!(!due(&db, id, now + 1, true).unwrap());
        assert!(paused(&db, now + 1));
    }
    #[test]
    fn activity_and_adaptive_freshness() {
        let db = db();
        let id = db.create("test").unwrap();
        let now = 10 * DAY;
        assert!(due(&db, id, now, false).unwrap());
        opened(&db, id, now).unwrap();
        attempted(&db, id, now).unwrap();
        assert!(!due(&db, id, now + 1, false).unwrap());
        completed(&db, id, now, "a").unwrap();
        assert!(due(&db, id, now + HOUR, false).unwrap());
        completed(&db, id, now + HOUR, "a").unwrap();
        assert!(!due(&db, id, now + 2 * HOUR, false).unwrap());
        assert!(due(&db, id, now + 2 * HOUR, true).unwrap());
        completed(&db, id, now + 2 * HOUR, "b").unwrap();
        assert!(due(&db, id, now + 3 * HOUR, false).unwrap());
    }
    #[test]
    fn unopened_playlists_are_never_background_candidates() {
        let db = db();
        let id = db.create("test").unwrap();
        db.conn
            .execute(
                "INSERT INTO google_playlists VALUES(?,'remote','account')",
                [id],
            )
            .unwrap();
        assert_eq!(candidate(&db, 10 * DAY, None).unwrap(), None);
        for t in [DAY, DAY + HOUR, DAY + 2 * HOUR] {
            opened(&db, id, t).unwrap();
        }
        assert_eq!(candidate(&db, 2 * DAY, None).unwrap(), Some(id));
        assert_eq!(candidate(&db, 2 * DAY, Some(id)).unwrap(), None);
        assert_eq!(candidate(&db, 10 * DAY, None).unwrap(), None);
    }
    #[test]
    fn budgets_reserve_manual_requests_and_pause_until_reset() {
        let db = db();
        let now = 1_700_000_000;
        reserve(&db, now, "read", 2000, true).unwrap();
        assert!(reserve(&db, now, "read", 1, true).is_err());
        reserve(&db, now, "read", 1, false).unwrap();
        pause(&db, now).unwrap();
        assert!(reserve(&db, now, "read", 1, false).is_err());
        reserve(&db, next_reset(now), "read", 1, true).unwrap();
    }
    #[test]
    fn reset_observes_pacific_daylight_saving() {
        for (date, hours) in [((2026, 3, 8), 23), ((2026, 11, 1), 25)] {
            let now = Los_Angeles
                .with_ymd_and_hms(date.0, date.1, date.2, 0, 0, 0)
                .unwrap()
                .timestamp();
            assert_eq!(next_reset(now) - now, hours * HOUR);
        }
    }
}
