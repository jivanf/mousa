---
name: install-mousa-after-commit
description: Rebuild Mousa and update its locally installed application after every commit made in this repository, including documentation-only commits. Also use when asked to refresh the installed Mousa build.
---

# Update the installed Mousa build

After every successful commit in this repository, run `./scripts/install.sh`
on Linux or `powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1`
on Windows from the repository root and wait for it to finish before reporting completion.
Repeat this after any follow-up or amended commit. This applies even when the
commit changes only documentation or skills.

Both installers build with `cargo build --release --locked`. On Windows, the
installer installs `mousa.exe` and its runtime dependencies in
`%LOCALAPPDATA%\Programs\Mousa`, creates a Start menu shortcut, and verifies the
installed executable's SHA-256 against the release build. Close the running
Windows app before replacing its executable; preserve playback unless the user
has authorized a restart.

On Linux, the installer installs the binary
at `$HOME/.local/bin/mousa`, and updates the desktop launcher and icon. Building
or launching `target/debug/mousa` alone does not update the app-menu version.

Verify the installed binary matches the release build:

```sh
cmp target/release/mousa "$HOME/.local/bin/mousa"
```

Preserve unrelated working-tree changes. The installer builds the current
checkout; do not discard or stash other work to make it match the commit.

If building, installing, or verification fails, investigate and fix errors within
the task's scope, then retry. If blocked, report the failure and that the installed
app has not been verified as current. Never report installation success based on
compilation alone.

Updating the installed file does not update a running process. Do not interrupt
playback or restart Mousa solely because of a commit. When the user asks to launch
or restart it, use `$HOME/.local/bin/mousa` on Linux or
`%LOCALAPPDATA%\Programs\Mousa\mousa.exe` on Windows so they test the installed version.
