//! Read-only YouTube Music catalog. Never uses the official, quota-metered API.
use anyhow::{Context, Result, bail, ensure};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Destination {
    pub api: String,
    pub body: Value,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Item {
    pub title: String,
    pub subtitle: String,
    pub kind: String,
    pub category: String,
    pub thumbnail: Option<String>,
    pub video: Option<String>,
    pub duration: f64,
    pub artist: String,
    pub destination: Option<Destination>,
    pub unavailable: bool,
    pub collection_member: bool,
}
impl Item {
    pub fn track(&self) -> Option<crate::db::Track> {
        let video = self.video.as_ref()?;
        if self.unavailable {
            return None;
        }
        Some(crate::db::Track {
            // Negative random identities remain distinct from positive library entry IDs,
            // including after restoring a saved queue across process restarts.
            id: -((rand::random::<u64>() % (i64::MAX as u64 - 1) + 1) as i64),
            playlist_index: 0,
            url: format!("https://www.youtube.com/watch?v={video}"),
            title: self.title.clone(),
            artist: self.artist.clone(),
            duration: self.duration,
            unavailable: None,
        })
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Page {
    pub title: String,
    pub subtitle: String,
    pub metadata: String,
    pub byline: String,
    pub creator: Option<Destination>,
    pub thumbnail: Option<String>,
    pub items: Vec<Item>,
    pub filters: Vec<(String, Destination)>,
    pub more: Vec<(String, Destination)>,
    pub shuffle: Option<Destination>,
}

struct Cache {
    pages: HashMap<String, (Instant, Page)>,
    blocked_until: Option<Instant>,
}
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
fn cache() -> &'static Mutex<Cache> {
    CACHE.get_or_init(|| {
        Mutex::new(Cache {
            pages: HashMap::new(),
            blocked_until: None,
        })
    })
}
pub fn search(query: &str) -> Destination {
    Destination {
        api: "search".into(),
        body: json!({"query":query}),
    }
}
pub fn fetch(destination: &Destination) -> Result<Page> {
    fetch_cancellable(destination, || false)
}

pub fn fetch_cancellable(destination: &Destination, cancelled: impl Fn() -> bool) -> Result<Page> {
    ensure!(
        ["search", "browse", "next"].contains(&destination.api.as_str()),
        "Unsupported catalog destination"
    );
    let key = format!("{}:{}", destination.api, destination.body);
    // Serializing transport coalesces duplicate requests and bounds catalog concurrency.
    let mut cache = cache().lock().unwrap();
    ensure!(!cancelled(), "Superseded catalog request");
    if let Some((at, page)) = cache.pages.get(&key)
        && at.elapsed() < Duration::from_secs(300)
    {
        return Ok(page.clone());
    }
    if cache
        .blocked_until
        .is_some_and(|until| until > Instant::now())
    {
        bail!("YouTube Music is temporarily limiting requests. Please try again shortly.");
    }
    let mut body = destination.body.clone();
    body["context"] = json!({"client":{"clientName":"WEB_REMIX","clientVersion":"1.20261021.01.00","hl":"en","gl":"US"}});
    let response = Client::builder()
        .timeout(Duration::from_secs(25))
        .build()?
        .post(format!(
            "https://music.youtube.com/youtubei/v1/{}?prettyPrint=false",
            destination.api
        ))
        .header("Origin", "https://music.youtube.com")
        .json(&body)
        .send()
        .context("Couldn’t reach YouTube Music. Check your connection and retry.")?;
    if response.status().as_u16() == 429 {
        let delay = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(60u64);
        cache.blocked_until = Some(Instant::now() + Duration::from_secs(delay.min(3600)));
        bail!("YouTube Music is temporarily limiting requests. Retry in {delay} seconds.");
    }
    let response = response
        .error_for_status()
        .context("YouTube Music request failed")?;
    let value: Value = response.json().context("Invalid YouTube Music response")?;
    ensure!(
        value.get("error").is_none(),
        "YouTube Music rejected this request"
    );
    let page = parse(&value, &destination.api);
    cache
        .pages
        .retain(|_, (at, _)| at.elapsed() < Duration::from_secs(300));
    if cache.pages.len() >= 64 {
        cache.pages.clear();
    }
    cache.pages.insert(key, (Instant::now(), page.clone()));
    Ok(page)
}

pub fn text(v: &Value) -> String {
    v["simpleText"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| {
            v["runs"]
                .as_array()
                .map(|runs| runs.iter().filter_map(|r| r["text"].as_str()).collect())
                .unwrap_or_default()
        })
}
fn find<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    match v {
        Value::Object(map) => map
            .get(key)
            .or_else(|| map.values().find_map(|v| find(v, key))),
        Value::Array(items) => items.iter().find_map(|v| find(v, key)),
        _ => None,
    }
}
fn thumbnail(v: &Value) -> Option<String> {
    find(v, "thumbnails")?.as_array()?.last()?["url"]
        .as_str()
        .filter(|s| s.starts_with("https://"))
        .map(str::to_owned)
}
fn endpoint(v: &Value) -> Option<Destination> {
    for (name, api) in [
        ("browseEndpoint", "browse"),
        ("searchEndpoint", "search"),
        ("watchEndpoint", "next"),
        ("watchPlaylistEndpoint", "next"),
    ] {
        if let Some(body) = v.get(name) {
            return Some(Destination {
                api: api.into(),
                body: body.clone(),
            });
        }
    }
    None
}
fn title_endpoint(v: &Value) -> Option<Destination> {
    endpoint(&v["navigationEndpoint"])
        .or_else(|| endpoint(&v["onTap"]))
        .or_else(|| find(&v["title"], "navigationEndpoint").and_then(endpoint))
        .or_else(|| find(&v["flexColumns"][0], "navigationEndpoint").and_then(endpoint))
}
fn seconds(s: &str) -> Option<f64> {
    let bits: Vec<_> = s.trim().split(':').collect();
    if !(2..=3).contains(&bits.len()) {
        return None;
    }
    bits.into_iter()
        .try_fold(0u64, |n, v| {
            v.parse::<u64>()
                .ok()
                .filter(|v| *v < 60 || n == 0)
                .and_then(|v| n.checked_mul(60)?.checked_add(v))
        })
        .map(|n| n as f64)
}
fn item(v: &Value, category: &str) -> Option<Item> {
    let title = if v.get("flexColumns").is_some() {
        text(&v["flexColumns"][0]["musicResponsiveListItemFlexColumnRenderer"]["text"])
    } else {
        text(&v["title"])
    };
    if title.is_empty() {
        return None;
    }
    let subtitle = if let Some(cols) = v["flexColumns"].as_array() {
        cols.iter()
            .skip(1)
            .map(|c| text(&c["musicResponsiveListItemFlexColumnRenderer"]["text"]))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    } else {
        text(&v["subtitle"])
    };
    let dest = title_endpoint(v);
    let play = find(&v["overlay"], "playNavigationEndpoint")
        .and_then(endpoint)
        .or_else(|| find(&v["thumbnailOverlay"], "playNavigationEndpoint").and_then(endpoint));
    let video = v["videoId"]
        .as_str()
        .or_else(|| v["playlistItemData"]["videoId"].as_str())
        .or_else(|| {
            dest.as_ref()
                .filter(|d| d.api == "next")
                .and_then(|d| d.body["videoId"].as_str())
        })
        .or_else(|| play.as_ref().and_then(|d| d.body["videoId"].as_str()))
        .map(str::to_owned);
    let page_type = dest
        .as_ref()
        .and_then(|d| find(&d.body, "pageType"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let video_type = dest
        .as_ref()
        .and_then(|d| find(&d.body, "musicVideoType"))
        .or_else(|| play.as_ref().and_then(|d| find(&d.body, "musicVideoType")))
        .and_then(Value::as_str)
        .unwrap_or("");
    let label = subtitle.split(['•', '·']).next().unwrap_or("").trim();
    let kind = match page_type {
        "MUSIC_PAGE_TYPE_ARTIST" => "Artist",
        "MUSIC_PAGE_TYPE_USER_CHANNEL" => "Profile",
        "MUSIC_PAGE_TYPE_ALBUM" => "Album",
        "MUSIC_PAGE_TYPE_PLAYLIST" => "Playlist",
        "MUSIC_PAGE_TYPE_PODCAST_SHOW_DETAIL_PAGE" => "Podcast",
        "MUSIC_PAGE_TYPE_PODCAST_EPISODE_DETAIL_PAGE"
        | "MUSIC_PAGE_TYPE_NON_MUSIC_AUDIO_TRACK_PAGE" => "Episode",
        _ if video_type == "MUSIC_VIDEO_TYPE_ATV" => "Song",
        _ if video_type == "MUSIC_VIDEO_TYPE_PODCAST_EPISODE" => "Episode",
        _ if [
            "Song", "Video", "Artist", "Album", "Single", "EP", "Playlist", "Podcast", "Episode",
            "Profile", "Station",
        ]
        .contains(&label) =>
        {
            label
        }
        _ if video.is_some() => "Video",
        _ => {
            if category.is_empty() {
                "Result"
            } else {
                category
            }
        }
    }
    .to_owned();
    let duration = find(&v["fixedColumns"], "text")
        .map(text)
        .and_then(|s| seconds(&s))
        .or_else(|| seconds(&text(&v["lengthText"])))
        .or_else(|| subtitle.split(['•', '·']).find_map(seconds))
        .unwrap_or(0.);
    let mut artists = Vec::new();
    fn artist_runs(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(m) => {
                if let Some(d) = endpoint(&v["navigationEndpoint"])
                    && d.body["browseId"]
                        .as_str()
                        .is_some_and(|id| id.starts_with("UC"))
                    && let Some(name) = v["text"].as_str()
                {
                    out.push(name.into());
                    return;
                }
                for v in m.values() {
                    artist_runs(v, out);
                }
            }
            Value::Array(a) => {
                for v in a {
                    artist_runs(v, out);
                }
            }
            _ => (),
        }
    }
    artist_runs(&v["flexColumns"], &mut artists);
    artist_runs(&v["subtitle"], &mut artists);
    artists.dedup();
    Some(Item {
        title,
        subtitle,
        kind,
        category: category.into(),
        thumbnail: thumbnail(&v["thumbnail"]).or_else(|| thumbnail(&v["thumbnailRenderer"])),
        video,
        duration,
        artist: artists.join(" · "),
        destination: dest.or(play),
        unavailable: v["isPlayable"].as_bool() == Some(false)
            || v["musicItemRendererDisplayPolicy"] == "MUSIC_ITEM_RENDERER_DISPLAY_POLICY_GREY_OUT",
        collection_member: true,
    })
}

pub fn parse(v: &Value, api: &str) -> Page {
    let mut page = Page::default();
    fn walk(v: &Value, category: &str, api: &str, page: &mut Page) {
        match v {
            Value::Array(a) => {
                for v in a {
                    walk(v, category, api, page);
                }
            }
            Value::Object(map) => {
                for (key, value) in map {
                    match key.as_str() {
                        "musicResponsiveListItemRenderer"
                        | "musicTwoRowItemRenderer"
                        | "playlistPanelVideoRenderer"
                        | "musicMultiRowListItemRenderer" => {
                            if let Some(i) = item(value, category) {
                                page.items.push(i);
                            }
                        }
                        "musicCardShelfRenderer" => {
                            if let Some(i) = item(value, "Top result") {
                                page.items.push(i);
                            }
                            walk(&value["contents"], "Top result", api, page);
                        }
                        "chipCloudChipRenderer" => {
                            if let Some(d) = endpoint(&value["navigationEndpoint"]) {
                                page.filters.push((text(&value["text"]), d));
                            }
                        }
                        "musicShelfRenderer"
                        | "musicCarouselShelfRenderer"
                        | "musicPlaylistShelfRenderer" => {
                            let title = text(&value["title"]);
                            let title = if title.is_empty() {
                                find(&value["header"], "title")
                                    .map(text)
                                    .unwrap_or_default()
                            } else {
                                title
                            };
                            let category = if title.is_empty() { category } else { &title };
                            let start = page.items.len();
                            walk(&value["contents"], category, api, page);
                            if key == "musicCarouselShelfRenderer" {
                                for item in &mut page.items[start..] {
                                    item.collection_member = false;
                                }
                            }
                            walk(&value["continuations"], category, api, page);
                            for candidate in [
                                &value["bottomEndpoint"],
                                &value["moreContentButton"]["buttonRenderer"]["navigationEndpoint"],
                            ] {
                                if let Some(d) = endpoint(candidate) {
                                    page.more.push((format!("More {category}"), d));
                                }
                            }
                            if let Some(e) = find(&value["header"], "moreContentButton")
                                .and_then(|v| find(v, "navigationEndpoint"))
                                .and_then(endpoint)
                            {
                                page.more.push((format!("More {category}"), e));
                            }
                        }
                        "nextContinuationData" | "reloadContinuationData" => {
                            if let Some(token) = value["continuation"].as_str() {
                                page.more.push((
                                    "Load more".into(),
                                    Destination {
                                        api: api.into(),
                                        body: json!({"continuation":token}),
                                    },
                                ));
                            }
                        }
                        "continuationCommand" => {
                            if let Some(token) = value["token"].as_str() {
                                page.more.push((
                                    "Load more".into(),
                                    Destination {
                                        api: api.into(),
                                        body: json!({"continuation":token}),
                                    },
                                ));
                            }
                        }
                        "musicDetailHeaderRenderer"
                        | "musicResponsiveHeaderRenderer"
                        | "musicImmersiveHeaderRenderer"
                        | "musicVisualHeaderRenderer" => {
                            page.title = text(&value["title"]);
                            page.subtitle = text(&value["subtitle"]);
                            page.metadata = text(&value["secondSubtitle"]);
                            page.byline = text(&value["straplineText"]);
                            page.creator = find(&value["straplineText"], "navigationEndpoint")
                                .and_then(endpoint);
                            if page.byline.is_empty() {
                                page.byline = text(&value["straplineTextOne"]);
                                page.creator =
                                    find(&value["straplineTextOne"], "navigationEndpoint")
                                        .and_then(endpoint);
                            }
                            page.thumbnail = thumbnail(&value["thumbnail"]);
                            if let Some(e) =
                                find(&value["playButton"], "navigationEndpoint").and_then(endpoint)
                            {
                                page.shuffle = Some(e);
                            }
                            if let Some(e) =
                                find(value, "shuffleNavigationEndpoint").and_then(endpoint)
                            {
                                page.shuffle = Some(e);
                            }
                        }
                        // Menus and background tabs contain duplicate or unrelated navigation.
                        "menu" | "trackingParams" | "frameworkUpdates" | "background" => (),
                        _ => walk(value, category, api, page),
                    }
                }
            }
            _ => (),
        }
    }
    walk(v, "", api, &mut page);
    let mut seen = HashSet::new();
    page.more
        .retain(|(_, d)| seen.insert(format!("{}:{}", d.api, d.body)));
    page
}

/// Resolve a finite collection before a queue or save action. Cancellation never mutates it.
pub fn collection(
    page: &Page,
    cancelled: impl Fn() -> bool,
    progress: impl Fn(usize),
) -> Result<Vec<Item>> {
    let mut items: Vec<_> = page
        .items
        .iter()
        .filter(|i| i.collection_member)
        .cloned()
        .collect();
    let mut pending: Vec<_> = page
        .more
        .iter()
        .filter(|(_, d)| d.body.get("continuation").is_some())
        .map(|(_, d)| d.clone())
        .collect();
    let mut seen = HashSet::new();
    while let Some(d) = pending.pop() {
        ensure!(!cancelled(), "Collection loading cancelled");
        ensure!(
            seen.insert(serde_json::to_string(&d)?),
            "YouTube Music returned a repeated continuation"
        );
        let next = fetch(&d)?;
        items.extend(next.items.into_iter().filter(|i| i.collection_member));
        progress(items.len());
        pending.extend(
            next.more
                .into_iter()
                .filter(|(_, d)| d.body.get("continuation").is_some())
                .map(|(_, d)| d),
        );
    }
    ensure!(!cancelled(), "Collection loading cancelled");
    Ok(items)
}

pub fn thumbnail_path(url: &str) -> std::path::PathBuf {
    use sha2::{Digest, Sha256};
    crate::db::data_dir()
        .join("thumbnails")
        .join(format!("{:x}.img", Sha256::digest(url.as_bytes())))
}
pub fn load_thumbnail(url: &str) -> Result<()> {
    use std::io::{Read, Write};
    let parsed = url::Url::parse(url)?;
    let host = parsed.host_str().unwrap_or("");
    ensure!(
        parsed.scheme() == "https"
            && ["ytimg.com", "googleusercontent.com", "ggpht.com"]
                .iter()
                .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}"))),
        "Unsupported thumbnail host"
    );
    let path = thumbnail_path(url);
    if path.is_file() {
        return Ok(());
    }
    let mut bytes = Vec::new();
    Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()?
        .get(url)
        .send()?
        .error_for_status()?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 4 * 1024 * 1024, "Thumbnail is too large");
    std::fs::create_dir_all(path.parent().unwrap())?;
    let temp = path.with_extension(format!("{}.tmp", rand::random::<u64>()));
    std::fs::File::create(&temp)?.write_all(&bytes)?;
    if !path.exists() {
        std::fs::rename(&temp, &path)?;
    } else {
        let _ = std::fs::remove_file(&temp);
    }
    Ok(())
}
