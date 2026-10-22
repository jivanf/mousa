//! Reads the account cache by default. --write-roundtrip creates and deletes only a private test playlist.
use mousa::{
    db::{self, Sort, Track},
    sync,
};
fn main() -> anyhow::Result<()> {
    if std::env::args().any(|a| a == "--write-roundtrip") {
        let id = sync::create(
            &format!(
                "Mousa temporary integration test {}",
                mousa::integrations::now()
            ),
            true,
        )?;
        let result = (|| -> anyhow::Result<()> {
            sync::rename(id, "Mousa temporary integration test (renamed)")?;
            let t = Track {
                id: 0,
                playlist_index: 0,
                unavailable: None,
                url: "https://www.youtube.com/watch?v=jNQXAC9IVRw".into(),
                title: "Me at the zoo".into(),
                artist: "jawed".into(),
                duration: 19.,
            };
            sync::add(id, &[t.clone(), t])?;
            let tracks = db::library()?.tracks(id, "", Sort::Position)?;
            anyhow::ensure!(tracks.len() == 2, "Expected two separate playlist entries");
            sync::move_entry(tracks[1].id, -1)?;
            let reordered = db::library()?.tracks(id, "", Sort::Position)?;
            anyhow::ensure!(
                reordered[0].id == tracks[1].id,
                "Remote reorder did not persist"
            );
            sync::remove(reordered[0].id)?;
            anyhow::ensure!(
                db::library()?.tracks(id, "", Sort::Position)?.len() == 1,
                "Removal did not persist"
            );
            Ok(())
        })();
        let cleanup = sync::delete(id);
        cleanup?;
        result?;
        println!(
            "Private test playlist: create, rename, duplicate add, reorder, remove, delete passed; test playlist removed."
        );
    }
    let mut completed = 0;
    sync::refresh(|| {
        completed += 1;
    })?;
    let db = db::library()?;
    let lists = db.playlists()?;
    println!(
        "Automatic refresh completed: {} Google playlists, {completed} cache updates.",
        lists.iter().filter(|p| p.google).count()
    );
    Ok(())
}
