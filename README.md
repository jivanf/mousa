# Mousa

A native Linux music player built with Rust, GPUI, SQLite/FTS5, and libmpv. Cream paper, acid lime, pink accents, hard borders and offset shadows. No webview.

## Run

Requirements: Rust 1.91+, a Vulkan-capable desktop, libmpv, yt-dlp, and a running Secret Service keyring for account credentials. On Arch Linux, the media packages are `mpv` and `yt-dlp`. GPUI also needs the usual Linux graphics/text development libraries (Wayland/X11, Vulkan, fontconfig, freetype, xkbcommon); D-Bus development files are required for the keyring integration.

```sh
cargo run --release --locked
```

Install a local binary, icon, and desktop launcher:

```sh
./scripts/install.sh
```

## Features in this first build

- Persistent local playlists: create, rename, delete, add/remove tracks, and reorder entries.
- Search playlist names; indexed prefix search across track titles and artists. Multiple terms use AND semantics.
- Sort tracks by playlist order, title, artist, or duration.
- Playing a filtered result starts at that exact playlist entry in the full current sort order. Duplicate videos retain separate entry identities.
- Virtualized track and queue lists; only visible rows are rendered.
- YouTube video and playlist URL imports through yt-dlp.
- libmpv playback, play/pause, previous/next, seek slider and ±10 seconds, volume, shuffle, playlist repeat.
- Queue append/remove/jump, and restoration of queue/settings across restarts. Playback remains paused on startup.
- Linux MPRIS metadata and playback controls.
- Google desktop OAuth with PKCE/state validation and a loopback callback. Import all pages of account playlists as local copies.
- Last.fm authorization, now-playing, and scrobbling after half the track or four minutes, for tracks longer than 30 seconds. Paused/buffering time does not count. Failed scrobbles remain in SQLite for retries.
- Refresh tokens and Last.fm credentials stored in the operating system keyring.

Playlist CRUD is local. Google import creates local copies; changes are not pushed back to YouTube, and repeated imports create additional copies. This keeps remote edits explicit for a future synchronization implementation. YouTube Music-specific collections are not assumed to be exposed by the public YouTube Data API.

## Keyboard controls

- Ctrl+Space: play/pause
- Ctrl+Right / Ctrl+Left: next/previous
- Ctrl+F: focus track search

Reorder arrows are shown in unfiltered playlist order. Filtering never silently replaces the playback queue.

## Google setup

YouTube Data API has been enabled in the existing `rustube-music` Google Cloud project. Create a **Desktop app** OAuth client in Google Auth Platform, configure the consent audience, and add your account as a test user if the project remains in Testing.

Save its downloaded JSON as:

```text
~/.local/share/mousa/google-client.json
```

Then click **Connect Google**, complete consent in the system browser, and select **Import Google Playlists**. The sign-in callback expires after three minutes. The application requests only `youtube.readonly`. Google API authorization is separate from yt-dlp playback and does not authorize yt-dlp with your Google account.

The **Mousa** Desktop OAuth client is configured on this workstation, and sign-in has been completed. Its JSON is stored outside the repository with owner-only permissions; the refresh token is in the system keyring. No Google credentials are committed. The existing project is External / In production and remains unverified; Google currently displays its existing consent name, `project-352734193567`. Public app verification and consent-screen branding are separate from this working desktop sign-in setup.

## Last.fm setup

Create a Last.fm API application at https://www.last.fm/api/account/create. In **Last.fm / Settings**, enter the API key and secret and save. Reopen settings, choose **Authorize**, approve in the browser, then choose **Finish**. Credentials stay in the system keyring. Scrobbling begins only after authorization; earlier listening is not retroactively submitted. Video uploader/title metadata can differ from Last.fm's artist/song naming.

## Performance architecture

SQLite uses WAL, foreign keys, indexed playlist ordering/sorting, and FTS5 with escaped user terms. Selective searches force row-ID lookups from FTS results rather than scanning an entire playlist's ordering index. Database/network work runs outside the UI thread. Search input is debounced and stale results are discarded. Playback owns libmpv on its own worker. The UI and player share queue snapshots through `Arc`, and idle polling only redraws when visible state changes.

All metadata for the selected playlist is currently held in memory; rows are virtualized, not the underlying database result. The representative UI fixture is 5,000 entries. An optional 100,000-entry test remains as a stress probe.

## Validation

```sh
cargo test --test library
cargo run --example google_oauth_probe -- --check-existing
cargo test --test library -- --ignored --nocapture
MOUSA_DATA_DIR=/tmp/mousa-playback-probe cargo run --example playback_probe
MOUSA_DATA_DIR=/tmp/mousa-youtube-probe cargo run --example youtube_probe
MOUSA_DATA_DIR=/tmp/mousa-demo cargo run --example seed
MOUSA_DATA_DIR=/tmp/mousa-demo cargo run --release
```

The playback probe uses a generated silent WAV. The YouTube probe uses a real public video with muted playback. The UI seed contains 5,000 clearly synthetic rows and refuses a nonempty library. Tests never populate the normal library.

## Current boundaries

This is an initial Linux implementation. Windows/macOS packaging and native media controls are not implemented. There is no offline media download, gapless guarantee, cloud write-back, or YouTube Music recommendation feed. YouTube resolution is unofficial and may require yt-dlp updates when YouTube changes. API failures surface in the status bar; unavailable tracks can be skipped manually.

The GPUI-compatible component version is pinned; only its native input and slider widgets are used. The optional webview feature is disabled. `vendor/xattr` retains upstream licenses and a small Linux compatibility fix for the removed `libc::ENOATTR` alias; see `vendor/xattr/MOUSA-PATCH.md`.
