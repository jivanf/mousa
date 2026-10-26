#![cfg(target_os = "windows")]

/// Run explicitly on a Windows desktop with the installed playback DLL on PATH.
#[test]
#[ignore = "requires the native Windows Credential Manager"]
fn windows_credentials_round_trip() {
    let name = format!("windows-smoke-{}", std::process::id());
    let entry = keyring::Entry::new("app.mousa.tests", &name).unwrap();
    entry.set_password("mousa-test-value").unwrap();
    let value = entry.get_password();
    entry.delete_credential().unwrap();
    assert_eq!(value.unwrap(), "mousa-test-value");
    assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
}

#[test]
#[ignore = "requires libmpv and a Windows audio device"]
fn windows_playback_initializes_and_decodes_audio() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("silence.wav");
    let samples = 44_100u32;
    let data_size = samples * 2;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&samples.to_le_bytes());
    wav.extend_from_slice(&(samples * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.resize(wav.len() + data_size as usize, 0);
    std::fs::write(&path, wav).unwrap();
    let mpv = libmpv2::Mpv::with_initializer(|m| {
        m.set_property("vo", "null")?;
        m.set_property("vid", "no")?;
        m.set_property("volume", 0.0)?;
        m.set_property("keep-open", "yes")?;
        Ok(())
    })
    .unwrap();
    mpv.command("loadfile", &[path.to_str().unwrap(), "replace"])
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if mpv.get_property::<f64>("time-pos").unwrap_or(0.) > 0.1 {
            assert!((mpv.get_property::<f64>("duration").unwrap() - 1.).abs() < 0.01);
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Audio did not advance"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
