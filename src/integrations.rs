use crate::db::{self, Track};
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::blocking::Client;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub fn ytdlp_path() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let bundled = dir.join("yt-dlp.exe");
        if bundled.is_file() {
            return bundled;
        }
    }
    "yt-dlp".into()
}

fn client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(concat!("Mousa/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()?)
}
// Startup can race network/DNS setup. Retry only transport failures, never
// rejected credentials, and keep the waits on the existing background worker.
const GOOGLE_RETRY_DELAYS: [u64; 5] = [2, 4, 8, 16, 30];

#[derive(Debug)]
enum RetryError<E> {
    Terminal(E),
    Exhausted { error: E, attempts: usize },
}

fn retry_with_delays<T, E>(
    mut request: impl FnMut() -> std::result::Result<T, E>,
    mut retryable: impl FnMut(&E) -> bool,
    mut wait: impl FnMut(Duration),
    delays: impl IntoIterator<Item = Duration>,
) -> std::result::Result<T, RetryError<E>> {
    let mut attempts = 0;
    let mut delays = delays.into_iter();
    loop {
        attempts += 1;
        match request() {
            Ok(value) => return Ok(value),
            Err(error) if retryable(&error) => {
                if let Some(delay) = delays.next() {
                    wait(delay);
                } else {
                    return Err(RetryError::Exhausted { error, attempts });
                }
            }
            Err(error) => return Err(RetryError::Terminal(error)),
        }
    }
}

fn retry_google_token<T>(
    request: impl FnMut() -> std::result::Result<T, reqwest::Error>,
    wait: impl FnMut(Duration),
) -> Result<T> {
    match retry_with_delays(
        request,
        |error| error.is_connect() || error.is_timeout(),
        wait,
        GOOGLE_RETRY_DELAYS.map(Duration::from_secs),
    ) {
        Ok(value) => Ok(value),
        Err(RetryError::Terminal(error)) => Err(error.into()),
        Err(RetryError::Exhausted { error, attempts }) => Err(error).context(format!(
            "Google connection failed after {attempts} attempts"
        )),
    }
}

fn secret(name: &str) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new("app.mousa", name)?)
}
pub fn store_secret(name: &str, value: &str) -> Result<()> {
    secret(name)?.set_password(value)?;
    Ok(())
}
pub fn get_secret(name: &str) -> Result<String> {
    Ok(secret(name)?.get_password()?)
}
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
pub fn import_url(input: &str) -> Result<Vec<Track>> {
    let url = url::Url::parse(input.trim()).context("Paste a YouTube video or playlist URL")?;
    anyhow::ensure!(
        matches!(url.scheme(), "https" | "http")
            && matches!(
                url.host_str(),
                Some("youtube.com" | "www.youtube.com" | "music.youtube.com" | "youtu.be")
            ),
        "Only YouTube URLs are supported"
    );
    let mut command = std::process::Command::new(ytdlp_path());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .args([
            "--flat-playlist",
            "--dump-single-json",
            "--skip-download",
            "--no-warnings",
            "--socket-timeout",
            "20",
            "--",
            url.as_str(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("Install yt-dlp")?;
    // Drain pipes concurrently to avoid blocking on large playlist JSON.
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut b = Vec::new();
        out.read_to_end(&mut b).map(|_| b)
    });
    let err = std::thread::spawn(move || {
        let mut b = String::new();
        err.read_to_string(&mut b).map(|_| b)
    });
    let deadline = Instant::now() + Duration::from_secs(180);
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if Instant::now() > deadline {
            child.kill()?;
            child.wait()?;
            bail!("YouTube import timed out; try again")
        };
        std::thread::sleep(Duration::from_millis(100));
    };
    let bytes = out
        .join()
        .map_err(|_| anyhow::anyhow!("Import worker stopped"))??;
    let error = err
        .join()
        .map_err(|_| anyhow::anyhow!("Import worker stopped"))??;
    anyhow::ensure!(
        status.success(),
        "YouTube import failed: {}",
        error.chars().take(500).collect::<String>()
    );
    let v: Value = serde_json::from_slice(&bytes)?;
    let items = v["entries"].as_array().cloned().unwrap_or_else(|| vec![v]);
    Ok(items
        .into_iter()
        .filter_map(|v| {
            let id = v["id"].as_str()?;
            Some(Track {
                id: 0,
                playlist_index: 0,
                unavailable: crate::db::unavailable_status(
                    v["title"].as_str(),
                    v["availability"].as_str(),
                ),
                url: format!("https://www.youtube.com/watch?v={id}"),
                title: crate::titles::complete(
                    v["title"].as_str().unwrap_or("Unavailable track"),
                    v["track"].as_str(),
                )
                .to_owned(),
                artist: crate::artists::resolve(&v),
                duration: v["duration"].as_f64().unwrap_or(0.),
            })
        })
        .collect())
}
fn google_client_at(directory: &std::path::Path) -> Result<Value> {
    let path = directory.join("google-client.json");
    let read = std::fs::read(&path);
    let diagnostic = format!(
        "time={} pid={} path={:?} cwd={:?} exe={:?} result={:?}\n",
        now(),
        std::process::id(),
        path,
        std::env::current_dir(),
        std::env::current_exe(),
        read.as_ref()
            .map(|v| v.len())
            .map_err(|e| (e.kind(), e.raw_os_error()))
    );
    let _ = std::fs::write(directory.join("configuration-read.log"), diagnostic);
    let v: Value = serde_json::from_slice(
        &read
            .with_context(|| format!("Google configuration is unavailable. Save Google Desktop OAuth JSON to {}. If MOUSA_DATA_DIR is set, check that it points to your library", path.display()))?,
    ).with_context(|| format!("Invalid Google Desktop OAuth JSON in {}", path.display()))?;
    anyhow::ensure!(
        v["installed"]["client_id"]
            .as_str()
            .is_some_and(|id| !id.trim().is_empty()),
        "Expected Google Desktop OAuth client JSON in {}",
        path.display()
    );
    Ok(v)
}
pub fn google_login() -> Result<String> {
    let v = google_client_at(&db::data_dir())?;
    let cfg = &v["installed"];
    let id = cfg["client_id"]
        .as_str()
        .context("Expected Desktop OAuth client JSON")?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let redirect = format!("http://127.0.0.1:{}/", listener.local_addr()?.port());
    let verifier = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
    let state = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut url = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", id),
        ("redirect_uri", &redirect),
        ("response_type", "code"),
        (
            "scope",
            "openid profile https://www.googleapis.com/auth/youtube",
        ),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("state", &state),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
    ]);
    webbrowser::open(url.as_str())?;
    let deadline = Instant::now() + Duration::from_secs(180);
    let code = loop {
        anyhow::ensure!(Instant::now() < deadline, "Google sign-in timed out");
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_secs(3)))?;
                let mut buf = [0; 8192];
                let n = stream.read(&mut buf)?;
                let request = String::from_utf8_lossy(&buf[..n]);
                let target = request.split_whitespace().nth(1).unwrap_or("/");
                let u = url::Url::parse(&format!("http://localhost{target}"))?;
                let q: BTreeMap<_, _> = u.query_pairs().into_owned().collect();
                if q.get("state") != Some(&state) {
                    let _ = stream.write_all(
                        b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nInvalid state",
                    );
                    continue;
                }
                let _=stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nReturn to Mousa to finish signing in.");
                if let Some(e) = q.get("error") {
                    bail!("Google authorization: {e}")
                };
                break q.get("code").context("Missing authorization code")?.clone();
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(e) => return Err(e.into()),
        }
    };
    let token: Value = client()?
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", id),
            ("client_secret", cfg["client_secret"].as_str().unwrap_or("")),
            ("code", &code),
            ("code_verifier", &verifier),
            ("redirect_uri", &redirect),
            ("grant_type", "authorization_code"),
        ])
        .send()?
        .error_for_status()?
        .json()?;
    anyhow::ensure!(
        token["scope"]
            .as_str()
            .unwrap_or("")
            .split_whitespace()
            .any(|scope| scope == "https://www.googleapis.com/auth/youtube"),
        "Google playlist editing permission was not granted. Existing sign-in was preserved."
    );
    let profile: Value = client()?
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(
            token["access_token"]
                .as_str()
                .context("Missing access token")?,
        )
        .send()?
        .error_for_status()?
        .json()?;
    let name = profile["name"]
        .as_str()
        .filter(|n| !n.trim().is_empty())
        .context("Google did not return your full name. Please grant profile access.")?;
    store_secret("google-name", name)?;
    store_secret(
        "google-refresh",
        token["refresh_token"]
            .as_str()
            .context("Google did not provide a refresh token")?,
    )?;
    Ok(name.to_owned())
}
pub(crate) fn google_access() -> Result<String> {
    let v = google_client_at(&db::data_dir())?;
    let refresh = get_secret("google-refresh").context("Sign in to Google first")?;
    let client = client()?;
    let t: Value = retry_google_token(
        || {
            client
                .post("https://oauth2.googleapis.com/token")
                .form(&[
                    (
                        "client_id",
                        v["installed"]["client_id"].as_str().unwrap_or(""),
                    ),
                    (
                        "client_secret",
                        v["installed"]["client_secret"].as_str().unwrap_or(""),
                    ),
                    ("refresh_token", &refresh),
                    ("grant_type", "refresh_token"),
                ])
                .send()?
                .error_for_status()?
                .json()
        },
        std::thread::sleep,
    )?;
    Ok(t["access_token"]
        .as_str()
        .context("Missing access token")?
        .into())
}
/// App credentials belong to the distributor, never to the connection form.
fn lastfm_credentials() -> Result<(String, String)> {
    let path = db::data_dir().join("lastfm-client.json");
    if path.exists() {
        let config: Value = serde_json::from_slice(&std::fs::read(path)?)?;
        return Ok((
            config["api_key"]
                .as_str()
                .filter(|s| !s.is_empty())
                .context("Missing Last.fm application key")?
                .into(),
            config["shared_secret"]
                .as_str()
                .filter(|s| !s.is_empty())
                .context("Missing Last.fm application secret")?
                .into(),
        ));
    }
    // Preserve installations configured before browser sign-in was introduced.
    Ok((get_secret("lastfm-key").context("Last.fm is not configured for this installation. Add lastfm-client.json with the application's api_key and shared_secret.")?,
        get_secret("lastfm-secret")?))
}
fn lastfm_response(mut p: BTreeMap<String, String>) -> Result<Value> {
    let (key, secret) = lastfm_credentials()?;
    p.insert("api_key".into(), key);
    let signature = lastfm_signature(&p, &secret);
    p.insert("api_sig".into(), signature);
    p.insert("format".into(), "json".into());
    let response = client()?
        .post("https://ws.audioscrobbler.com/2.0/")
        .form(&p)
        .send()?;
    let status = response.status();
    let value: Value = response.json()?;
    lastfm_payload(status, value)
}
fn lastfm_payload(status: reqwest::StatusCode, value: Value) -> Result<Value> {
    // Last.fm uses HTTP 403 for error 14 while browser approval is pending.
    // Preserve API errors so the desktop flow can distinguish waiting from failure.
    anyhow::ensure!(
        status.is_success() || value.get("error").is_some(),
        "Last.fm request failed ({status})"
    );
    Ok(value)
}
fn lastfm_signature(p: &BTreeMap<String, String>, secret: &str) -> String {
    let mut value = p
        .iter()
        .filter(|(k, _)| !matches!(k.as_str(), "format" | "callback" | "api_sig"))
        .map(|(k, v)| format!("{k}{v}"))
        .collect::<String>();
    value.push_str(secret);
    format!("{:x}", md5::compute(value))
}
fn check_lastfm(v: Value) -> Result<Value> {
    anyhow::ensure!(
        v.get("error").is_none(),
        "Last.fm: {}",
        v["message"].as_str().unwrap_or("Request failed")
    );
    Ok(v)
}
fn lastfm(p: BTreeMap<String, String>) -> Result<Value> {
    check_lastfm(lastfm_response(p)?)
}
pub fn lastfm_login() -> Result<String> {
    let v = lastfm(BTreeMap::from([("method".into(), "auth.getToken".into())]))?;
    let token = v["token"].as_str().context("Missing Last.fm token")?;
    let mut url = url::Url::parse("https://www.last.fm/api/auth/")?;
    url.query_pairs_mut().extend_pairs([
        ("api_key", lastfm_credentials()?.0.as_str()),
        ("token", token),
    ]);
    webbrowser::open(url.as_str())?;
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        anyhow::ensure!(
            Instant::now() < deadline,
            "Last.fm sign-in timed out. Try connecting again."
        );
        let v = lastfm_response(BTreeMap::from([
            ("method".into(), "auth.getSession".into()),
            ("token".into(), token.into()),
        ]))?;
        if v["error"].as_u64() == Some(14) {
            std::thread::sleep(Duration::from_secs(2));
            continue;
        }
        let v = check_lastfm(v)?;
        let name = v["session"]["name"]
            .as_str()
            .context("Missing Last.fm username")?;
        let key = v["session"]["key"]
            .as_str()
            .context("Missing Last.fm session")?;
        let _guard = LASTFM_LOCK.lock().unwrap();
        db::library()?.conn.execute("DELETE FROM scrobbles", [])?;
        store_secret("lastfm-name", name)?;
        store_secret("lastfm-session", key)?;
        return Ok(name.into());
    }
}
fn delete_secret(name: &str) -> Result<()> {
    match secret(name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
pub fn google_logout() -> Result<()> {
    delete_secret("google-name")?;
    delete_secret("google-refresh")
}
static LASTFM_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub fn lastfm_logout() -> Result<()> {
    let _guard = LASTFM_LOCK.lock().unwrap();
    db::library()?.conn.execute("DELETE FROM scrobbles", [])?;
    delete_secret("lastfm-name")?;
    delete_secret("lastfm-token")?;
    delete_secret("lastfm-session")
}
pub fn queue_scrobble(t: &Track, started: i64) -> Result<()> {
    let _guard = LASTFM_LOCK.lock().unwrap();
    if t.artist.is_empty() || get_secret("lastfm-session").is_err() {
        return Ok(());
    }
    db::library()?.conn.execute(
        "INSERT INTO scrobbles(artist,track,started) VALUES(?,?,?)",
        rusqlite::params![t.artist, t.title, started],
    )?;
    Ok(())
}
pub fn lastfm_track(t: &Track, started: Option<i64>) -> Result<()> {
    let _guard = LASTFM_LOCK.lock().unwrap();
    lastfm_track_inner(t, started)
}
fn lastfm_track_inner(t: &Track, started: Option<i64>) -> Result<()> {
    let mut p = BTreeMap::from([
        (
            "method".into(),
            if started.is_some() {
                "track.scrobble"
            } else {
                "track.updateNowPlaying"
            }
            .into(),
        ),
        ("sk".into(), get_secret("lastfm-session")?),
        ("artist".into(), t.artist.clone()),
        ("track".into(), t.title.clone()),
    ]);
    if let Some(s) = started {
        p.insert("timestamp".into(), s.to_string());
    }
    lastfm(p)?;
    Ok(())
}
pub fn flush_scrobbles() -> Result<()> {
    static SYNC: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let Ok(_guard) = SYNC.try_lock() else {
        return Ok(());
    };
    let db = db::library()?;
    let rows: Vec<(i64, String, String, i64)> = db
        .conn
        .prepare("SELECT id,artist,track,started FROM scrobbles ORDER BY id LIMIT 50")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (id, artist, title, started) in rows {
        let _guard = LASTFM_LOCK.lock().unwrap();
        // A disconnect may have cleared this batch while a previous request ran.
        let pending: bool = db.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM scrobbles WHERE id=?)",
            [id],
            |r| r.get(0),
        )?;
        if !pending {
            continue;
        }
        lastfm_track_inner(
            &Track {
                id: 0,
                playlist_index: 0,
                unavailable: None,
                url: String::new(),
                title,
                artist,
                duration: 0.,
            },
            Some(started),
        )?;
        db.conn.execute("DELETE FROM scrobbles WHERE id=?", [id])?;
    }
    Ok(())
}

#[cfg(test)]
mod auth_tests {
    #[test]
    fn google_configuration_errors_identify_the_file_and_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("google-client.json");
        let error = super::google_client_at(dir.path()).unwrap_err().to_string();
        assert!(error.contains(&path.display().to_string()));
        assert!(error.contains("MOUSA_DATA_DIR"));
        std::fs::write(&path, br#"{"web":{"client_id":"wrong-type"}}"#).unwrap();
        assert!(
            super::google_client_at(dir.path())
                .unwrap_err()
                .to_string()
                .contains("Expected Google Desktop OAuth")
        );
        std::fs::write(&path, br#"{"installed":{"client_id":"desktop-client"}}"#).unwrap();
        assert_eq!(
            super::google_client_at(dir.path()).unwrap()["installed"]["client_id"],
            "desktop-client"
        );
    }

    use super::*;
    #[test]
    fn google_token_recovers_without_waiting_again_after_success() {
        let mut attempts = 0;
        let mut waits = Vec::new();
        let value = retry_with_delays(
            || {
                attempts += 1;
                if attempts < 3 {
                    Err("transport")
                } else {
                    Ok("token")
                }
            },
            |_| true,
            |delay| waits.push(delay),
            GOOGLE_RETRY_DELAYS.map(Duration::from_secs),
        )
        .unwrap();
        assert_eq!(value, "token");
        assert_eq!(attempts, 3);
        assert_eq!(waits, vec![Duration::from_secs(2), Duration::from_secs(4)]);
    }

    #[test]
    fn google_token_reports_failure_after_all_retries() {
        let mut attempts = 0;
        let mut waits = Vec::new();
        let error = retry_with_delays::<(), _>(
            || {
                attempts += 1;
                Err("transport")
            },
            |_| true,
            |delay| waits.push(delay),
            GOOGLE_RETRY_DELAYS.map(Duration::from_secs),
        )
        .unwrap_err();
        assert_eq!(attempts, 6);
        assert_eq!(waits, GOOGLE_RETRY_DELAYS.map(Duration::from_secs));
        assert!(matches!(
            error,
            RetryError::Exhausted {
                error: "transport",
                attempts: 6
            }
        ));
    }

    #[test]
    fn google_token_does_not_retry_http_auth_errors() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            stream.read(&mut buffer).unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let error = Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("http://{address}"))
            .send()
            .unwrap()
            .error_for_status()
            .unwrap_err();
        server.join().unwrap();
        let mut error = Some(error);
        assert!(
            retry_google_token::<()>(|| Err(error.take().unwrap()), |_| panic!("must not retry"))
                .is_err()
        );
    }

    #[test]
    fn desktop_signature_sorts_parameters_and_excludes_transport_fields() {
        let mut p = BTreeMap::from([
            ("token".into(), "token-value".into()),
            ("method".into(), "auth.getSession".into()),
            ("api_key".into(), "app-key".into()),
        ]);
        let expected = format!(
            "{:x}",
            md5::compute("api_keyapp-keymethodauth.getSessiontokentoken-valuesecret")
        );
        assert_eq!(lastfm_signature(&p, "secret"), expected);
        p.insert("format".into(), "json".into());
        p.insert("callback".into(), "callback".into());
        p.insert("api_sig".into(), "old-signature".into());
        assert_eq!(lastfm_signature(&p, "secret"), expected);
    }
    #[test]
    fn pending_authorization_survives_http_forbidden() {
        let pending = serde_json::json!({"error":14,"message":"Unauthorized Token"});
        let value = lastfm_payload(reqwest::StatusCode::FORBIDDEN, pending).unwrap();
        assert_eq!(value["error"].as_u64(), Some(14));
        assert!(check_lastfm(value).is_err());
        assert!(
            lastfm_payload(
                reqwest::StatusCode::BAD_GATEWAY,
                serde_json::json!({"message":"Gateway failed"})
            )
            .is_err()
        );
    }
    #[test]
    fn api_failures_are_not_successful_sessions() {
        for code in [4, 9, 10, 14, 15, 26, 29] {
            let error =
                check_lastfm(serde_json::json!({"error":code,"message":"Rejected"})).unwrap_err();
            assert_eq!(error.to_string(), "Last.fm: Rejected");
        }
        assert!(
            check_lastfm(serde_json::json!({"session":{"name":"listener","key":"session"}}))
                .is_ok()
        );
    }
}
