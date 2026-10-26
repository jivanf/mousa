# Mousa project instructions

## Changes and releases

- Work on a `codex/` branch, submit changes through a pull request, and enable squash auto-merge. Never push directly to `main`; Windows, Linux, and Release policy checks must pass. Use a Conventional Commit PR title, which becomes the squash commit title.
- Classify actual user impact: `fix` and `perf` produce patch releases; `feat` produces minor releases. Use `docs`, `test`, `ci`, `build`, `chore`, `refactor`, or `style` only when the change has no releasable behavior impact. Runtime dependency fixes are `fix`, not merely `chore`. A revert is classified by its resulting user impact.
- Mark incompatible changes with `!` in the title and an explanatory `BREAKING CHANGE: ...` footer in the PR body. Describe changes to saved data, configuration, updater compatibility, or supported platforms. Preserve compatibility with the previous release's rollback path; if that is impossible, explain the migration and recovery limits explicitly.
- Leave version bumps, changelog generation, release tags, and publication to release automation. Both packages share one version. Daily batches run at 17:00 UTC; urgent preparation uses the manual workflow and still requires all checks. Breaking releases and the 1.0.0 milestone require the repository owner's approval of the current release PR revision. Use `release/hold` to pause a release.
- Never move release tags or replace published assets. Retry a failed publication before preparing another release. Follow [release maintenance](docs/releases.md) for setup, previews, and recovery. Do not create a release solely for automation or documentation maintenance.

## Project requirements

- Only change `README.md` when the user explicitly requests a README change.
- Mousa supports both Windows and Linux. Ensure every change is compatible with both platforms. Gate unavoidable platform-specific code explicitly and provide equivalent behavior for the other supported platform; keep build, installation, release, and test workflows working on both operating systems.
- After every commit in this repository, including amendments and documentation-only commits, read and follow [install-mousa-after-commit](.agents/skills/install-mousa-after-commit/SKILL.md). Rebuild and update the installed application before reporting completion.

- Use button tooltips selectively, only when they clarify an otherwise ambiguous action. Do not add tooltips merely because a button uses an icon: universally understood controls such as Play, Pause, and the search input's clear (×) button need none. Avoid tooltips that simply repeat a visible label or an obvious action. Keep useful explanations for ambiguous actions or destinations, error details, and truncated content; preserve accessibility labels independently of tooltips.

- For UI changes, read `DESIGN.md` and the relevant approved mocks before editing. Keep `DESIGN.md` synchronized in the same change whenever tokens, dimensions, typography, component appearance, interaction states, navigation, or approved design exceptions change. Update its normative front matter and prose together, and keep `src/theme.rs` and shared components consistent with them. Add new design values to the specification and theme tokens rather than introducing undocumented one-off values. Follow final designer mocks; document user-authorized deviations explicitly instead of silently redesigning them.
- Verify changed UI states against the approved references using the relevant gallery or running app, and run the design-document lint when `DESIGN.md` changes.
