//! Background audio downloads used by the local MCP server.

use crate::{db::Track, integrations};
use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

static NEXT_JOB: AtomicU64 = AtomicU64::new(1);
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileResult {
    pub title: String,
    pub path: Option<PathBuf>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Job {
    pub id: String,
    pub status: Status,
    pub collection: String,
    pub format: String,
    pub total: usize,
    pub completed: usize,
    pub files: Vec<FileResult>,
    pub error: Option<String>,
}

struct Entry {
    job: Mutex<Job>,
    cancelled: AtomicBool,
}

#[derive(Clone, Default)]
pub struct Manager {
    jobs: Arc<Mutex<BTreeMap<String, Arc<Entry>>>>,
}

#[derive(Clone)]
pub struct Request {
    pub collection: String,
    pub tracks: Vec<Track>,
    pub output_directory: PathBuf,
    pub format: String,
    pub overwrite: bool,
    pub numbered: bool,
}

impl Manager {
    pub fn start(&self, request: Request) -> Result<Job> {
        validate(&request)?;
        let id = job_id();
        let entry = Arc::new(Entry {
            job: Mutex::new(Job {
                id: id.clone(),
                status: Status::Queued,
                collection: request.collection.clone(),
                format: request.format.clone(),
                total: request.tracks.len(),
                completed: 0,
                files: Vec::new(),
                error: None,
            }),
            cancelled: AtomicBool::new(false),
        });
        self.jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, entry.clone());
        let initial = entry.job.lock().unwrap_or_else(|e| e.into_inner()).clone();
        thread::spawn(move || run(entry, request));
        Ok(initial)
    }

    pub fn get(&self, id: &str) -> Option<Job> {
        self.jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .map(|entry| entry.job.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }

    pub fn list(&self) -> Vec<Job> {
        self.jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|entry| entry.job.lock().unwrap_or_else(|e| e.into_inner()).clone())
            .collect()
    }

    pub fn cancel(&self, id: &str) -> Option<Job> {
        let entry = self
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()?;
        entry.cancelled.store(true, Ordering::Release);
        Some(entry.job.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }
}

fn validate(request: &Request) -> Result<()> {
    if request.tracks.is_empty() {
        bail!("there are no available tracks to download");
    }
    if !request.output_directory.is_absolute() {
        bail!("output_directory must be an absolute path");
    }
    if request.format != "best"
        && (request.format.is_empty()
            || request.format.len() > 16
            || !request.format.chars().all(|c| c.is_ascii_alphanumeric()))
    {
        bail!("format must be 'best' or an audio filename extension");
    }
    Ok(())
}

fn job_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!(
        "download-{millis}-{}",
        NEXT_JOB.fetch_add(1, Ordering::Relaxed)
    )
}

fn run(entry: Arc<Entry>, request: Request) {
    {
        let mut job = entry.job.lock().unwrap_or_else(|e| e.into_inner());
        job.status = Status::Running;
    }
    let directory = if request.numbered {
        request
            .output_directory
            .join(safe_name(&request.collection))
    } else {
        request.output_directory.clone()
    };
    if let Err(error) = fs::create_dir_all(&directory) {
        finish_error(&entry, error.into());
        return;
    }
    for (index, track) in request.tracks.iter().enumerate() {
        if entry.cancelled.load(Ordering::Acquire) {
            finish_cancelled(&entry);
            return;
        }
        let prefix = if request.numbered {
            format!("{:03} - ", index + 1)
        } else {
            String::new()
        };
        let artist = if track.artist.trim().is_empty() {
            String::new()
        } else {
            format!("{} - ", track.artist.trim())
        };
        let stem = safe_name(&format!("{prefix}{artist}{}", track.title));
        let result = download_one(
            track,
            &directory,
            &stem,
            &request.format,
            request.overwrite,
            &entry.cancelled,
        );
        let mut job = entry.job.lock().unwrap_or_else(|e| e.into_inner());
        job.completed += 1;
        job.files.push(match result {
            Ok(path) => FileResult {
                title: track.title.clone(),
                path: Some(path),
                error: None,
            },
            Err(error) => FileResult {
                title: track.title.clone(),
                path: None,
                error: Some(format!("{error:#}")),
            },
        });
    }
    let mut job = entry.job.lock().unwrap_or_else(|e| e.into_inner());
    if entry.cancelled.load(Ordering::Acquire) {
        job.status = Status::Cancelled;
    } else if job.files.iter().all(|file| file.path.is_none()) {
        job.status = Status::Failed;
        job.error = Some("every track failed to download".into());
    } else {
        job.status = Status::Completed;
    }
}

fn download_one(
    track: &Track,
    directory: &Path,
    stem: &str,
    format: &str,
    overwrite: bool,
    cancelled: &AtomicBool,
) -> Result<PathBuf> {
    let temporary = directory.join(format!(
        ".{stem}.mousa-download-{}",
        NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&temporary)?;
    let template = temporary.join("source.%(ext)s");
    let mut command = Command::new(integrations::ytdlp_path());
    command.args([
        "--no-playlist",
        "--no-warnings",
        "--no-progress",
        "--socket-timeout",
        "20",
        "-f",
        "bestaudio/best",
        "-o",
    ]);
    command.arg(&template).args(["--", &track.url]);
    let output = run_child(command, cancelled).context("yt-dlp could not download the track")?;
    if !output.success() {
        cleanup(&temporary);
        bail!("yt-dlp exited with status {output}");
    }
    let source = fs::read_dir(&temporary)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .find(|path| path.is_file() && path.extension().is_some())
        .ok_or_else(|| anyhow!("yt-dlp did not produce an audio file"))?;
    let extension = if format == "best" {
        source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("audio")
    } else {
        format
    };
    let destination = directory.join(format!("{stem}.{extension}"));
    if destination.exists() && !overwrite {
        cleanup(&temporary);
        return Ok(destination);
    }
    if format == "best" {
        if overwrite && destination.exists() {
            fs::remove_file(&destination)?;
        }
        fs::rename(&source, &destination)?;
    } else {
        let mut ffmpeg = Command::new(integrations::ffmpeg_path());
        ffmpeg.args(["-hide_banner", "-loglevel", "error"]);
        ffmpeg.arg(if overwrite { "-y" } else { "-n" });
        ffmpeg.arg("-i").arg(&source).args(["-vn", "-metadata"]);
        ffmpeg.arg(format!("title={}", track.title));
        ffmpeg.args(["-metadata"]);
        ffmpeg.arg(format!("artist={}", track.artist));
        ffmpeg.arg(&destination);
        let output = run_child(ffmpeg, cancelled)
            .context("ffmpeg is required to export the requested format")?;
        if !output.success() {
            cleanup(&temporary);
            bail!("ffmpeg could not export '{format}' (status {output})");
        }
    }
    cleanup(&temporary);
    Ok(destination)
}

fn run_child(mut command: Command, cancelled: &AtomicBool) -> Result<ExitStatus> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    poll_child(&mut child, cancelled)
}

fn poll_child(child: &mut Child, cancelled: &AtomicBool) -> Result<ExitStatus> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if cancelled.load(Ordering::Acquire) {
            child.kill()?;
            return Ok(child.wait()?);
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn finish_error(entry: &Entry, error: anyhow::Error) {
    let mut job = entry.job.lock().unwrap_or_else(|e| e.into_inner());
    job.status = Status::Failed;
    job.error = Some(format!("{error:#}"));
}

fn finish_cancelled(entry: &Entry) {
    let mut job = entry.job.lock().unwrap_or_else(|e| e.into_inner());
    job.status = Status::Cancelled;
}

fn cleanup(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

fn safe_name(value: &str) -> String {
    let mut name = value
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\0'..='\u{1f}' => '_',
            other => other,
        })
        .collect::<String>();
    name = name.trim().trim_end_matches(['.', ' ']).to_string();
    if name.is_empty() {
        name = "Untitled".into();
    }
    const MAX: usize = 180;
    if name.chars().count() > MAX {
        name = name.chars().take(MAX).collect();
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_portable_file_names() {
        assert_eq!(safe_name(" A/B:C*? . "), "A_B_C__");
        assert_eq!(safe_name("..."), "Untitled");
    }

    #[test]
    fn accepts_free_form_safe_formats() {
        let base = Request {
            collection: "Track".into(),
            tracks: vec![Track {
                id: 1,
                playlist_index: 0,
                url: "https://example.com".into(),
                title: "Track".into(),
                artist: "Artist".into(),
                duration: 1.0,
                thumbnail: None,
                unavailable: None,
            }],
            output_directory: std::env::temp_dir(),
            format: "aiff".into(),
            overwrite: false,
            numbered: false,
        };
        assert!(validate(&base).is_ok());
        assert!(
            validate(&Request {
                format: "x;rm".into(),
                ..base
            })
            .is_err()
        );
    }
}
