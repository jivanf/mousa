![Mousa logo](assets/mousa.svg)

# Mousa

A native Linux and Windows music player for YouTube.

## Features

- Create, rename, delete, and reorder local and YouTube playlists.
- Ctrl-click toggles track selection; Shift-click selects a range; Ctrl+Shift-click adds a range. Drag any selected row to move the entire selection in playlist order.
- Drag a track row to reorder a playlist; use the play button revealed over its number to play it. Reordering is disabled during search.
- Search YouTube Music from the sidebar or Ctrl+K: songs, videos, albums, artists, playlists, podcasts, episodes, and profiles.
- Filter your existing playlists and tracks independently of catalog search.
- Play a filtered playlist track and continue from that entry in the full playlist.
- Browse catalog detail pages, play or queue tracks and collections, and save them to local or owned YouTube playlists.
- Subscribe to artists from their detail pages.
- Play/pause, previous/next, seeking, volume, shuffle, and repeat.
- Manage your queue and restore it across restarts. Playback starts paused.
- Resume the last playlist or queue view, including per-playlist searches and scroll positions.
- Connect Google to see your owned YouTube playlists automatically.
- Browse cached playlists offline; YouTube playback and edits require a connection.
- Use Linux desktop media controls through MPRIS.
- Connect Last.fm for now-playing updates and scrobbling.

Changes to YouTube playlists are saved to your YouTube account. Deleting one in Mousa also deletes it on YouTube and requires confirmation. Local playlists stay on your device. Some YouTube Music collections and playlists saved from other owners may not be available.

## Install and run

### Windows (x64)

Install Rust 1.91+ using [rustup](https://rustup.rs/) and Visual Studio Build Tools
with the **Desktop development with C++** workload and a Windows SDK. Mousa uses
Direct3D on Windows and stores credentials in Windows Credential Manager.

From PowerShell in this repository, build, install, and launch:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1 -Launch
```

The installer downloads checksum-verified x64 libmpv and yt-dlp releases, builds
Mousa, and installs it to `%LOCALAPPDATA%\Programs\Mousa` with a Start menu shortcut.
Run the same command to update it after changing the source. Close Mousa before
updating. An alternative destination can be supplied with `-InstallDir`.
Add `-RunTests` to run the test suite and Windows credential/audio smoke tests
(requires a Windows audio device).

App data is stored in `%APPDATA%\Mousa\Mousa\data`. Put `google-client.json` in
that folder before signing in. The `MOUSA_DATA_DIR` environment variable can
override the data directory on either platform.

When tools run inside an MSIX-packaged app such as Codex, Windows can redirect
their AppData access to that package's `LocalCache\Roaming` directory. The same
printed path can therefore refer to a different configuration and library than
a normal Start menu launch. The installer's `-Launch` option uses an unpackaged
Windows process to avoid inheriting this redirection. Set up OAuth JSON files in
the real user AppData folder from an unpackaged process; verify authentication
there as well. Preserve both library databases when recovering an existing
split setup rather than overwriting either one.

### Linux

Requirements: Linux, Rust 1.91+, a Vulkan-capable desktop, libmpv, yt-dlp, and a running Secret Service keyring. On Arch Linux, the media packages are `mpv` and `yt-dlp`. Building also requires the Linux graphics and text development libraries for Wayland/X11, Vulkan, fontconfig, freetype, and xkbcommon, plus D-Bus development files.

Run from source:

```sh
cargo run --release --locked
```

Install a local binary, icon, and desktop launcher:

```sh
./scripts/install.sh
```

## Keyboard controls

- Ctrl+Space: play/pause
- Ctrl+Right / Ctrl+Left: next/previous track
- Ctrl+J / Ctrl+L: seek backward/forward ten seconds
- Ctrl+S: toggle shuffle
- Ctrl+R: toggle repeat
- Ctrl+M: mute/unmute
- Ctrl+K: open catalog search and focus its input
- Ctrl+F: focus the current search/filter field
- Esc in catalog search: close the destination picker, return from details, or restore the previous library/queue view
- Alt+Up / Alt+Down: move the selected group, or the focused playlist track (with search cleared)
- Esc: cancel a track drag; releasing outside the track list also cancels
- Tab / Shift+Tab: move between controls
- Enter / Space: activate a focused button
- Arrow keys: adjust a focused seek or volume slider
- Home / End: move a focused slider to its minimum/maximum

## Connect Google

Create a Google Cloud project, enable the **YouTube Data API v3**, and create a **Desktop app** OAuth client in Google Auth Platform. Configure the consent audience and add your account as a test user if the app is in Testing.

Save the downloaded client JSON at `%APPDATA%\Mousa\Mousa\data\google-client.json`
on Windows, or on Linux at:

```text
~/.local/share/mousa/google-client.json
```

Choose **Sign in with Google** on the landing page and complete authorization in your browser. Your owned playlists appear automatically and refresh every ten minutes. A Google account is required to enter the library.

The sidebar displays your full Google profile name. Hover the account button to reveal **Sign out**; signing out removes the local Google credentials, stops playback, and returns to the landing page. Cached playlists remain on this device. Existing installations need to sign in once more to grant profile access.

Mousa requests your basic Google profile and permission to manage YouTube playlists so you can edit them. If you previously granted read-only access, reconnect to enable editing. Google authorization is separate from video playback and does not sign yt-dlp into your account.

Keep the downloaded client file and account credentials private; do not commit them to a repository.

## YouTube refresh policy

The library catalog refreshes at most hourly. Playlist contents load when first opened; unused playlists are never downloaded just to support search, which is playlist-specific. Cached tracks remain immediately available while refreshes run.

Opened or playing playlists are checked for freshness every ten minutes. Their content refresh interval starts at one hour, doubles after unchanged snapshots, and caps at 24 hours. A detected change resets it to one hour. Restarts preserve freshness. The **Refresh** button bypasses this interval for the selected playlist; duplicate requests within five seconds are coalesced.

A playlist visited at least three times (visits separated by ten minutes), most recently within seven days, qualifies for opportunistic background refresh once daily. One candidate is considered per scheduler pass. Automatic API work shares a local budget of 2,000 estimated units per Pacific calendar day, reserving capacity for opening playlists, manual refreshes, and edits. This local estimate does not measure other installations using the same Google project.

Video durations persist for seven days; missing/unavailable metadata is retried after six hours. Successful batches survive later request failures. The quota-exhausted state persists until midnight America/Los_Angeles, including daylight-saving changes. While paused, cached data remains usable; YouTube edits require the quota to recover. Per-endpoint estimated usage is recorded locally, without credentials or request bodies.

On normal exit, Mousa saves browsing context and window size, position, and maximized state. Window placement follows desktop/compositor support; unavailable display positions fall back to centering. Menus, forms, unsaved form input, errors, and pending account actions reopen closed. If the saved playlist is missing, Mousa returns to the available library.

### Streaming preparation and playback measurements

Mousa prepares **one next-track stream URL when 30 seconds or less remain**, once the current track has loaded and playback is not paused. Short tracks prepare immediately; tracks with unknown duration wait for a valid duration. This resolves a URL with yt-dlp in the background; it does not prebuffer the next track's audio or save media files. The current stream continues using mpv's normal buffering.

On startup, a signed-in session also prepares its **saved current track's URL while remaining paused**. An empty queue or an explicitly stopped session does not trigger preparation. Pressing Play retains a pending lookup and loads its result when ready. Manual YouTube selections use the same bounded background resolver; pause, seek, and track switching remain responsive while it runs. Startup preparation runs once, does not refresh repeatedly while idle, and never loads audio into mpv or changes the saved playback position. Selecting another track, replacing/removing queue entries, or stopping cancels it. Volume, pause, seek, shuffle/repeat changes, and appending tracks preserve it because the selected current entry is unchanged. This result is separate from the next-track choice, so pressing Next cannot accidentally reuse the current track's URL.

Shuffle retains its chosen next entry for both preparation and advancement. Queue replacement, appends/removals, jumps, Previous, Stop, and changes to shuffle/repeat cancel the old preparation. Late results cannot overwrite the replacement. Pause and seek retain the choice; URLs older than two minutes are refreshed when playback re-enters the preparation window. URLs approaching their advertised expiry are not used. Preparation has a 25-second timeout; a pending preparation is retained when its track becomes current instead of being cancelled and restarted. Failed direct playback falls back once to resolving the original YouTube URL through mpv. This fallback tests candidate formats with yt-dlp so a URL rejected by the CDN can be replaced with a working format; these checks may fetch a small media sample. Playback-library error events trigger this fallback immediately. Prepared streams also retry the original URL if playback has not started within 15 seconds. Normal resolution/retries have a 45-second startup deadline; if that expires, playback stops with a retryable error rather than remaining at 0:00. Deliberate pauses suspend the deadline. Prepared loads keep mpv's YouTube helper running instead of unloading/reloading it between tracks. Required HTTP headers remain in memory and apply only to that file. Direct prepared loading uses mpv 0.38+'s per-file `loadfile` options; unsupported command syntax falls back to normal streaming.

Diagnostics are written asynchronously to `~/.local/share/mousa/playback.jsonl` (or the app's XDG/`MOUSA_DATA_DIR` data directory). Rotation retains the current file and `playback.jsonl.1`, each at most 1 MiB. They contain timestamps, process/playback IDs, queue indexes, preparation outcomes/durations, load-to-first-progress timing, file-loaded milestones, startup timeouts and retry reasons, and cache-wait counts/durations. They do not contain track titles, source/stream URLs, HTTP headers, credentials, or raw resolver errors. Writes are best-effort and may be dropped if the bounded log queue fills; logging never blocks playback.

After listening to several normally advancing tracks, summarize the logs with:

```sh
python3 scripts/playback-report.py
# Optional: python3 scripts/playback-report.py /path/to/mousa-data
```

The report separates `origin=resume` (pressing Play on an unloaded saved/current track) from `origin=automatic` (end-of-track advancement). Preparation events label their purpose as `startup` or `next`. Resume timing begins when the playback worker handles Play and includes seeking to the saved position; it does not include time spent idle before Play. An expected saved-position seek is not treated as a user interruption.

Compare `preparation=ready` automatic transitions with `pending`, `failed`, `expired`, or `not_requested`. `fallback` means a prepared load failed and normal resolution was retried. For automatic transitions, `first_progress_ms` measures from the observed end event to advancing playback position on the next track; `file_loaded_ms` and `playback_restart_ms` distinguish opening from playback readiness. User pauses/seeks during startup are marked `interrupted` and excluded from the report. Buffering before first progress is separate from stalls during playback. The playback loop samples at a nominal 50 ms interval; these values approximate audible gaps, not hardware output latency, and very short cache waits can be missed.

If prepared transitions are consistently quick and stall-free, further audio prebuffering is unlikely to help much. Persistent delays above roughly 200–300 ms **with cache waits**, or frequent playback stalls, justify a prebuffering experiment. URL-resolution failures or slow preparation call for resolver improvements first. Use a reasonable sample across tracks and network conditions; this comparison is observational, not a controlled benchmark.

## Catalog search

Catalog search and detail browsing use YouTube Music’s unofficial internal API. They do not consume official YouTube Data API quota and require no additional browser login. Category filters come from the service, including community and featured playlists; result ordering may differ from the YouTube Music website. Requests use the public English/US catalog context. Upstream changes can require compatibility updates.

Search waits 300 ms after typing. Successful responses are cached in memory for five minutes; thumbnails are cached locally. The most recent 20 distinct searches are stored on this device and can be removed individually. Missing metadata does not cause a per-result official API lookup.

Search replaces the main content view while preserving playback and the current queue. Clicking a playable result starts playback; opening a collection does not. Collection actions load remaining pages before changing the queue or saving tracks, and loading can be cancelled. Save actions use the existing remote-first playlist path: quota exhaustion or a partial remote failure is shown explicitly, and additions are never automatically retried.

Artist subscription state is fetched through the official API only when the artist detail page opens, with a five-minute account-scoped cache. Subscription changes, playlist mutations, and existing library refreshes continue to use official API quota.

Catalog detail pages follow the search design: album and playlist track tables, artist song and album sections, and navigable creator credits. **Add to library** saves a local snapshot of a discovered playlist; it does not subscribe to that playlist on YouTube. **Save to playlist** opens the existing destination picker. All search and filter inputs include an inline clear button only while nonempty.

Before the first search, the catalog shows category chips and the “Find something to play” panel. Opening this view does not fetch results. A category selected before typing is resolved from the service’s first search response. With saved history, the same view shows recent searches and Clear all; clearing history restores the first-use panel.
