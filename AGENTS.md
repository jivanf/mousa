# Workflow

## Required task workflow

Follow this workflow for every new repository task, including code, UI,
documentation, tooling, and maintenance work:

1. Create a new, dedicated Git worktree and branch before making changes.
   Do not work in the primary checkout or reuse another task's worktree. Start
   from current `main` unless the user specifies a different base or the task
   depends on unmerged work; make that dependency explicit and preserve
   unrelated changes.
2. Use the task's worktree for all edits, checks, commits, and Workbench builds
   and installation. Keep the same worktree and branch throughout the task.
3. Implement and verify the changes, update all affected Workbench states, commit
   locally, and, only for UI changes, rebuild and reinstall the Workbench following
   the installation skill. Present the result for review with any verification
   limits clearly stated.
4. Treat follow-up corrections, additions, and revisions as part of the same
   task. Continue in its existing worktree; repeat verification and commits, and
   rebuild/reinstall the Workbench only when the revision changes UI. Do not create
   a new worktree for each revision or treat the first implementation as approval
   to publish.
5. Once the user approves the completed changes, create the pull request. If the
   user explicitly preapproved PR creation at the start of the task, create it
   when the requested work and required verification are complete without asking
   again. Until then, keep changes local, including changes intended for an
   existing PR. Approval is task-specific, not blanket approval for future tasks.
6. Use a Conventional Commit PR title and enable squash auto-merge. Address CI
   failures and requested revisions in the same worktree and PR. Required Windows,
   Linux, and Release policy checks must pass; never bypass them or push directly
   to `main`. Verify the actual merge result before reporting the PR as merged.
7. After the PR is merged and no task work remains, delete its task worktree and
   prune stale worktree metadata using standard Git commands. Preserve any needed
   ignored artifacts first. Keep the worktree available while review, revisions,
   or merge checks are pending.

# Changes and releases

- Follow the required task workflow above for worktree isolation, user review, PR creation, and squash auto-merge. The Conventional Commit PR title becomes the squash commit title.
- Classify actual user impact: `fix` and `perf` produce patch releases; `feat` produces minor releases. Use `docs`, `test`, `ci`, `build`, `chore`, `refactor`, or `style` only when the change has no releasable behavior impact. Runtime dependency fixes are `fix`, not merely `chore`. A revert is classified by its resulting user impact.
- Mark incompatible changes with `!` in the title and an explanatory `BREAKING CHANGE: ...` footer in the PR body. Describe changes to saved data, configuration, updater compatibility, or supported platforms. Preserve compatibility with the previous release's rollback path; if that is impossible, explain the migration and recovery limits explicitly.
- Leave version bumps, changelog generation, release tags, and publication to release automation. Both packages share one version. Daily batches run at 17:00 UTC; urgent preparation uses the manual workflow and still requires all checks. Breaking releases and the 1.0.0 milestone require the repository owner's approval of the current release PR revision. Use `release/hold` to pause a release.
- Never move release tags or replace published assets. Retry a failed publication before preparing another release. Follow [release maintenance](docs/releases.md) for setup, previews, and recovery. Do not create a release solely for automation or documentation maintenance.

# Project requirements

- Only change `README.md` when the user explicitly requests a README change.
- Mousa supports both Windows and Linux. Ensure every change is compatible with both platforms. Gate unavoidable platform-specific code explicitly and provide equivalent behavior for the other supported platform; keep build, installation, release, and test workflows working on both operating systems.
- Rebuild, reinstall, and verify the Workbench only when UI changes are made, following [install-workbench-after-change](.agents/skills/install-workbench-after-change/SKILL.md). UI changes include application or Workbench appearance, interactions, components, theme tokens, assets, and preview states. Documentation, guidelines, skills, and other non-UI changes do not trigger a Workbench rebuild; a commit or amendment alone is not a trigger. Honor explicit user instructions to skip rebuilding. Reinstall the main Mousa app only when the user explicitly requests it; routine development and commits do not authorize a main-app reinstall.

- For UI changes, read `DESIGN.md` and the relevant approved mocks before editing. Keep `DESIGN.md` synchronized in the same change whenever tokens, dimensions, typography, component appearance, interaction states, navigation, or approved design exceptions change. Update its normative front matter and prose together, and keep `src/theme.rs` and shared components consistent with them. Add new design values to the specification and theme tokens rather than introducing undocumented one-off values. Follow final designer mocks; document user-authorized deviations explicitly instead of silently redesigning them.
- Reflect every UI change in the Workbench in the same change. The Workbench must expose every possible application UI state, including loading, empty, error/retry, success, disabled, selection, playback, focus/hover, dialogs, and navigation states. Add or update deterministic stories and working local interactions for all affected states; a standalone gallery alone is insufficient. Keep previews isolated from real accounts, network mutations, playback, and user library data.
- Verify changed UI states against the approved references in the Workbench, and run the design-document lint when `DESIGN.md` changes. Report any missing state coverage or verification blocker explicitly.

# UX

Read [UX guidelines](UX.md) before changing user-facing interactions, controls,
navigation, feedback, accessibility behavior, or other UX conventions.

# VCS

## Branches

- Branch names must follow the format `<type>/<n>`, where:
  - `<type>` is one of the commit types.
  - `<n>` is a brief, descriptive name summarizing the changes in the branch.

## Commit

- Always commit changes after they have been verified, before reporting the task complete.
- Verify with checks appropriate to the change: relevant Rust tests and builds for code, installed Workbench checks for every affected UI state, and the design-document lint when `DESIGN.md` changes. Do not treat failed or incomplete verification as success.
- Keep commits focused on one logical change. Review the staged diff and include only the task's changes; preserve unrelated working-tree and staged changes.
- Write meaningful Conventional Commits messages with an affected-area scope and a clear, concise description in the imperative mood (for example, "Add" or "Fix").
- Use backticks around code references and Rust notation where appropriate, such as `CatalogView`, `render()`, or `Player::new()`.
- Use the body when needed to explain the reason, behavior change, or verification. Include a `BREAKING CHANGE:` footer for incompatible changes, describing the impact and required migration.
- For commits that change UI, follow [install-workbench-after-change](.agents/skills/install-workbench-after-change/SKILL.md) to rebuild, reinstall, and verify the Workbench before reporting completion; report any installation blocker explicitly. Do not rebuild for non-UI commits or repeat a verified rebuild solely because a commit was created. Install the main app only on explicit user request.

### Format

```text
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

- The description starts with an uppercase letter and uses proper capitalization.
- The scope uses lowercase kebab case.

### Types

Only the following commit types are allowed:

- `fix`: Correct a real or potential bug, including regressions, playback failures, integration incompatibilities, security issues, or data-loss risks. Be able to describe the failure being corrected or prevented.
- `feat`: Add a new user-facing product feature. Internal helpers and developer-facing APIs alone are not features.
- `refactor`: Improve or restructure existing code or behavior without adding a user-facing feature or correcting a real or potential bug. Includes internal APIs and intentional behavior improvements when the previous behavior was valid.
- `quality`: Improve code quality or quality tooling, such as rustfmt, Clippy, or lint configuration.
- `perf`: Optimize runtime performance, responsiveness, memory use, or resource consumption.
- `test`: Add or change tests, fixtures, or verification utilities.
- `build`: Change Cargo dependencies, build configuration, native dependencies, or application packaging.
- `ci`: Change continuous-integration configuration or automation scripts used by CI or local development workflows.
- `docs`: Change documentation, contributor instructions, or skill instructions.
- `chore`: Perform routine maintenance or other non-functional cleanup not covered by another type.
- `ui`: Change presentation or interaction design, including layout, typography, theme tokens, icons, and component appearance. Rust changes in GPUI views and components are allowed. Use `feat` for new product capabilities and `fix` for defective behavior; do not classify every Rust change as `refactor`.

Prefer `fix` over `refactor` only when there is a concrete failure mode. Choose the type by the purpose of the change, not by the file extension.

### Scopes

Use the affected feature or subsystem, including for its related tests and documentation:

| Scope | Area |
| --- | --- |
| `catalog` | YouTube Music search, results, detail browsing, and catalog caching |
| `playback` | mpv playback, stream resolution and preparation, transport, and playback diagnostics |
| `queue` | Queue contents, ordering, metadata, and advancement |
| `playlists` | Playlist creation, editing, reordering, and track selection |
| `library` | Library browsing, filtering, and library-level operations |
| `sync` | YouTube refresh scheduling, quota policy, and synchronization |
| `storage` | SQLite schema, persistence, migrations, and saved view state |
| `accounts` | Authentication, credentials, and connected-account management |
| `lastfm` | Last.fm integration and scrobbling |
| `media-controls` | Linux MPRIS and Windows system media controls |
| `ui` | Shared GPUI components, theme tokens, assets, and design specification |
| `windows` | Windows-specific platform behavior and installation |
| `linux` | Linux-specific platform behavior and installation |
| `repo` | Repository-wide documentation, contributor instructions, and maintenance |

- Choose the narrowest scope that describes the logical change. A catalog layout change uses `catalog`; a shared button change uses `ui`.
- For tooling and maintenance, `quality`, `build`, and `ci` may use a tool or dependency scope such as `cargo`, `rustfmt`, `clippy`, `gpui`, `mpv`, `yt-dlp`, or `actions`.
- Add a new scope here when a new subsystem warrants one instead of inventing overlapping names for existing areas.

Examples:

```text
feat(catalog): Add artist subscriptions
fix(playback): Retry expired stream URLs
ui(catalog): Keep collection headers fixed while tracks scroll
refactor(queue): Extract metadata reconciliation
test(sync): Cover quota reset across daylight saving time
build(gpui): Update the pinned GPUI dependency
docs(repo): Define commit guidelines
```
