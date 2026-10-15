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

fn client() -> Result<Client> {
    Ok(Client::builder().timeout(Duration::from_secs(30)).build()?)
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
    let mut child = std::process::Command::new("yt-dlp")
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
                url: format!("https://www.youtube.com/watch?v={id}"),
                title: v["title"]
                    .as_str()
                    .unwrap_or("Unavailable track")
                    .to_owned(),
                artist: v["artist"]
                    .as_str()
                    .or(v["uploader"].as_str())
                    .or(v["channel"].as_str())
                    .unwrap_or("")
                    .trim_end_matches(" - Topic")
                    .to_owned(),
                duration: v["duration"].as_f64().unwrap_or(0.),
            })
        })
        .collect())
}
pub fn google_login() -> Result<()> {
    let path = db::data_dir().join("google-client.json");
    let v: Value = serde_json::from_slice(
        &std::fs::read(&path)
            .with_context(|| format!("Save Google Desktop OAuth JSON to {}", path.display()))?,
    )?;
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
        ("scope", "https://www.googleapis.com/auth/youtube"),
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
    store_secret(
        "google-refresh",
        token["refresh_token"]
            .as_str()
            .context("Google did not provide a refresh token")?,
    )?;
    Ok(())
}
pub(crate) fn google_access() -> Result<String> {
    let v: Value =
        serde_json::from_slice(&std::fs::read(db::data_dir().join("google-client.json"))?)?;
    let refresh = get_secret("google-refresh").context("Sign in to Google first")?;
    let t: Value = client()?
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
        .json()?;
    Ok(t["access_token"]
        .as_str()
        .context("Missing access token")?
        .into())
}
fn lastfm(mut p: BTreeMap<String, String>) -> Result<Value> {
    let key = get_secret("lastfm-key").context("Set Last.fm API key and secret in Settings")?;
    let secret = get_secret("lastfm-secret")?;
    p.insert("api_key".into(), key);
    let mut signature = p.iter().map(|(k, v)| format!("{k}{v}")).collect::<String>();
    signature.push_str(&secret);
    p.insert("api_sig".into(), format!("{:x}", md5::compute(signature)));
    p.insert("format".into(), "json".into());
    let v: Value = client()?
        .post("https://ws.audioscrobbler.com/2.0/")
        .form(&p)
        .send()?
        .error_for_status()?
        .json()?;
    anyhow::ensure!(
        v.get("error").is_none(),
        "Last.fm: {}",
        v["message"].as_str().unwrap_or("Request failed")
    );
    Ok(v)
}
pub fn lastfm_begin() -> Result<()> {
    let v = lastfm(BTreeMap::from([("method".into(), "auth.getToken".into())]))?;
    let token = v["token"].as_str().context("Missing Last.fm token")?;
    store_secret("lastfm-token", token)?;
    let mut u = url::Url::parse("https://www.last.fm/api/auth/")?;
    u.query_pairs_mut().extend_pairs([
        ("api_key", get_secret("lastfm-key")?.as_str()),
        ("token", token),
    ]);
    webbrowser::open(u.as_str())?;
    Ok(())
}
pub fn lastfm_finish() -> Result<()> {
    let v = lastfm(BTreeMap::from([
        ("method".into(), "auth.getSession".into()),
        ("token".into(), get_secret("lastfm-token")?),
    ]))?;
    store_secret(
        "lastfm-session",
        v["session"]["key"]
            .as_str()
            .context("Approve Mousa on Last.fm first")?,
    )?;
    Ok(())
}
pub fn lastfm_track(t: &Track, started: Option<i64>) -> Result<()> {
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
        lastfm_track(
            &Track {
                id: 0,
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
