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

- Local and YouTube playlists: create, rename, delete, add/remove tracks, and reorder entries. New YouTube playlists are private; the creation form also offers local-only playlists.
- Search playlist names; indexed prefix search across track titles and artists. Multiple terms use AND semantics.
- Tracks always follow the saved playlist order; use the reorder arrows to change it.
- Playing a filtered result starts at that exact playlist entry in the full saved playlist order. Duplicate videos retain separate entry identities.
- Virtualized track and queue lists; only visible rows are rendered.
- YouTube video and playlist URL imports through yt-dlp.
- libmpv playback, play/pause, previous/next, seek slider and ±10 seconds, volume, shuffle, playlist repeat.
- Queue append/remove/jump, and restoration of queue/settings across restarts. Playback remains paused on startup.
- Linux MPRIS metadata and playback controls.
- Google desktop OAuth with PKCE/state validation and a loopback callback. Owned YouTube playlists appear automatically on startup and after sign-in, with background refresh every ten minutes and an optional Refresh button. Cached lists are immediately available.
- Last.fm authorization, now-playing, and scrobbling after half the track or four minutes, for tracks longer than 30 seconds. Paused/buffering time does not count. Failed scrobbles remain in SQLite for retries.
- Refresh tokens and Last.fm credentials stored in the operating system keyring.

YouTube playlists are labeled in the sidebar. Their edits are saved to YouTube before local success is reported; deletion also deletes the remote playlist and asks for confirmation. A failed edit remains visible as an error. Offline browsing uses the cache, but remote edits require a connection; there is no offline write queue. Partial multi-track additions are reported with a completed count: refresh before retrying. YouTube may require manual playlist ordering for reorder operations. Local-only playlists stay local. Earlier imported copies remain local and are never silently converted or deleted. YouTube Music-specific collections and playlists merely saved from other owners are not assumed to be exposed by the public owned-playlists API.

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

Then click **Connect Google** and complete consent in the system browser. Your playlists appear automatically. Existing read-only users must reconnect once to approve editing. The sign-in callback expires after three minutes. The application requests the `youtube` management scope required for playlist edits; Google describes this as managing your YouTube account. Google API authorization is separate from yt-dlp playback and does not authorize yt-dlp with your Google account.

The **Mousa** Desktop OAuth client is configured on this workstation, and sign-in with playlist management permission has been completed. Its JSON is stored outside the repository with owner-only permissions; the refresh token is in the system keyring. No Google credentials are committed. The existing project is External / In production and remains unverified; Google currently displays its existing consent name, `project-352734193567`. Public app verification and consent-screen branding are separate from this working desktop sign-in setup.

## Last.fm setup

Create a Last.fm API application at https://www.last.fm/api/account/create. In **Last.fm / Settings**, enter the API key and secret and save. Reopen settings, choose **Authorize**, approve in the browser, then choose **Finish**. Credentials stay in the system keyring. Scrobbling begins only after authorization; earlier listening is not retroactively submitted. Video uploader/title metadata can differ from Last.fm's artist/song naming.

## Performance architecture

SQLite uses WAL, foreign keys, indexed playlist ordering/sorting, and FTS5 with escaped user terms. Selective searches force row-ID lookups from FTS results rather than scanning an entire playlist's ordering index. Database/network work runs outside the UI thread. Search input is debounced and stale results are discarded. Playback owns libmpv on its own worker. The UI and player share queue snapshots through `Arc`, and idle polling only redraws when visible state changes.

Google playlist refresh also fetches video lengths through [`videos.list` with `contentDetails`](https://developers.google.com/youtube/v3/docs/videos/list). Requests batch up to 50 unique video IDs, sharing results across playlists during each refresh. Existing entries gain their lengths without changing IDs; unavailable lengths display `–:–`. Duration stays at the right edge of each row while actions appear immediately to its left on hover or keyboard focus.

Remote catalogs and playlist entries are reconciled transactionally using stable YouTube playlist and playlist-item IDs. Complete pages are fetched before replacing each cached snapshot; duplicate videos retain distinct IDs, and refreshes do not duplicate playlists. Confirmed edits are protected from stale API reads for one minute, including deletion tombstones, because YouTube list responses can briefly lag behind successful writes. Remote operations are serialized within the process to keep refreshes from overwriting in-flight edits.

All metadata for the selected playlist is currently held in memory; rows are virtualized, not the underlying database result. The representative UI fixture is 5,000 entries. An optional 100,000-entry test remains as a stress probe.

## Validation

```sh
cargo test --test library
cargo run --example google_oauth_probe -- --check-existing --require-editing
cargo run --example google_sync_probe
cargo test --test library -- --ignored --nocapture
MOUSA_DATA_DIR=/tmp/mousa-playback-probe cargo run --example playback_probe
MOUSA_DATA_DIR=/tmp/mousa-youtube-probe cargo run --example youtube_probe
MOUSA_DATA_DIR=/tmp/mousa-demo cargo run --example seed
MOUSA_DATA_DIR=/tmp/mousa-demo cargo run --release
```

The optional `cargo run --example google_sync_probe -- --write-roundtrip` creates a private temporary playlist, checks all edit operations, and deletes that test playlist.

The playback probe uses a generated silent WAV. The YouTube probe uses a real public video with muted playback. The UI seed contains 5,000 clearly synthetic rows and refuses a nonempty library. Tests never populate the normal library.

## Current boundaries

This is an initial Linux implementation. Windows/macOS packaging and native media controls are not implemented. There is no offline media download, gapless guarantee, YouTube Music recommendation feed. YouTube resolution is unofficial and may require yt-dlp updates when YouTube changes. API failures appear in a dismissible error message; unavailable tracks can be skipped manually.

The GPUI-compatible component version is pinned; its native input widget is used; seek and volume use the shared Mousa bar component. The optional webview feature is disabled. `vendor/xattr` retains upstream licenses and a small Linux compatibility fix for the removed `libc::ENOATTR` alias; see `vendor/xattr/MOUSA-PATCH.md`.

## Design system

[DESIGN.md](DESIGN.md) records the accepted designer specification and review adjustments.
[src/theme.rs](src/theme.rs) defines native tokens; [src/ui.rs](src/ui.rs) owns shared components, fonts, and icons.
Space Grotesk, Space Mono, and Tabler icons are bundled with their licenses.

```sh
npx @google/design.md lint DESIGN.md
cargo run --example gallery
cargo run --example gallery -- --player-only
```

The gallery is independent of playback, accounts, and library data.
[Visual QA](design-qa.md) links the reference comparison and application screenshots.

### Complete music titles

YouTube sometimes puts literal ellipses in video titles. Mousa recovers full music names from the publisher block in auto-generated descriptions during Google sync, and from structured track metadata when available during URL import. Replacements must extend the exact title prefix; missing text is never guessed. Normal sync reuses descriptions already returned by the playlist API, with no extra per-track requests.

For an existing cache, `cargo run --example repair_titles` previews the number of recoverable titles. Add `-- --apply` to update them using batched video metadata requests. This changes local title metadata only; playlist membership, ordering, and YouTube data are unchanged.
