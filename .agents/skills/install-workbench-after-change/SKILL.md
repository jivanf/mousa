---
name: install-workbench-after-change
description: Rebuild and reinstall the Mousa Workbench after repository changes and commits, or when asked to refresh the Workbench. Main-app installation requires an explicit user request.
---

# Update the installed Workbench

After each change set and every successful commit (including amendments and
documentation-only commits), rebuild and reinstall the Workbench before reporting
completion. Run from the current task's worktree root, not the primary checkout
or another task's worktree, and wait for completion:

- Windows: `powershell -ExecutionPolicy Bypass -File .\scripts\install-workbench.ps1`
- Linux: `bash ./scripts/install-workbench.sh`

These scripts build `cargo build --release --locked --example workbench`, install
the Workbench and its launcher entry, and verify the installed binary against the
release build. Windows installs to `%LOCALAPPDATA%\Programs\Mousa\Workbench`;
Linux installs to `$HOME/.local/bin/mousa-workbench`.

Close a running Workbench before replacing its executable. A requested Workbench
update authorizes closing and reopening that preview app. Do not close the main
Mousa app or interrupt playback. Preserve unrelated working-tree changes; the
installer builds the current checkout. Report build, installation, or hash-check
failures explicitly and resolve failures within the task's scope before retrying.
Compilation alone does not confirm successful installation. Use the installed
Workbench when reviewing UI, not an old process or a debug-only build.

Every UI change must have corresponding Workbench coverage for all affected
application states, using deterministic fixtures and isolated local interactions.
Review the changed states and check that search, selection, clearing, and other
applicable controls actually work in the previews.

## Main app: explicit request only

Do not run `scripts/install.ps1` without `-WorkbenchOnly`, or `scripts/install.sh`,
as routine post-change work. Reinstall the main Mousa app only when the user
explicitly asks. Then use the main installer for the current platform, preserve
playback unless closing/restarting was authorized, and verify the installed
binary. A previous task's installation request is not ongoing authorization.
