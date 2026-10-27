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
