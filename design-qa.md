# Mousa design adoption QA

Final result: passed. Reviewed on Linux with GPUI 0.2.2, 2026-10-15.

## Review decision

Approved the supplied DESIGN.md and theme.rs with documented adaptations: a complete player outline matching the image, lime shadows on active toggles, layout tokens, and a Linux renderer workaround for hard shadows. Retained full track titles, variable-height virtualized rows, and opening the queue from now-playing information.

The player follows the reference composition and supplied token sizes. The 44px transport controls are intentionally smaller than a literal half-scale reference; the 72px play button remains dominant. The bundled Space Grotesk and Space Mono fonts follow the written specification.

## Evidence

- [Original reference](docs/player-reference.png): 2400 × 500.
- [Normalized comparison](docs/player-comparison.png): reference above implementation, each 1200 × 250, no selective cropping or scaling of components.
- [Player](docs/player.png): same title, artist, 0:14 / 4:00, volume 48%, repeat enabled.
- [Component gallery](docs/gallery.png): 1200 × 800; default, hover, pressed, focused, disabled, on, destructive, input, playlist and row states.
- [Keyboard seek, Tab focus and mute state](docs/player-keyboard.png).
- [Application](docs/application.png): 1280 × 840, isolated 5,000-entry synthetic library.
- [Settings form](docs/settings.png): minimum window size, contextual placeholders.
- [Compact application](docs/application-compact.png): 1100 × 700 minimum supported size.

Screenshots were captured from actual native windows on an isolated X11 display using software Vulkan. They contain no real account or playlist data.

## Findings resolved

1. Zero-blur shadows vanished in the pinned Linux renderer. A 0.01px sigma avoids its divide-by-zero while preserving visibly hard edges.
2. SVG icons did not inherit parent text colors. Explicit semantic tints now render ink and lime correctly.
3. Empty button labels introduced unwanted flex spacing. Icon and playlist controls now omit empty text nodes.
4. Input and player controls used inconsistent primitives. They now share tokens and native reusable components.
5. The settings form reused a playlist placeholder. Placeholders now describe each form's actual input.

6. Custom focusable controls were missing Tab stops. Explicit Tab order now includes shared buttons, rows, inputs and enabled bars.

## Interaction and validation

- Seek responds to pointer input, Home, End, and arrows. End displayed 4:00; Home followed by Right displayed 0:02 for the 240-second fixture.
- Tab moves from seeking to mute; Enter activates mute and displays 0%. Play changes to the pause icon. Active repeat uses an ink face, lime icon and lime shadow.
- Real app volume was checked through MPRIS: End = 1.00, Left = 0.95, mute = 0, unmute = 0.95. Pointer dragging also changed volume.
- Row actions appear on hover or individual focus. Full titles continue to wrap in virtualized rows.
- Compact layout preserves all seven transport buttons, seeking, volume, and metadata without horizontal overflow.
- The settings form was visually inspected at minimum size.
- Google design.md lint: zero errors and warnings.
- Rust formatting and Clippy with warnings denied pass. Library tests: nine pass; optional 100,000-entry stress test remains ignored.

## Scope of verification

This review validates the Linux UI and existing library tests. It does not claim cross-platform rendering, screen-reader certification, or a new live Google/Last.fm integration test. Network and storage behavior were not changed by the design adoption.
