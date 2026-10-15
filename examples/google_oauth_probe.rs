//! Interactive OAuth smoke test; credentials remain in the operating system keyring.
use anyhow::Context;
use serde_json::Value;

fn main() -> anyhow::Result<()> {
    if !std::env::args().any(|arg| arg == "--check-existing") {
        mousa::integrations::google_login()?;
    }
    let refresh = mousa::integrations::get_secret("google-refresh")?;
    anyhow::ensure!(!refresh.is_empty(), "Refresh token was not saved");
    let config: Value = serde_json::from_slice(&std::fs::read(
        mousa::db::data_dir().join("google-client.json"),
    )?)?;
    let cfg = &config["installed"];
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let token: Value = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            (
                "client_id",
                cfg["client_id"].as_str().context("Missing client ID")?,
            ),
            (
                "client_secret",
                cfg["client_secret"]
                    .as_str()
                    .context("Missing client secret")?,
            ),
            ("refresh_token", refresh.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()?
        .error_for_status()?
        .json()?;
    if std::env::args().any(|arg| arg == "--require-editing") {
        anyhow::ensure!(
            token["scope"]
                .as_str()
                .unwrap_or("")
                .split_whitespace()
                .any(|scope| scope == "https://www.googleapis.com/auth/youtube"),
            "Reconnect Google to grant playlist editing permission"
        );
    }
    client
        .get("https://www.googleapis.com/youtube/v3/playlists")
        .bearer_auth(
            token["access_token"]
                .as_str()
                .context("Missing access token")?,
        )
        .query(&[("part", "id"), ("mine", "true"), ("maxResults", "1")])
        .send()?
        .error_for_status()?;
    println!(
        "Google token refresh and YouTube playlist API access succeeded. Credentials remain in the system keyring."
    );
    Ok(())
}
