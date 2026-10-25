# Mousa project instructions

- After every commit in this repository, including amendments and documentation-only commits, read and follow [install-mousa-after-commit](.agents/skills/install-mousa-after-commit/SKILL.md). Rebuild and update the installed application before reporting completion.

- Use button tooltips selectively, only when they clarify an otherwise ambiguous action. Do not add tooltips merely because a button uses an icon: universally understood controls such as Play, Pause, and the search input's clear (×) button need none. Avoid tooltips that simply repeat a visible label or an obvious action. Keep useful explanations for ambiguous actions or destinations, error details, and truncated content; preserve accessibility labels independently of tooltips.

- For UI changes, read `DESIGN.md` and the relevant approved mocks before editing. Keep `DESIGN.md` synchronized in the same change whenever tokens, dimensions, typography, component appearance, interaction states, navigation, or approved design exceptions change. Update its normative front matter and prose together, and keep `src/theme.rs` and shared components consistent with them. Add new design values to the specification and theme tokens rather than introducing undocumented one-off values. Follow final designer mocks; document user-authorized deviations explicitly instead of silently redesigning them.
- Verify changed UI states against the approved references using the relevant gallery or running app, and run the design-document lint when `DESIGN.md` changes.

<vcs description="Guidelines for version control in Mousa.">

## Commit

- Always commit changes after they have been verified, before reporting the task complete.
- Verify with checks appropriate to the change: relevant Rust tests and builds for code, gallery or running-app checks for UI, and the design-document lint when `DESIGN.md` changes. Do not treat failed or incomplete verification as success.
- Keep commits focused on one logical change. Review the staged diff and include only the task's changes; preserve unrelated working-tree and staged changes.
- Write meaningful Conventional Commits messages with an affected-area scope and a clear, concise description in the imperative mood (for example, "Add" or "Fix").
- Use backticks around code references and Rust notation where appropriate, such as `CatalogView`, `render()`, or `Player::new()`.
- Use the body when needed to explain the reason, behavior change, or verification. Include a `BREAKING CHANGE:` footer for incompatible changes, describing the impact and required migration.
- After every commit, including amendments and documentation-only commits, follow [install-mousa-after-commit](.agents/skills/install-mousa-after-commit/SKILL.md). Rebuild, update, and verify the installed application before reporting completion; report any installation blocker explicitly.

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

</vcs>
