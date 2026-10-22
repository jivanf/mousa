//! Live regression probe: use a disposable MOUSA_DATA_DIR and a playable URL argument.
use mousa::{
    db::{self, Track},
    player::{Command, Player, Snapshot},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
fn wait_for(player: &Player, condition: impl Fn(&Snapshot) -> bool) {
    let start = Instant::now();
    loop {
        let state = player.state.lock().unwrap().clone();
        assert!(state.error.is_empty(), "{}", state.error);
        if condition(&state) {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "Playback did not reach expected state"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
fn main() {
    assert!(
        std::env::var("MOUSA_DATA_DIR").is_ok(),
        "Use a disposable data directory"
    );
    let track = Track {
        id: 1,
        playlist_index: 0,
        url: std::env::args().nth(1).expect("YouTube URL"),
        title: "Playback regression probe".into(),
        artist: "".into(),
        duration: 258.,
    };
    let saved = Snapshot {
        queue: Arc::new(vec![track.clone()]),
        paused: true,
        volume: 0.,
        ..Default::default()
    };
    db::library()
        .unwrap()
        .save("session", &serde_json::to_string(&saved).unwrap())
        .unwrap();
    let player = Player::start();
    player.send(Command::Pause(false)); // Do not wait for startup preparation.
    wait_for(&player, |s| s.position > 1.);
    println!("Immediate Play after startup advances");
    player.send(Command::Pause(true));
    wait_for(&player, |s| s.paused);
    std::thread::sleep(Duration::from_millis(300));
    let position = player.state.lock().unwrap().position;
    std::thread::sleep(Duration::from_millis(500));
    assert!((player.state.lock().unwrap().position - position).abs() < 0.1);
    player.send(Command::Pause(false));
    wait_for(&player, |s| s.position > position + 0.5);
    println!("Pause/resume advances");
    player.send(Command::Queue(vec![track.clone(), track.clone(), track], 0));
    wait_for(&player, |s| s.position < 0.1);
    player.send(Command::Pause(true)); // Pause while the resolver is running.
    wait_for(&player, |s| s.paused);
    std::thread::sleep(Duration::from_secs(3));
    assert!(player.state.lock().unwrap().paused);
    player.send(Command::Next); // Cancel that lookup and resolve another selection.
    wait_for(&player, |s| s.index == 1 && s.position > 1.);
    println!("Track switching during resolution advances");
    let duration = player.state.lock().unwrap().duration;
    player.send(Command::Seek(duration - 2.));
    wait_for(&player, |s| s.index == 2 && s.position > 1.);
    println!("Automatic transition with pending preparation advances");
    player.send(Command::Stop);
}
