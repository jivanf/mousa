//! Interactive desktop-auth check; does not submit listening history.
use anyhow::{Context, ensure};
use mousa::integrations;
use serde_json::Value;
use std::collections::BTreeMap;

fn main() -> anyhow::Result<()> {
    let name = integrations::lastfm_login()?;
    let config: Value = serde_json::from_slice(&std::fs::read(
        mousa::db::data_dir().join("lastfm-client.json"),
    )?)?;
    let mut params = BTreeMap::from([
        (
            "api_key",
            config["api_key"]
                .as_str()
                .context("Missing application key")?
                .to_owned(),
        ),
        ("method", "user.getInfo".to_owned()),
        ("sk", integrations::get_secret("lastfm-session")?),
    ]);
    let signature = params
        .iter()
        .map(|(k, v)| format!("{k}{v}"))
        .collect::<String>()
        + config["shared_secret"]
            .as_str()
            .context("Missing application secret")?;
    params.insert("api_sig", format!("{:x}", md5::compute(signature)));
    params.insert("format", "json".to_owned());
    let response: Value = reqwest::blocking::Client::builder()
        .user_agent(concat!("Mousa/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()?
        .post("https://ws.audioscrobbler.com/2.0/")
        .form(&params)
        .send()?
        .error_for_status()?
        .json()?;
    ensure!(
        response.get("error").is_none(),
        "Last.fm verification failed"
    );
    ensure!(
        response["user"]["name"].as_str() == Some(name.as_str()),
        "Session identity mismatch"
    );
    ensure!(
        integrations::get_secret("lastfm-name")? == name,
        "Saved username mismatch"
    );
    println!("Desktop authentication and authenticated profile request passed for {name}.");
    Ok(())
}
