# Mousa project instructions

- Only change `README.md` when the user explicitly requests a README change.
- Mousa supports both Windows and Linux. Ensure every change is compatible with both platforms. Gate unavoidable platform-specific code explicitly and provide equivalent behavior for the other supported platform; keep build, installation, release, and test workflows working on both operating systems.
- After every commit in this repository, including amendments and documentation-only commits, read and follow [install-mousa-after-commit](.agents/skills/install-mousa-after-commit/SKILL.md). Rebuild and update the installed application before reporting completion.

- Use button tooltips selectively, only when they clarify an otherwise ambiguous action. Do not add tooltips merely because a button uses an icon: universally understood controls such as Play, Pause, and the search input's clear (×) button need none. Avoid tooltips that simply repeat a visible label or an obvious action. Keep useful explanations for ambiguous actions or destinations, error details, and truncated content; preserve accessibility labels independently of tooltips.

- For UI changes, read `DESIGN.md` and the relevant approved mocks before editing. Keep `DESIGN.md` synchronized in the same change whenever tokens, dimensions, typography, component appearance, interaction states, navigation, or approved design exceptions change. Update its normative front matter and prose together, and keep `src/theme.rs` and shared components consistent with them. Add new design values to the specification and theme tokens rather than introducing undocumented one-off values. Follow final designer mocks; document user-authorized deviations explicitly instead of silently redesigning them.
- Verify changed UI states against the approved references using the relevant gallery or running app, and run the design-document lint when `DESIGN.md` changes.
