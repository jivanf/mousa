//! Isolated UI fixture: MOUSA_DATA_DIR=/tmp/mousa-demo cargo run --example seed
use mousa::db::{self, Track};
fn main() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("MOUSA_DATA_DIR").is_ok(),
        "Set MOUSA_DATA_DIR to a disposable directory"
    );
    let mut db = db::library()?;
    anyhow::ensure!(
        db.playlists()?.is_empty(),
        "Fixture directory must have an empty library"
    );
    let id = db.create("Performance lab · 5,000 tracks")?;
    let tracks: Vec<_> = (0..5000)
        .map(|i| Track {
            id: 0,
            url: "https://www.youtube.com/watch?v=jNQXAC9IVRw".into(),
            title: format!("Synthetic track {:04}", i + 1),
            artist: format!("Test artist {:02}", i % 50),
            duration: 19.,
        })
        .collect();
    db.add(id, &tracks)?;
    println!(
        "Created 5,000 explicitly synthetic rows in the isolated test library. Every row uses the same public playback test URL."
    );
    Ok(())
}
