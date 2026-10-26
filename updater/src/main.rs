#![cfg_attr(windows, windows_subsystem = "windows")]

use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use reqwest::blocking::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const REPOSITORY: &str = "jivanf/mousa";
const MAX_DOWNLOAD: u64 = 512 * 1024 * 1024;
const INTERVAL: u64 = 24 * 60 * 60;
const PUBLIC_KEY: &str = include_str!("../release-public-key.hex");

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    installation_id: u64,
    active: Option<String>,
    previous: Option<String>,
    pending: bool,
    #[serde(default)]
    rejected: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    protocol: u32,
    version: String,
    #[serde(default)]
    installation_id: u64,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

fn executable(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

fn target() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        _ => bail!("Unsupported release target"),
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().context("Missing parent")?)?;
    serde_json::to_writer(&mut file, value)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|err| err.error)?;
    Ok(())
}

fn valid_version(value: &str) -> Result<Version> {
    let version = Version::parse(value)?;
    ensure!(
        version.to_string() == value && version.pre.is_empty() && version.build.is_empty(),
        "Expected canonical stable version"
    );
    Ok(version)
}

fn read_state(cache: &Path) -> Result<State> {
    let path = cache.join("state.json");
    if !path.exists() {
        return Ok(State::default());
    }
    let state: State = serde_json::from_reader(File::open(path)?)?;
    for value in state
        .active
        .iter()
        .chain(state.previous.iter())
        .chain(state.rejected.iter())
    {
        valid_version(value)?;
    }
    Ok(state)
}

fn app_path(root: &Path, cache: &Path, active: &Option<String>) -> PathBuf {
    match active {
        Some(version) => cache.join(version).join(executable("mousa")),
        None => root.join(executable("mousa")),
    }
}

fn command(path: &Path) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW, including version probes.
    }
    cmd
}

fn log(cache: &Path, message: &str) {
    let path = cache.join("update.log");
    if fs::metadata(&path).is_ok_and(|m| m.len() > 64 * 1024) {
        let _ = fs::remove_file(cache.join("update.previous.log"));
        let _ = fs::rename(&path, cache.join("update.previous.log"));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} {message}", now());
    }
}

fn key() -> Result<[u8; 32]> {
    let hex = PUBLIC_KEY.trim();
    ensure!(hex.len() == 64, "Invalid release public key");
    let mut key = [0; 32];
    for (index, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)?;
    }
    Ok(key)
}

fn release_version(
    release: &Release,
    current: &Version,
    repair: bool,
    rejected: &[String],
) -> Result<Option<Version>> {
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = valid_version(
        release
            .tag_name
            .strip_prefix('v')
            .context("Release tag must start with v")?,
    )?;
    if rejected.contains(&version.to_string())
        || version < *current
        || (version == *current && !repair)
    {
        return Ok(None);
    }
    Ok(Some(version))
}

fn validate_digest(path: &Path, expected: &str) -> Result<()> {
    let expected = expected
        .strip_prefix("sha256:")
        .context("Missing SHA-256 release digest")?;
    ensure!(
        expected.len() == 64 && expected.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid digest"
    );
    let mut input = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let size = input.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hasher.update(&buffer[..size]);
    }
    ensure!(
        format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected),
        "Release checksum mismatch"
    );
    Ok(())
}

fn probe(path: &Path, version: &str) -> Result<()> {
    // Poll rather than waiting forever if a damaged executable hangs during initialization.
    let mut child = command(path)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    for _ in 0..100 {
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "Release version probe failed");
            let mut output = String::new();
            child
                .stdout
                .take()
                .context("Missing version output")?
                .take(1024)
                .read_to_string(&mut output)?;
            ensure!(
                output.trim() == format!("Mousa {version}"),
                "Release version mismatch"
            );
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    bail!("Release version probe timed out")
}

fn stage(cache: &Path, current: &str, rejected: &[String], repair: bool) -> Result<Option<String>> {
    let current = valid_version(current)?;
    let client = Client::builder()
        .user_agent(concat!("Mousa/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .https_only(true)
        .build()?;
    let response = client
        .get(format!(
            "https://api.github.com/repos/{REPOSITORY}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .send()?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let mut metadata = String::new();
    response
        .error_for_status()?
        .take(1024 * 1024)
        .read_to_string(&mut metadata)?;
    let release: Release = serde_json::from_str(&metadata)?;
    let Some(version) = release_version(&release, &current, repair, rejected)? else {
        return Ok(None);
    };
    let version = version.to_string();
    let name = format!("mousa-{version}-{}.tar.gz", target()?);
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .context("Release lacks target archive")?;
    ensure!(
        asset.size > 0 && asset.size <= MAX_DOWNLOAD,
        "Release archive too large"
    );
    ensure!(
        asset.browser_download_url.starts_with(&format!(
            "https://github.com/{REPOSITORY}/releases/download/"
        )),
        "Unexpected release download origin"
    );
    let download = tempfile::tempdir_in(cache)?;
    let archive = download.path().join(&name);
    let mut file = File::create(&archive)?;
    let bytes = std::io::copy(
        &mut client
            .get(&asset.browser_download_url)
            .send()?
            .error_for_status()?
            .take(MAX_DOWNLOAD + 1),
        &mut file,
    )?;
    ensure!(
        bytes == asset.size && bytes <= MAX_DOWNLOAD,
        "Incomplete or oversized archive"
    );
    file.sync_all()?;
    drop(file);
    validate_digest(
        &archive,
        asset
            .digest
            .as_deref()
            .context("Release asset lacks digest")?,
    )?;
    self_update::verify_signature(&archive, &[key()?])?;
    let staged = tempfile::tempdir_in(cache)?;
    let mut files = vec![executable("mousa"), executable("mousa-launcher")];
    if cfg!(windows) {
        files.extend(["libmpv-2.dll".into(), "yt-dlp.exe".into()]);
    }
    for name in &files {
        self_update::Extract::from_source(&archive).extract_file(staged.path(), name)?;
        let path = staged.path().join(name);
        ensure!(
            fs::symlink_metadata(&path)?.file_type().is_file(),
            "Release entry is not a regular file"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        }
        OpenOptions::new().write(true).open(path)?.sync_all()?;
    }
    probe(&staged.path().join(executable("mousa")), &version)?;
    probe(&staged.path().join(executable("mousa-launcher")), &version)?;
    let destination = cache.join(&version);
    // Never overwrite a running or previously confirmed version directory.
    if destination.exists() {
        probe(&destination.join(executable("mousa")), &version)?;
    } else {
        fs::rename(staged.path(), &destination)?;
    }
    log(cache, &format!("Verified and staged release {version}"));
    write_json(&cache.join("staged.json"), &version)?;
    Ok(Some(version))
}

fn activate(cache: &Path, state: &mut State, version: String) -> Result<()> {
    let mut next = state.clone();
    next.previous = next.active.take();
    next.active = Some(version.clone());
    next.pending = true;
    // The library handles Windows executable renaming and delayed cleanup.
    self_replace::self_replace(cache.join(&version).join(executable("mousa-launcher")))?;
    write_json(&cache.join("state.json"), &next)?;
    *state = next;
    let _ = fs::remove_file(cache.join("staged.json"));
    log(
        cache,
        &format!("Activated {version}; awaiting successful GUI startup"),
    );
    Ok(())
}

fn rollback(cache: &Path, state: &mut State) -> Result<()> {
    if let Some(version) = state.active.take() {
        state.rejected.push(version);
    }
    state.active = state.previous.take();
    state.pending = false;
    write_json(&cache.join("state.json"), state)?;
    log(
        cache,
        "Startup failed; restored previous application version",
    );
    Ok(())
}

fn launch(
    root: &Path,
    cache: &Path,
    state: &mut State,
    args: &[std::ffi::OsString],
) -> Result<bool> {
    let health_dir = tempfile::tempdir_in(cache)?;
    let health = health_dir.path().join("ready");
    let app = app_path(root, cache, &state.active);
    let spawned = command(&app)
        .args(args)
        .env("MOUSA_SUPERVISED", "1")
        .env("MOUSA_HEALTH_FILE", &health)
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(_) if state.pending => {
            rollback(cache, state)?;
            return Ok(false);
        }
        Err(error) => return Err(error.into()),
    };
    loop {
        if state.pending && health.exists() {
            state.pending = false;
            write_json(&cache.join("state.json"), state)?;
            log(cache, "Updated application reached healthy GUI startup");
        }
        if child.try_wait()?.is_some() {
            if state.pending {
                if !health.exists() {
                    rollback(cache, state)?;
                    return Ok(false);
                }
                state.pending = false;
                write_json(&cache.join("state.json"), state)?;
                log(cache, "Updated application reached healthy GUI startup");
            }
            return Ok(true);
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn run() -> Result<()> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let root = exe.parent().context("No install directory")?;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--version") {
        println!("Mousa {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "--register") {
        probe(&root.join(executable("mousa")), env!("CARGO_PKG_VERSION"))?;
        write_json(
            &root.join("mousa-managed.json"),
            &Receipt {
                protocol: 1,
                version: env!("CARGO_PKG_VERSION").into(),
                installation_id: rand::random(),
            },
        )?;
        return Ok(());
    }
    // Only an explicit installer registration permits automatic writes.
    let receipt: Receipt = serde_json::from_reader(File::open(root.join("mousa-managed.json"))?)?;
    ensure!(receipt.protocol == 1, "Unsupported installation protocol");
    valid_version(&receipt.version)?;
    let cache = root.join(".mousa-updates");
    fs::create_dir_all(&cache)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(cache.join("lock"))?;
    if lock.try_lock_exclusive().is_err() {
        ensure!(
            !args
                .first()
                .is_some_and(|a| a == "--update-now" || a == "--repair"),
            "Mousa is running; the background updater owns this installation"
        );
        return Ok(());
    }
    let mut state = read_state(&cache)?;
    if state.installation_id != receipt.installation_id {
        // A fresh installer run takes precedence over an older cached release,
        // even if the old supervisor finishes writing state after installation.
        state = State {
            installation_id: receipt.installation_id,
            ..State::default()
        };
        write_json(&cache.join("state.json"), &state)?;
    }
    let manual = args
        .first()
        .is_some_and(|a| a == "--update-now" || a == "--repair");
    let repair = args.first().is_some_and(|a| a == "--repair");
    if !manual
        && !state.pending
        && let Ok(data) = fs::read(cache.join("staged.json"))
    {
        let staged: String = serde_json::from_slice(&data)?;
        let version = valid_version(&staged)?;
        let current = valid_version(state.active.as_deref().unwrap_or(&receipt.version))?;
        if version > current && !state.rejected.contains(&staged) {
            activate(&cache, &mut state, staged)?;
        }
    }
    let current = state.active.clone().unwrap_or(receipt.version);
    if manual {
        match stage(&cache, &current, &state.rejected, repair) {
            Ok(Some(version)) => activate(&cache, &mut state, version)?,
            Ok(None) => log(&cache, "No newer stable release available"),
            Err(error) => {
                log(&cache, &format!("Update failed: {error:#}"));
                return Err(error);
            }
        }
        return Ok(());
    }
    let (tx, rx) = mpsc::channel();
    let update_cache = cache.clone();
    let rejected = state.rejected.clone();
    thread::spawn(move || {
        loop {
            // Persist the attempt time, including failures, to avoid API storms on relaunch.
            let last: u64 = fs::read(update_cache.join("checked.json"))
                .ok()
                .and_then(|v| serde_json::from_slice(&v).ok())
                .unwrap_or(0);
            if last <= now() && now() - last < INTERVAL {
                thread::sleep(Duration::from_secs(60));
                continue;
            }
            thread::sleep(Duration::from_secs(10 + rand::random_range(0..20)));
            let _ = write_json(&update_cache.join("checked.json"), &now());
            match stage(&update_cache, &current, &rejected, false) {
                Ok(Some(version)) => {
                    let _ = tx.send(version);
                    return;
                }
                Ok(None) => {}
                Err(error) => log(
                    &update_cache,
                    &format!("Background update failed: {error:#}"),
                ),
            }
        }
    });
    if !launch(root, &cache, &mut state, &args)? {
        launch(root, &cache, &mut state, &args)?;
    }
    // A partial download never delays closing the app or becomes active.
    if let Ok(version) = rx.try_recv()
        && !state.rejected.contains(&version)
        && let Err(error) = activate(&cache, &mut state, version)
    {
        log(&cache, &format!("Activation deferred: {error:#}"));
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Mousa launcher: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_releases_only_and_never_downgrade() {
        let mut r = Release {
            tag_name: "v0.2.0".into(),
            draft: false,
            prerelease: false,
            assets: vec![],
        };
        let current = Version::new(0, 1, 0);
        assert!(release_version(&r, &current, false, &[]).unwrap().is_some());
        r.draft = true;
        assert!(release_version(&r, &current, false, &[]).unwrap().is_none());
        r.draft = false;
        r.prerelease = true;
        assert!(release_version(&r, &current, false, &[]).unwrap().is_none());
        r.prerelease = false;
        r.tag_name = "v0.1.0".into();
        assert!(release_version(&r, &current, false, &[]).unwrap().is_none());
        assert!(release_version(&r, &current, true, &[]).unwrap().is_some());
        r.tag_name = "v0.0.1".into();
        assert!(release_version(&r, &current, true, &[]).unwrap().is_none());
        r.tag_name = "v0.2.0".into();
        assert!(
            release_version(&r, &current, false, &["0.2.0".into()])
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn rejects_path_and_unstable_versions() {
        for value in [
            "../escape",
            "0.1.0/../../",
            "0.1.0-beta",
            "0.1.0+build",
            "v0.1.0",
        ] {
            assert!(valid_version(value).is_err());
        }
    }

    #[test]
    fn checksum_mismatch_and_missing_signature_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("release.tar.gz");
        fs::write(&file, b"tampered").unwrap();
        assert!(validate_digest(&file, &format!("sha256:{}", "0".repeat(64))).is_err());
        assert!(self_update::verify_signature(&file, &[key().unwrap()]).is_err());
    }

    #[test]
    fn rollback_restores_previous_and_quarantines_failed_release() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State {
            installation_id: 0,
            active: Some("0.2.0".into()),
            previous: Some("0.1.0".into()),
            pending: true,
            rejected: vec![],
        };
        rollback(dir.path(), &mut state).unwrap();
        let stored = read_state(dir.path()).unwrap();
        assert_eq!(stored.active.as_deref(), Some("0.1.0"));
        assert!(!stored.pending);
        assert_eq!(stored.rejected, ["0.2.0"]);
    }

    #[test]
    fn state_replacement_is_atomic_and_validates_paths() {
        let dir = tempfile::tempdir().unwrap();
        write_json(&dir.path().join("state.json"), &State::default()).unwrap();
        write_json(
            &dir.path().join("state.json"),
            &State {
                active: Some("../outside".into()),
                ..State::default()
            },
        )
        .unwrap();
        assert!(read_state(dir.path()).is_err());
    }

    #[test]
    fn signed_archive_roundtrip_and_wrong_key_rejection() {
        use std::io::Cursor;
        let dir = tempfile::tempdir().unwrap();
        let name = "mousa-0.1.0-test.tar.gz";
        let archive = dir.path().join(name);
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(7);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, "./mousa", &b"payload"[..])
            .unwrap();
        let data = tar.into_inner().unwrap().finish().unwrap();
        let signing = zipsign_api::SigningKey::from_bytes(&[42; 32]);
        let public = signing.verifying_key().to_bytes();
        let mut signed = File::create(&archive).unwrap();
        zipsign_api::sign::copy_and_sign_tar(
            &mut Cursor::new(data),
            &mut signed,
            &[signing],
            Some(name.as_bytes()),
        )
        .unwrap();
        drop(signed);
        self_update::verify_signature(&archive, &[public]).unwrap();
        assert!(self_update::verify_signature(&archive, &[key().unwrap()]).is_err());
        let extracted = dir.path().join("extracted");
        fs::create_dir(&extracted).unwrap();
        self_update::Extract::from_source(&archive)
            .extract_file(&extracted, "mousa")
            .unwrap();
        assert_eq!(fs::read(extracted.join("mousa")).unwrap(), b"payload");
        let renamed = dir.path().join("other.tar.gz");
        fs::rename(archive, &renamed).unwrap();
        assert!(self_update::verify_signature(&renamed, &[public]).is_err());
    }

    #[test]
    #[ignore = "subprocess fixture invoked by lifecycle test"]
    fn child_ready() {
        let path = std::env::var_os("MOUSA_HEALTH_FILE").unwrap();
        fs::write(path, b"ready").unwrap();
    }

    #[test]
    #[ignore = "subprocess fixture invoked by lifecycle test"]
    fn child_fails() {
        std::process::exit(1);
    }

    #[test]
    fn actual_child_startup_confirms_or_rolls_back() {
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("cache");
        fs::create_dir_all(cache.join("0.2.0")).unwrap();
        fs::copy(
            std::env::current_exe().unwrap(),
            cache.join("0.2.0").join(executable("mousa")),
        )
        .unwrap();
        let mut state = State {
            installation_id: 0,
            active: Some("0.2.0".into()),
            previous: None,
            pending: true,
            rejected: vec![],
        };
        let ready = ["--exact", "tests::child_ready", "--ignored"].map(Into::into);
        assert!(launch(root.path(), &cache, &mut state, &ready).unwrap());
        assert!(!state.pending);
        state.pending = true;
        let failed = ["--exact", "tests::child_fails", "--ignored"].map(Into::into);
        assert!(!launch(root.path(), &cache, &mut state, &failed).unwrap());
        assert!(state.active.is_none());
        assert_eq!(state.rejected, ["0.2.0"]);
    }
}
