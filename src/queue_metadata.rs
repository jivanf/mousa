//! Saved queues retain their membership; refresh only metadata for matching entries.
use crate::{
    db::{self, Track},
    player::Snapshot,
};
use rusqlite::OptionalExtension;
use std::{collections::HashMap, sync::Arc};

pub fn read(queue: &[Track]) -> anyhow::Result<Vec<(Track, Track)>> {
    read_from(&db::library()?, queue)
}

fn read_from(db: &db::Library, queue: &[Track]) -> anyhow::Result<Vec<(Track, Track)>> {
    let mut stmt = db.conn.prepare_cached(
        "SELECT title,artist,duration,unavailable FROM entries WHERE id=?1 AND url=?2",
    )?;
    let mut updates = Vec::new();
    for old in queue {
        let new = stmt
            .query_row(rusqlite::params![old.id, old.url], |row| {
                Ok(Track {
                    id: old.id,
                    url: old.url.clone(),
                    title: row.get(0)?,
                    artist: row.get(1)?,
                    duration: row.get(2)?,
                    unavailable: row.get(3)?,
                    ..old.clone()
                })
            })
            .optional()?;
        if let Some(new) = new
            && new != *old
        {
            updates.push((old.clone(), new));
        }
    }
    Ok(updates)
}

pub fn apply(s: &mut Snapshot, updates: Vec<(Track, Track)>) {
    let updates: HashMap<_, _> = updates
        .into_iter()
        .map(|(old, new)| ((old.id, old.url.clone()), (old, new)))
        .collect();
    let queue = Arc::make_mut(&mut s.queue);
    for track in queue.iter_mut() {
        if let Some((old, new)) = updates.get(&(track.id, track.url.clone())) {
            // An older background result must not overwrite a newer update.
            if track.unavailable == old.unavailable {
                track.unavailable.clone_from(&new.unavailable);
            }
            if track.title == old.title {
                track.title.clone_from(&new.title);
            }
            if track.artist == old.artist {
                track.artist.clone_from(&new.artist);
            }
            if track.duration == old.duration && new.duration.is_finite() && new.duration > 0. {
                track.duration = new.duration;
            }
        }
    }
    if (!s.duration.is_finite() || s.duration <= 0.)
        && let Some(current) = queue.get(s.index)
    {
        s.duration = current.duration;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track(id: i64, title: &str, duration: f64) -> Track {
        Track {
            id,
            url: format!("https://www.youtube.com/watch?v={id}"),
            title: title.into(),
            artist: "Artist".into(),
            duration,
            playlist_index: 1,
            unavailable: None,
        }
    }
    #[test]
    fn hydrates_saved_queue_from_library_and_keeps_missing_entries() {
        let db = db::Library::open(":memory:").unwrap();
        db.conn
            .execute("INSERT INTO playlists(id,name) VALUES(1,'Test')", [])
            .unwrap();
        db.conn.execute("INSERT INTO entries(id,playlist_id,url,title,artist,duration,position) VALUES(1,1,'https://www.youtube.com/watch?v=1','Complete title','Artist',478,0)", []).unwrap();
        let old = track(1, "Complete...", 0.);
        let missing = track(2, "Removed from library", 20.);
        let mut s = Snapshot {
            queue: Arc::new(vec![old, missing.clone()]),
            ..Default::default()
        };
        let updates = read_from(&db, &s.queue).unwrap();
        apply(&mut s, updates);
        assert_eq!(s.current().unwrap().title, "Complete title");
        assert_eq!(s.queue[0].duration, 478.);
        assert_eq!(s.queue[0].playlist_index, 1);
        assert_eq!(s.queue[1], missing);
        assert!(read_from(&db, &s.queue).unwrap().is_empty());
    }
    #[test]
    fn updates_all_duplicates_without_changing_playback_or_order() {
        let old = track(1, "Short...", 0.);
        let full = track(1, "Short title in full", 478.);
        let other = track(2, "Other", 10.);
        let mut s = Snapshot {
            queue: Arc::new(vec![old.clone(), other.clone(), old.clone()]),
            index: 2,
            position: 42.,
            paused: false,
            ..Default::default()
        };
        apply(&mut s, vec![(old, full.clone())]);
        assert_eq!(*s.queue, vec![full.clone(), other, full]);
        assert_eq!(
            (s.index, s.position, s.paused, s.duration),
            (2, 42., false, 478.)
        );
    }
    #[test]
    fn stale_updates_keep_new_metadata_and_known_duration() {
        let old = track(1, "Short...", 0.);
        let current = track(1, "Already newer", 480.);
        let mut s = Snapshot {
            queue: Arc::new(vec![current.clone()]),
            duration: 480.,
            ..Default::default()
        };
        apply(&mut s, vec![(old, track(1, "Short title in full", 478.))]);
        assert_eq!(*s.queue, vec![current]);
        assert_eq!(s.duration, 480.);
    }
}
