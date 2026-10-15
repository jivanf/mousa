//! Network smoke test. Uses a disposable library, muted audio, and no downloads.
use mousa::{
    integrations,
    player::{Command, Player},
};
fn main() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("MOUSA_DATA_DIR").is_ok(),
        "Use a disposable MOUSA_DATA_DIR"
    );
    let tracks = integrations::import_url("https://www.youtube.com/watch?v=jNQXAC9IVRw")?;
    println!("Imported {}: {}", tracks.len(), tracks[0].title);
    let p = Player::start();
    p.send(Command::Volume(0.));
    p.send(Command::Queue(tracks, 0));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let s = p.state.lock().unwrap();
        anyhow::ensure!(s.error.is_empty(), "{}", s.error);
        if s.position > 0.5 {
            println!("YouTube playback advances: {:.2}s", s.position);
            drop(s);
            p.send(Command::Stop);
            break;
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "YouTube playback timed out"
        );
    }
    Ok(())
}
