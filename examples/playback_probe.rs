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
    let p = Player::start();
    p.send(Command::Volume(0.));
    p.send(Command::Queue(
        vec![Track {
            id: 1,
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
    p.send(Command::Stop);
    println!("Pause, seek, stop: passed");
}
