---
version: alpha
name: Mousa
description: Neobrutalist native desktop music player. Rust, GPUI, Linux first. Not a webview.
colors:
  primary: "#111111"
  secondary: "#3B3B3B"
  tertiary: "#C8F560"
  neutral: "#FBF7EE"
  canvas: "#E6DFD0"
  field: "#FFFFFF"
  danger: "#FF8FC7"
  on-primary: "#C8F560"
  on-tertiary: "#111111"
  on-danger: "#111111"
typography:
  page-title:
    fontFamily: Space Grotesk
    fontSize: 28px
    fontWeight: 500
  track-title:
    fontFamily: Space Grotesk
    fontSize: 18px
    fontWeight: 700
  body-md:
    fontFamily: Space Grotesk
    fontSize: 14px
    fontWeight: 500
  body-sm:
    fontFamily: Space Grotesk
    fontSize: 13px
    fontWeight: 500
  label-caps:
    fontFamily: Space Mono
    fontSize: 12px
    fontWeight: 700
    letterSpacing: 0.08em
  mono-md:
    fontFamily: Space Mono
    fontSize: 14px
    fontWeight: 700
rounded:
  none: 0px
spacing:
  xs: 4px
  sm: 8px
  md: 12px
  lg: 16px
  xl: 24px
  xxl: 32px
  border-strong: 3px
  border-hairline: 1px
x-shadows:
  sm: "3px 3px 0 0 {colors.primary}"
  md: "5px 5px 0 0 {colors.primary}"
  lg: "8px 8px 0 0 {colors.primary}"
  on: "3px 3px 0 0 {colors.tertiary}"
  focus-ring: "0 0 0 3px {colors.primary}"
components:
  player-center:
    width: 512px
  player-center-compact:
    width: 432px
  player-volume:
    width: 224px
  player-breakpoint:
    width: 1200px
  player-inset:
    padding: 32px
  time-label:
    width: 48px
  icon:
    size: 24px
  hero-icon:
    size: 32px
  min-window:
    width: 1100px
    height: 700px
  default-window:
    width: 1280px
    height: 840px
  button:
    backgroundColor: "{colors.neutral}"
    textColor: "{colors.primary}"
    typography: "{typography.mono-md}"
    rounded: "{rounded.none}"
    height: 36px
    padding: 12px
  button-hover:
    backgroundColor: "{colors.field}"
    textColor: "{colors.primary}"
  button-primary:
    backgroundColor: "{colors.tertiary}"
    textColor: "{colors.on-tertiary}"
  button-toggle-on:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
  button-destructive:
    backgroundColor: "{colors.danger}"
    textColor: "{colors.on-danger}"
  button-row-action:
    backgroundColor: "{colors.neutral}"
    textColor: "{colors.primary}"
    height: 32px
    width: 32px
  button-transport:
    backgroundColor: "{colors.neutral}"
    textColor: "{colors.primary}"
    height: 44px
    width: 44px
  button-hero:
    backgroundColor: "{colors.tertiary}"
    textColor: "{colors.on-tertiary}"
    height: 72px
    width: 72px
  input:
    backgroundColor: "{colors.field}"
    textColor: "{colors.primary}"
    typography: "{typography.body-md}"
    height: 36px
  bar-track:
    backgroundColor: "{colors.field}"
    height: 22px
  bar-fill:
    backgroundColor: "{colors.tertiary}"
  bar-thumb:
    backgroundColor: "{colors.field}"
    height: 28px
    width: 28px
  track-row:
    backgroundColor: "{colors.neutral}"
    textColor: "{colors.primary}"
    typography: "{typography.body-md}"
    height: 56px
  track-row-meta:
    backgroundColor: "{colors.neutral}"
    textColor: "{colors.secondary}"
    typography: "{typography.body-sm}"
  track-row-playing:
    backgroundColor: "{colors.tertiary}"
    textColor: "{colors.on-tertiary}"
  track-row-selected:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
  playlist-item:
    backgroundColor: "{colors.neutral}"
    textColor: "{colors.primary}"
    typography: "{typography.body-md}"
  playlist-item-selected:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
  sidebar:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.primary}"
    width: 260px
  player-bar:
    backgroundColor: "{colors.neutral}"
    textColor: "{colors.primary}"
---

## Overview

Mousa is a native Rust desktop app built on GPUI, currently targeting Linux.
Nothing in this file is HTML or CSS: components are GPUI elements (`div()`,
`svg()`, virtualized `list`, stateful elements with `.id()`). Token values in the
front matter are normative. The Rust constants in `theme.rs` implement them
and must match.

The look is neobrutalist: hard edges, heavy ink outlines, offset shadows,
flat color. It is a library tool with 1000+ track lists, so it stays dense
and quiet by default.

Principles:

1. Hard edges. Zero radius, zero blur, no gradients, no transparency tricks.
2. Heavy 3px ink outlines on anything interactive. 1px hairlines only as
   dividers inside lists.
3. One hero per view. In the player it is the play button.
4. Color means something. Lime and pink are never decoration.
5. Prefer quiet rows with actions revealed on hover over repeated buttons.

Read this file and use only tokens from it. Check every change in the
gallery (`cargo run --example gallery`). A mockup, if one exists, shows the
look only. Do not port its markup.

## Colors

- **Primary (#111111), "ink":** text, borders, shadows, and inverted fills.
- **Secondary (#3B3B3B), "muted":** secondary text such as artist names and
  captions. Passes 4.5:1 on neutral.
- **Tertiary (#C8F560), "lime":** playing or primary. The currently playing
  row, the play button, bar fills, and at most one primary action per view.
- **Neutral (#FBF7EE), "surface":** resting fill for buttons, cards, rows,
  the player bar.
- **Canvas (#E6DFD0):** window and panel background, including the sidebar.
- **Field (#FFFFFF):** inputs, bar tracks, thumbs, and the hover fill.
- **Danger (#FF8FC7), pink:** destructive actions only (Delete, Remove).
  This replaces the original prototype pink.
- **On-primary (#C8F560):** text and icons on an ink fill.

On / selected is expressed by inversion: ink fill with lime text. Used for
the selected playlist, selected row, active shuffle or repeat. Never use pink
for selection.

Drift to fix in the current app: the sidebar background, every playlist card
and several buttons are lime, so lime no longer signals anything. Sidebar
becomes canvas, playlist cards neutral, the selected playlist inverted.

## Typography

Space Grotesk for text (500 and 700 only) and Space Mono for labels, times,
and numbers (400 and 700). Fonts are embedded and loaded through the app's
`AssetSource` at startup. Never depend on system fonts.

- **page-title:** page headers, light and large.
- **track-title:** the now-playing title in the player bar.
- **body-md / body-sm:** list text and secondary text. Track titles wrap in variable-height
  virtualized rows to preserve Mousa’s existing full-title behavior; artist
  names and playlist cards truncate with ellipsis.
- **label-caps:** small uppercase labels such as NOW PLAYING.
- **mono-md:** button labels, times, indices.

Icons are monochrome SVGs embedded as assets on a 24px grid with a 2.5
stroke and round joins, tinted with the element's text color. No unicode
glyphs as icons (the current arrow and cross buttons depend on font
coverage) and no emoji.

## Layout

Desktop and mouse first, so controls are denser than touch targets.
Spacing scale: xs 4, sm 8, md 12, lg 16, xl 24, xxl 32. Do not use other
values.

Control sizes: row actions 32px, default controls and inputs 36px, transport
buttons 44px, hero 72px, bar track 22px, bar thumb 28px, track row minimum 56px,
sidebar 260px.

Player bar, docked at the bottom, three zones: track info (left), transport
and timeline (center), volume (right). The timeline sits directly under the
transport row with times beside the bar. Transport order: shuffle, previous,
back 10, play, forward 10, next, repeat.

Long lists use GPUI’s virtualized `list` and `ListState`. Only visible rows are rendered. Row height starts at 56px and grows for complete titles; this intentionally preserves the existing application behavior.

## Elevation & Depth

Depth is created only by hard offset shadows: sm (3px) for buttons, md (5px)
for the hero control, lg (8px) for cards, dialogs, and the player bar. Blur is
visually zero. The color is ink, except the active toggle shadow, which is lime.

The pinned GPUI Linux renderer has no zero-sigma branch in its shadow shader.
`theme.rs` uses a 0.01px raster sigma on Linux to avoid division by zero while
rendering a visually hard edge. This is a renderer workaround, not a blur token.

Shadows paint outside the element. Reserve margin equal to the offset, and do
not put shadowed controls inside an `overflow_hidden` or scroll container
without padding, or the shadow is clipped. Rows in lists and items in the
sidebar carry no shadow.

Pressed state: GPUI has no transform. In `.active(...)`, offset the element
by the shadow offset (top and left) and drop the shadow, so it looks pushed
in. Keyboard focus adds the focus ring (3px ink, outside the border) as an
extra shadow. Every interactive element must be focusable and show it.

## Shapes

Radius is always zero: use `.rounded_none()`. Borders are 3px ink on buttons,
cards, bars, and inputs. Row dividers are 1px ink. No other border widths.

## Components

**Button.** Neutral fill, 3px ink border, sm shadow, 36px high, mono label
(uppercase for text buttons). Hover: field fill. Pressed: offset, no shadow.
Disabled: canvas fill, secondary text, no shadow.

**Primary button.** Tertiary fill. At most one per view.

**Toggle button** (shuffle, repeat, nav). Off is a Button. On is inverted:
primary fill, on-primary icon, shadow in lime. The inversion is the signal,
so state never depends on color alone.

**Destructive button.** Danger fill. Delete and remove only.

**Bar** (progress and volume, one component). 22px track with 3px ink border
and field fill; lime fill ending in a 3px ink edge; 28px square thumb with
sm shadow centered on the value. One custom element taking a 0..1 value and
emitting changes on mouse down and drag, including release outside its bounds. Clicking the track seeks.
Arrow keys adjust, Home/End jump to bounds. Disabled progress controls do not
receive keyboard focus. GPUI itself has no built-in slider; the existing
component dependency has one, but Mousa uses a custom Bar for this geometry.

**Transport row.** 44px buttons with md gaps. Play is the 72px hero in
tertiary with the md shadow.

**Track row.** Minimum 56px high, neutral, 1px divider below. Index in mono at left,
title (body-md) over artist (body-sm, secondary), duration in mono at right.
Actions (queue, move up, move down, remove) are 32px icon buttons revealed on
row hover or individual keyboard focus (`.group("row")` and `.group_hover("row", ...)`).
Playing row is tertiary. Selected row is inverted.

**Playlist item.** Neutral card with 3px border: name over a mono label of
`count . source`. Selected is inverted. No shadow.

**Text input.** Field fill, 3px ink border, 36px high, placeholder in
secondary, focus ring on focus.

**Player bar.** Neutral, a complete 3px outline, 8px offset shadow, 32px
outer side/bottom inset and 24px inner padding, matching the supplied visual.
Three zones as described in Layout. Center zone is 512px at widths of at least
1200px and 432px below that breakpoint; volume is 224px. Track info wraps.
Clicking track info opens the queue at the current track. Mute restores the
last audible volume; it does not reset volume to a fixed level.
Volume reuses Bar plus a mute button.

**Page header.** page-title in ink, a secondary metadata line, and actions at
top right (Rename default, Delete destructive).

## Do's and Don'ts

| Do | Don't |
|---|---|
| Use tokens from this file via `theme.rs` | Hardcode hex values, px sizes or shadows |
| One lime primary action per view | Fill whole panels with lime |
| Invert for "on" and "selected" | Use pink for selection |
| Reveal row actions on hover | Repeat four buttons on every row |
| Embed fonts and icons as assets | Rely on system fonts or unicode glyphs |
| Keep shadows on controls only | Put shadows on rows or scroll content |
| Add a tooltip and keyboard action to icon-only buttons | Rely on a label attribute, GPUI has none |
| Add a token here before using a new value | Introduce one-off values in components |

## GPUI Implementation

- `theme.rs` exposes each token as a function returning `Hsla` or `Pixels`
  (`BoxShadow` for shadows). It mirrors the front matter above. When they
  disagree, the front matter wins and `theme.rs` is a bug.
- Border widths are `border-strong` and `border-hairline` in `spacing`, because
  the format has no border token type. `x-shadows` is an extension key for the
  same reason: `lint` accepts it, but export commands ignore it.
- Components live in one module. Build `Button`, `Bar`, `TrackRow` and the
  rest once, and reuse them. Do not restyle ad hoc inside screens.

## Adoption review

Accepted the designer palette, type hierarchy, control sizes and semantic
color use. Corrected the player border to match the attached reference,
added the missing active-toggle shadow, and documented the pinned-renderer
workaround. Preserved recent full-title wrapping and queue navigation.
The 44px transport buttons follow the supplied tokens; the reference is a
visual direction, not a requirement to scale every control with window size.
Fonts and Tabler SVGs are bundled with their licenses. Label tracking is
implemented by the shared `caps` component because GPUI 0.2.2 lacks a text
letter-spacing property. Keyboard activation uses native GPUI Enter/Space
handling; all icon actions also have descriptive tooltips.

## Verification

1. Run `npx @google/design.md lint DESIGN.md`. It must report no errors.
2. Add or update the component in `examples/gallery.rs` in every state:
   default, hover, pressed, focused, disabled, on.
3. Run the gallery and compare against this file, not against memory.
4. Commit gallery screenshots to `docs/` so reviewers can see the result
   without running the app.
