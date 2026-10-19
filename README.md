![Mousa logo](assets/mousa.svg)

# Mousa

A native Linux music player for YouTube, with fast playlist browsing, a customizable queue, and a neobrutalist interface.

## Features

- Create, rename, delete, and reorder local and YouTube playlists.
- Search playlists, track titles, and artists.
- Play a search result and continue from that entry in the full playlist.
- Add tracks or entire playlists from YouTube URLs.
- Play/pause, previous/next, seeking, volume, shuffle, and repeat.
- Manage your queue and restore it across restarts. Playback starts paused.
- Resume the last playlist or queue view, including per-playlist searches and scroll positions.
- Connect Google to see your owned YouTube playlists automatically.
- Browse cached playlists offline; YouTube playback and edits require a connection.
- Use Linux desktop media controls through MPRIS.
- Connect Last.fm for now-playing updates and scrobbling.
- Store account credentials in your operating system's keyring.

Changes to YouTube playlists are saved to your YouTube account. Deleting one in Mousa also deletes it on YouTube and requires confirmation. Local playlists stay on your device. Some YouTube Music collections and playlists saved from other owners may not be available.

## Install and run

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
- Ctrl+F: focus track search
- Tab / Shift+Tab: move between controls
- Enter / Space: activate a focused button
- Arrow keys: adjust a focused seek or volume slider
- Home / End: move a focused slider to its minimum/maximum

## Connect Google

Create a Google Cloud project, enable the **YouTube Data API v3**, and create a **Desktop app** OAuth client in Google Auth Platform. Configure the consent audience and add your account as a test user if the app is in Testing.

Save the downloaded client JSON at:

```text
~/.local/share/mousa/google-client.json
```

Choose **Sign in with Google** on the landing page and complete authorization in your browser. Your owned playlists appear automatically and refresh every ten minutes. A Google account is required to enter the library.

The sidebar displays your full Google profile name. Hover the account button to reveal **Sign out**; signing out removes the local Google credentials, stops playback, and returns to the landing page. Cached playlists remain on this device. Existing installations need to sign in once more to grant profile access.

Mousa requests your basic Google profile and permission to manage YouTube playlists so you can edit them. If you previously granted read-only access, reconnect to enable editing. Google authorization is separate from video playback and does not sign yt-dlp into your account.

Keep the downloaded client file and account credentials private; do not commit them to a repository.

## Connect Last.fm

Choose **Connect Last.fm**, then approve Mousa in your browser. Mousa completes the desktop token exchange automatically and stores the session in the system keyring. Sign-in times out after three minutes and can be retried. The sidebar then displays your Last.fm username; hover to reveal **Disconnect**. Disconnecting deletes the saved session and queued scrobbles. To revoke Mousa's grant on Last.fm itself, remove it from [Last.fm application settings](https://www.last.fm/settings/applications).

For application maintainers: [desktop authentication](https://www.last.fm/api/desktopauth) requires a registered application key and shared secret. Provision these once for the installation in `~/.local/share/mousa/lastfm-client.json` with fields `api_key` and `shared_secret`, and restrict the file to its owner (`chmod 600`). These are application credentials, not fields users enter to connect their accounts. Legacy application credentials already stored in the keyring remain supported. Do not commit credentials.

Scrobbling starts after authorization. Video metadata may differ from Last.fm's artist and track names.

## Limitations

Mousa currently supports Linux. Offline downloads, gapless playback guarantees, and YouTube Music recommendations are not available. YouTube playback may require yt-dlp updates when YouTube changes. Playlist edits require a connection and are not queued offline.

## YouTube refresh policy

The library catalog refreshes at most hourly. Playlist contents load when first opened; unused playlists are never downloaded just to support search, which is playlist-specific. Cached tracks remain immediately available while refreshes run.

Opened or playing playlists are checked for freshness every ten minutes. Their content refresh interval starts at one hour, doubles after unchanged snapshots, and caps at 24 hours. A detected change resets it to one hour. Restarts preserve freshness. The **Refresh** button bypasses this interval for the selected playlist; duplicate requests within five seconds are coalesced.

A playlist visited at least three times (visits separated by ten minutes), most recently within seven days, qualifies for opportunistic background refresh once daily. One candidate is considered per scheduler pass. Automatic API work shares a local budget of 2,000 estimated units per Pacific calendar day, reserving capacity for opening playlists, manual refreshes, and edits. This local estimate does not measure other installations using the same Google project.

Video durations persist for seven days; missing/unavailable metadata is retried after six hours. Successful batches survive later request failures. The quota-exhausted state persists until midnight America/Los_Angeles, including daylight-saving changes. While paused, cached data remains usable; YouTube edits require the quota to recover. Per-endpoint estimated usage is recorded locally, without credentials or request bodies.

On normal exit, Mousa saves browsing context and window size, position, and maximized state. Window placement follows desktop/compositor support; unavailable display positions fall back to centering. Menus, forms, unsaved form input, errors, and pending account actions reopen closed. If the saved playlist is missing, Mousa returns to the available library.

### Streaming preparation and playback measurements

Mousa prepares **one next-track stream URL when 30 seconds or less remain**, once the current track has loaded and playback is not paused. Short tracks prepare immediately; tracks with unknown duration wait for a valid duration. This resolves a URL with yt-dlp in the background; it does not prebuffer the next track's audio or save media files. The current stream continues using mpv's normal buffering.

On startup, a signed-in session also prepares its **saved current track's URL while remaining paused**. An empty queue or an explicitly stopped session does not trigger preparation. Pressing Play consumes that result if it is ready and fresh; otherwise normal streaming begins immediately without waiting for the background job. Startup preparation runs once, does not refresh repeatedly while idle, and never loads audio into mpv or changes the saved playback position. Selecting another track, replacing/removing queue entries, or stopping cancels it. Volume, pause, seek, shuffle/repeat changes, and appending tracks preserve it because the selected current entry is unchanged. This result is separate from the next-track choice, so pressing Next cannot accidentally reuse the current track's URL.

Shuffle retains its chosen next entry for both preparation and advancement. Queue replacement, appends/removals, jumps, Previous, Stop, and changes to shuffle/repeat cancel the old preparation. Late results cannot overwrite the replacement. Pause and seek retain the choice; URLs older than two minutes are refreshed when playback re-enters the preparation window. URLs approaching their advertised expiry are not used. Preparation has a 25-second timeout; a pending/failed preparation never delays advancing. Failed direct playback falls back once to resolving the original YouTube URL through mpv. Prepared streams also retry the original URL if playback has not started within 15 seconds. Normal resolution/retries have a 45-second startup deadline; if that expires, playback stops with a retryable error rather than remaining at 0:00. Deliberate pauses suspend the deadline. Prepared loads keep mpv's YouTube helper running instead of unloading/reloading it between tracks. Required HTTP headers remain in memory and apply only to that file. Direct prepared loading uses mpv 0.38+'s per-file `loadfile` options; unsupported command syntax falls back to normal streaming.

Diagnostics are written asynchronously to `~/.local/share/mousa/playback.jsonl` (or the app's XDG/`MOUSA_DATA_DIR` data directory). Rotation retains the current file and `playback.jsonl.1`, each at most 1 MiB. They contain timestamps, process/playback IDs, queue indexes, preparation outcomes/durations, load-to-first-progress timing, file-loaded milestones, startup timeouts and retry reasons, and cache-wait counts/durations. They do not contain track titles, source/stream URLs, HTTP headers, credentials, or raw resolver errors. Writes are best-effort and may be dropped if the bounded log queue fills; logging never blocks playback.

After listening to several normally advancing tracks, summarize the logs with:

```sh
python3 scripts/playback-report.py
# Optional: python3 scripts/playback-report.py /path/to/mousa-data
```

The report separates `origin=resume` (pressing Play on an unloaded saved/current track) from `origin=automatic` (end-of-track advancement). Preparation events label their purpose as `startup` or `next`. Resume timing begins when the playback worker handles Play and includes seeking to the saved position; it does not include time spent idle before Play. An expected saved-position seek is not treated as a user interruption.

Compare `preparation=ready` automatic transitions with `pending`, `failed`, `expired`, or `not_requested`. `fallback` means a prepared load failed and normal resolution was retried. For automatic transitions, `first_progress_ms` measures from the observed end event to advancing playback position on the next track; `file_loaded_ms` and `playback_restart_ms` distinguish opening from playback readiness. User pauses/seeks during startup are marked `interrupted` and excluded from the report. Buffering before first progress is separate from stalls during playback. The playback loop samples at a nominal 50 ms interval; these values approximate audible gaps, not hardware output latency, and very short cache waits can be missed.

If prepared transitions are consistently quick and stall-free, further audio prebuffering is unlikely to help much. Persistent delays above roughly 200–300 ms **with cache waits**, or frequent playback stalls, justify a prebuffering experiment. URL-resolution failures or slow preparation call for resolver improvements first. Use a reasonable sample across tracks and network conditions; this comparison is observational, not a controlled benchmark.
