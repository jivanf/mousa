//! Public channel metadata, without downloading videos or consuming API quota.
use crate::catalog::{Destination, Item, Page};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn fetch(destination: &Destination, cancelled: &impl Fn() -> bool) -> Result<Page> {
    let id = destination.body["browseId"]
        .as_str()
        .context("Missing channel ID")?;
    if destination.body["mousaPlaylist"] == true {
        let id = id.strip_prefix("VL").context("Invalid playlist ID")?;
        ensure!(
            id.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)),
            "Invalid playlist ID"
        );
        let value = metadata(
            &format!("https://www.youtube.com/playlist?list={id}"),
            cancelled,
        )?
        .context("Playlist is unavailable")?;
        let mut page = Page::default();
        append(&mut page, &value, false);
        page.title = value["title"].as_str().unwrap_or("Playlist").into();
        page.byline = value["channel"].as_str().unwrap_or_default().into();
        for item in &mut page.items {
            item.collection_member = true;
        }
        return Ok(page);
    }
    ensure!(
        id.starts_with("UC")
            && id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)),
        "Invalid channel ID"
    );
    let mut page = Page::default();
    for tab in ["videos", "playlists"] {
        ensure!(!cancelled(), "Superseded catalog request");
        let url = format!("https://www.youtube.com/channel/{id}/{tab}");
        if let Some(value) = metadata(&url, cancelled)? {
            append(&mut page, &value, tab == "playlists");
        }
    }
    Ok(page)
}

fn metadata(url: &str, cancelled: &impl Fn() -> bool) -> Result<Option<Value>> {
    let mut command = Command::new(crate::integrations::ytdlp_path());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .args([
            "--ignore-config",
            "--flat-playlist",
            "--skip-download",
            "--dump-single-json",
            "--no-warnings",
            "--socket-timeout",
            "10",
            "--retries",
            "1",
            "--",
            url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Could not load channel: yt-dlp is unavailable")?;
    // Drain both pipes while waiting, including for channels with large catalogs.
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || read(stdout));
    let err = std::thread::spawn(move || read(stderr));
    let start = Instant::now();
    let status = loop {
        if cancelled() || start.elapsed() > Duration::from_secs(180) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = out.join();
            let _ = err.join();
            bail!("Channel loading cancelled or timed out. Please retry.");
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = out.join();
                let _ = err.join();
                return Err(error.into());
            }
        }
    };
    let output = out
        .join()
        .map_err(|_| anyhow::anyhow!("Channel reader failed"))??;
    let errors = err
        .join()
        .map_err(|_| anyhow::anyhow!("Channel reader failed"))??;
    if !status.success() {
        let error = String::from_utf8_lossy(&errors);
        // A channel may legitimately have no public uploads or playlists.
        if error.contains("does not have a videos tab")
            || error.contains("does not have a playlists tab")
        {
            return Ok(None);
        }
        bail!("Could not load channel: {}", error.trim());
    }
    Ok(Some(
        serde_json::from_slice(&output).context("Invalid channel response")?,
    ))
}

fn read(mut stream: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn append(page: &mut Page, value: &Value, playlists: bool) {
    if page.title.is_empty() {
        page.title = value["channel"].as_str().unwrap_or_default().into();
        page.thumbnail = value["thumbnails"].as_array().and_then(|images| {
            images
                .iter()
                .find(|image| image["id"] == "avatar_uncropped")
                .or_else(|| images.last())
                .and_then(|image| image["url"].as_str())
                .map(str::to_owned)
        });
        if let Some(count) = value["channel_follower_count"].as_u64() {
            page.metadata = format!("{count} subscribers");
        }
    }
    for entry in value["entries"].as_array().into_iter().flatten() {
        let (Some(id), Some(title)) = (entry["id"].as_str(), entry["title"].as_str()) else {
            continue;
        };
        let kind = if playlists { "Playlist" } else { "Video" };
        page.items.push(Item {
            title: title.into(),
            kind: kind.into(),
            category: format!("{kind}s"),
            artist: page.title.clone(),
            video: (!playlists).then(|| id.into()),
            duration: entry["duration"].as_f64().unwrap_or(0.),
            thumbnail: entry["thumbnails"]
                .as_array()
                .and_then(|images| images.last())
                .and_then(|image| image["url"].as_str())
                .map(str::to_owned),
            destination: Some(Destination {
                api: if playlists { "browse" } else { "next" }.into(),
                body: if playlists {
                    json!({"browseId":format!("VL{id}"), "mousaPlaylist":true})
                } else {
                    json!({"videoId":id})
                },
            }),
            ..Default::default()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channel_content_keeps_videos_playable_and_playlists_navigable() {
        let mut page = Page::default();
        append(
            &mut page,
            &json!({"channel":"Creator", "entries":[{"id":"video", "title":"A video", "duration":42}, null]}),
            false,
        );
        append(
            &mut page,
            &json!({"entries":[{"id":"PL123", "title":"A playlist"}]}),
            true,
        );
        assert_eq!(page.title, "Creator");
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].track().unwrap().duration, 42.);
        assert_eq!(page.items[0].category, "Videos");
        assert_eq!(page.items[1].category, "Playlists");
        assert_eq!(
            page.items[1].destination.as_ref().unwrap().body["browseId"],
            "VLPL123"
        );
    }
}
