//! Keep direct launches through the same managed updater as desktop shortcuts.
use std::{path::PathBuf, process::Command};

pub fn dispatch() {
    if std::env::args().any(|arg| arg == "--version") {
        println!("Mousa {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }
    if std::env::var_os("MOUSA_SUPERVISED").is_some() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(root) = exe.parent() else { return };
    if !root.join("mousa-managed.json").is_file() {
        return;
    }
    let launcher = root.join(if cfg!(windows) {
        "mousa-launcher.exe"
    } else {
        "mousa-launcher"
    });
    if let Ok(status) = Command::new(launcher)
        .args(std::env::args_os().skip(1))
        .status()
    {
        std::process::exit(status.code().unwrap_or(1));
    }
    // A damaged launcher must not prevent using the bootstrap application.
}

pub fn healthy() {
    if let Some(path) = std::env::var_os("MOUSA_HEALTH_FILE") {
        let _ = std::fs::write(PathBuf::from(path), b"ready");
    }
}
