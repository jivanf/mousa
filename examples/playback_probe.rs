//! Run with MOUSA_DATA_DIR pointing at a disposable directory.
use mousa::{
    db::Track,
    player::{Command, Player},
};
use std::{io::Write, time::Duration};
fn main() {
    assert!(
        std::env::var("MOUSA_DATA_DIR").is_ok(),
        "Use a disposable MOUSA_DATA_DIR"
    );
    let path = mousa::db::data_dir().join("silence.wav");
    let len = 44100u32 * 2 * 10;
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(len + 36).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&44100u32.to_le_bytes()).unwrap();
    f.write_all(&88200u32.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&len.to_le_bytes()).unwrap();
    f.write_all(&vec![0; len as usize]).unwrap();
    drop(f);
    let mut writer = mousa::db::library().unwrap();
    writer
        .conn
        .execute("DELETE FROM settings WHERE key='session'", [])
        .unwrap();
    let p = Player::start();
    let refresh = writer
        .conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    refresh
        .execute(
            "INSERT OR REPLACE INTO settings VALUES('probe-refresh','in progress')",
            [],
        )
        .unwrap();
    p.send(Command::Volume(0.));
    p.send(Command::Queue(
        vec![Track {
            id: 1,
            playlist_index: 0,
            url: path.to_string_lossy().into(),
            title: "Silent playback probe".into(),
            artist: "Mousa QA".into(),
            duration: 10.,
        }],
        0,
    ));
    std::thread::sleep(Duration::from_secs(2));
    {
        let s = p.state.lock().unwrap();
        assert!(s.position > 0., "Playback did not advance: {}", s.error);
        assert!(s.duration >= 9.);
        println!(
            "libmpv playback advances: {:.2}s / {:.2}s",
            s.position, s.duration
        );
    }
    p.send(Command::Pause(true));
    std::thread::sleep(Duration::from_millis(500));
    let pos = p.state.lock().unwrap().position;
    std::thread::sleep(Duration::from_millis(500));
    assert!((p.state.lock().unwrap().position - pos).abs() < 0.1);
    p.send(Command::Seek(6.));
    std::thread::sleep(Duration::from_millis(500));
    assert!(p.state.lock().unwrap().position >= 5.5);
    refresh.commit().unwrap();
    // A failed periodic save must retry even without a new playback command.
    let deadline = std::time::Instant::now() + Duration::from_secs(6);
    loop {
        let saved = writer
            .load("session")
            .and_then(|json| serde_json::from_str::<mousa::player::Snapshot>(&json).ok());
        if saved.is_some_and(|s| s.paused && s.position >= 5.5) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Session save did not retry after the refresh released its lock"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    p.send(Command::Stop);
    println!("Playback, pause and seek during a database write; session-save retry; stop: passed");
    p.send(Command::Queue(
        vec![Track {
            id: 2,
            playlist_index: 0,
            url: path.with_file_name("missing.wav").to_string_lossy().into(),
            title: "Missing file probe".into(),
            artist: "".into(),
            duration: 10.,
        }],
        0,
    ));
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let state = p.state.lock().unwrap();
        if !state.error.is_empty() {
            assert!(state.paused);
            break;
        }
        drop(state);
        assert!(
            std::time::Instant::now() < deadline,
            "Failed load was ignored"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("Failed load reports an error without waiting for the startup timeout: passed");
}
