![Mousa logo](assets/mousa.svg)

# Mousa

A native Linux music player for YouTube.

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

## Limitations

Mousa currently supports Linux. Offline downloads, gapless playback guarantees, and YouTube Music recommendations are not available. YouTube playback may require yt-dlp updates when YouTube changes. Playlist edits require a connection and are not queued offline.
