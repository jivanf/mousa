---
version: alpha
name: Mousa
description: Neobrutalist native desktop music player. Rust, GPUI, Linux first. Not a webview.
x-workbench-navigation: searchable-grouped-stories
x-workbench-persistence: story-filter-viewport-mode
colors:
  primary: "#111111"
  secondary: "#3B3B3B"
  tertiary: "#C8F560"
  neutral: "#FBF7EE"
  canvas: "#E6DFD0"
  field: "#FFFFFF"
  danger: "#FF8FC7"
  scrim: "rgba(17, 17, 17, 0.55)"
  on-primary: "#C8F560"
  on-tertiary: "#111111"
  on-danger: "#111111"
typography:
  playlist-title-input:
    fontFamily: Space Grotesk
    fontSize: 16px
    fontWeight: 500
  landing-title:
    fontFamily: Space Grotesk
    fontSize: 56px
    fontWeight: 700
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
  catalog-control:
    fontFamily: Space Mono
    fontSize: 13px
    fontWeight: 700
  catalog-number:
    fontFamily: Space Mono
    fontSize: 14px
    fontWeight: 400
  catalog-query:
    fontFamily: Space Grotesk
    fontSize: 18px
    fontWeight: 500
  mono-md:
    fontFamily: Space Mono
    fontSize: 14px
    fontWeight: 700
rounded:
  none: 0px
spacing:
  playlist-editor-gap: 20px
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
x-motion:
  button-press-duration: 150ms
  button-release-duration: 150ms
  button-easing: smoothstep
  button-trigger: click
x-alignment:
  playlist-editor-character-counters: right
x-scrolling:
  scrollbar-gutter: overflow-only
  catalog-collection-detail: table-with-fixed-header-and-selection
  catalog-artist-detail: page
  pagination: automatic-virtual-scroll
x-session:
  catalog: preserve-query-page-history-filter-and-scroll
x-search-navigation:
  video: uploader-channel-detail
  channel-search: case-insensitive-video-and-playlist-titles
components:
  workbench-sidebar:
    width: 272px
    padding: 8px
  workbench-preview:
    width: 1280px
    height: 840px
  workbench-preview-compact:
    width: 1100px
  workbench-toolbar:
    padding: 16px
  feedback-alert:
    width: 100%
    padding: 12px
    textColor: "{colors.primary}"
    typography: "{typography.body-md}"
  feedback-alert-success:
    backgroundColor: "{colors.tertiary}"
  feedback-alert-info:
    backgroundColor: "{colors.neutral}"
  playlist-editor:
    width: 520px
    padding: 24px
  modal-backdrop:
    backgroundColor: "{colors.scrim}"
  playlist-editor-header:
    height: 56px
  playlist-editor-title:
    height: 44px
    padding: 14px
    typography: "{typography.playlist-title-input}"
  playlist-editor-description:
    height: 150px
    padding: 10px
  playlist-editor-counter:
    height: 22px
  playlist-editor-icon:
    size: 16px
  playlist-editor-close-icon:
    size: 18px
  landing-card:
    width: 520px
    padding: 48px
    typography: "{typography.landing-title}"
  landing-logo:
    size: 100px
  player-center:
    width: 512px
  player-center-compact:
    width: 432px
  player-volume:
    width: 224px
  player-breakpoint:
    width: 1200px
  player-inset:
    padding: 16px
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
  selection-bar:
    height: 66px
    backgroundColor: "{colors.canvas}"
    padding: 16px
  selection-bar-unavailable:
    height: 90px
  selection-summary:
    width: 128px
  selection-count:
    height: 36px
  track-row-selected:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.primary}"
    borderLeftWidth: 6px
  track-scrollbar:
    width: 20px
  track-scrollbar-thumb:
    height: 48px
  track-scrollbar-readout:
    height: 30px
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
    width: 272px
  catalog-nav:
    height: 40px
    gap: 10px
  catalog-search:
    height: 56px
    typography: "{typography.catalog-query}"
    padding: 16px
  catalog-filter:
    height: 36px
    gap: 12px
    typography: "{typography.catalog-control}"
  catalog-result:
    height: 64px
    x-interaction: navigation-only
    x-multi-selection: false
  catalog-thumbnail:
    size: 48px
  catalog-result-trailing:
    width: 84px
  catalog-icon:
    size: 18px
  catalog-detail:
    padding: 32px
    gap: 32px
  catalog-cover:
    size: 192px
  catalog-portrait:
    size: 160px
  catalog-album-card:
    width: 176px
    gap: 24px
  catalog-action:
    height: 44px
    padding: 18px
    gap: 10px
    typography: "{typography.catalog-control}"
  catalog-track:
    height: 56px
  catalog-track-number:
    width: 56px
    typography: "{typography.catalog-number}"
  catalog-track-duration:
    width: 60px
    typography: "{typography.catalog-number}"
  catalog-recent:
    height: 48px
  catalog-state:
    padding: 20px
  catalog-first-use:
    padding: 24px
    gap: 20px
  catalog-first-use-icon-frame:
    size: 56px
  catalog-first-use-icon:
    size: 28px
  catalog-first-use-copy:
    width: 560px
  catalog-skeleton-title:
    height: 12px
  catalog-skeleton-subtitle:
    height: 10px
  player-bar:
    backgroundColor: "{colors.neutral}"
    textColor: "{colors.primary}"
---

## Overview

Mousa is a native Rust desktop app built on GPUI, targeting Linux and Windows.
Nothing in this file is HTML or CSS: components are GPUI elements (`div()`,
`svg()`, virtualized `list`, stateful elements with `.id()`). Token values in the
front matter are normative. The Rust constants in `theme.rs` implement them
and must match.

The look is neobrutalist: hard edges, heavy ink outlines, offset shadows,
flat color. It is a library tool with 1000+ track lists, so it stays dense
and quiet by default.

Principles:

1. Hard edges. Zero radius, zero blur, no gradients, no transparency tricks.
   The functional modal scrim is the sole translucent surface exception.
2. Heavy 3px ink outlines on anything interactive. 1px hairlines only as
   dividers inside lists.
3. One hero per view. In the player it is the play button.
4. Color means something. Lime and pink are never decoration.
5. Prefer quiet rows with actions revealed on hover over repeated buttons.

Read this file and use only tokens from it. Check every change in the
gallery (`cargo run --example gallery`). Approved designer mocks define the visual target. Implement them with native
components rather than porting their markup. Record user-approved exceptions here;
do not reinterpret a final design without authorization.

## Colors

- **Primary (#111111), "ink":** text, borders, shadows, and inverted fills.
- **Secondary (#3B3B3B), "muted":** secondary text such as artist names and
  captions. Passes 4.5:1 on neutral.
- **Tertiary (#C8F560), "lime":** playing or primary. The currently playing
  row, the play button, bar fills, success alerts, and at most one primary action per view.
- **Neutral (#FBF7EE), "surface":** resting fill for buttons, cards, rows,
  the player bar.
- **Canvas (#E6DFD0):** window and panel background, including the sidebar.
- **Field (#FFFFFF):** inputs, bar tracks, thumbs, and the hover fill.
- **Danger (#FF8FC7), pink:** destructive actions (Delete, Remove) and error banner backgrounds.
  This replaces the original prototype pink.
- **On-primary (#C8F560):** text and icons on an ink fill.
- **Scrim (ink at 55% opacity):** modal backdrop only. It covers the complete
  window content, including sidebar and player, and makes the background inert.
  No translucent cards, fields, decorative overlays, or glass effects. Fully
  transparent hit targets and hidden controls are not translucent surfaces.

On / selected is expressed by inversion: ink fill with lime text. Used for
the selected playlist, active shuffle or repeat. Track multi-selection preserves
the existing playlist appearance: canvas fill with a 6px ink left rule; playing
rows keep their lime fill and unavailable inverted rows keep their existing fill.
Never use pink for selection.

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
Spacing scale: xs 4, sm 8, md 12, lg 16, xl 24, xxl 32. Use these values
or the explicitly documented component-specific catalog measurements below.

Control sizes: row actions 32px, default controls and inputs 36px, transport
buttons 44px, hero 72px, bar track 22px, bar thumb 28px, track row minimum 56px,
sidebar 272px. The sidebar token includes the panel width; its controls use
16px side insets. Library, Queue, and Search navigation is 40px high with
10px gaps and left-aligned labels.

Player bar, docked at the bottom, three zones: track info (left), transport
and timeline (center), volume (right). The timeline sits directly under the
transport row with times beside the bar. Transport order: shuffle, previous,
back 10, play, forward 10, next, repeat.

Long lists use GPUI’s virtualized `list` and `ListState`. Only visible rows are rendered. Row height starts at 56px and grows for complete titles; this intentionally preserves the existing application behavior.

## Elevation & Depth

Depth is created only by hard offset shadows: sm (3px) for buttons, md (5px)
for the hero control, lg (8px) for cards, dialogs, and the player bar. Blur is
visually zero. The color is ink, except the active toggle shadow, which is lime.

The pinned GPUI Linux and Windows renderers have no zero-sigma branch in their shadow shaders.
`theme.rs` uses a 0.01px raster sigma on those platforms to avoid division by zero while
rendering a visually hard edge. This is a renderer workaround, not a blur token.

Shadows paint outside the element. Reserve margin equal to the offset, and do
not put shadowed controls inside an `overflow_hidden` or scroll container
without padding, or the shadow is clipped. Rows in lists and items in the
sidebar carry no shadow.

Button motion is a user-authorized adaptation of neobrutalism.dev's hover
effect to clicks, including native Enter/Space activation. Move the face
down/right by its shadow offset over 150ms, then return over 150ms, using
smoothstep easing. Shrink the shadow offset by the same amount so its outer
edge stays fixed. Actions fire immediately. Hover changes fill only.
Shadowless and disabled controls stay still. Repeated clicks restart the pulse.
Preserve reserved margins and the 3px keyboard focus ring; surrounding layout
must not shift. Every interactive element must be focusable and show focus.

## Shapes

Radius is always zero: use `.rounded_none()`. Borders are 3px ink on buttons,
cards, bars, and inputs. Row dividers are 1px ink. No other border widths.

## Components

**Button.** Neutral fill, 3px ink border, sm shadow, 36px high, mono label
(uppercase for text buttons). Hover: field fill. Click: animated offset and
shadow collapse, then return.
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
title (body-md) over artist (body-sm, secondary), duration in mono fixed at the
right edge. Unknown lengths display `–:–`. Hover actions appear immediately to
the left of the duration, retaining their space so neither metadata nor duration
shifts on hover; each action is also revealed on keyboard focus.
Actions (queue, move up, move down, remove) are 32px icon buttons revealed on
row hover or individual keyboard focus (`.group("row")` and `.group_hover("row", ...)`).
Playing row is tertiary. Multi-selected rows use canvas with a 6px ink left rule.

**Track scrollbar.** Overflowing playlist and queue lists reserve a 20px white rail with a
3px ink divider on the left. The square ink thumb has 3px side insets and a
proportional height with a 48px minimum (capped to the viewport). While dragging,
the thumb becomes white with a 3px ink outline; a 30px-high ink readout sits
8px to the left, showing the first visible row position and filtered list total
as `612 / 1230` in bold 12px Space Mono, lime. The readout appears only during
dragging and stays within the viewport vertically. No shadows or explanatory
labels. The thumb follows wheel scrolling and programmatic navigation. Clicking
the rail seeks; dragging continues outside the rail. Arrow keys, Page Up/Down,
and Home/End work when focused. Non-overflowing lists have no rail, divider, thumb, readout, or reserved gutter.
This user-authorized correction also applies to catalog and description scrollbars.
The rail updates when content or viewport size changes.
Preview with `cargo run --example gallery -- --scrollbar`.

**Playlist item.** Neutral card with 3px border: name over a mono label of
`count . source`. Selected is inverted. No shadow.

**Text input.** Field fill, 3px ink border, 36px high, placeholder in
secondary, focus ring on focus.

**Player bar.** Neutral, a complete 3px outline, 8px offset shadow, 16px
outer side inset aligned with the sidebar controls, 32px bottom inset, and
24px inner padding.
Three zones as described in Layout. Center zone is 512px at widths of at least
1200px and 432px below that breakpoint; volume is 224px. Track info wraps.
Clicking track info opens the queue at the current track. Mute restores the
last audible volume; it does not reset volume to a fixed level.
Volume reuses Bar plus a mute button.

**Page header.** page-title in ink, a secondary metadata line, and actions at
top right (Edit default, Delete destructive).

## Playlist editing

Source: the approved Claude artifact's **Edit playlist** and **Edit playlist
states** artboards. Edit opens a 520px surface dialog with a 3px ink outline,
8px hard shadow and the scrim above. Center it in the window; allow vertical
scrolling with 32px total viewport clearance when the dialog exceeds the window.
The 56px header has a 3px lower rule, 24px left and 12px right padding, a 13px
bold mono heading, and a 32px close control with an 18px icon. The body has 24px
padding and 20px between field groups. Labels use label-caps with an 8px field gap.

Title uses a 44px white input with 16px medium text, 14px horizontal inset,
and the standard focus ring. Its right-aligned 12px mono counter is below, with at least 22px
height. Description is a 150px multiline white field with 14px text at 1.45 line
height, 10px vertical and 14px horizontal text insets, a scrolling
viewport using the shared 20px ruled scrollbar, and a right-aligned 12px mono counter. Preserve newlines and URLs as
plain text. The editor limits are 150 title characters and 5000 description
characters; over-limit text stays editable with Save disabled. Empty or
whitespace-only titles show inline guidance and REQUIRED, without a red field.
**User-authorized correction to the mock:** both character counts align to the
right edge of their fields. Title validation guidance remains left-aligned.

YouTube privacy uses three equal 36px buttons, 12px gaps, 16px icons, 13px bold
mono text, and the standard inverted selected state. Public, Unlisted and Private
each have explanatory helper text underneath. Local playlists have title and
description only: local storage has no YouTube visibility setting.

The footer has a 3px upper rule, 16px vertical and 24px horizontal padding.
Show UNSAVED CHANGES on the left when dirty, Cancel and lime Save on the right.
Disable Save when pristine or invalid. While saving, disable fields and dismissal,
show SAVING…, and keep the dialog open. Load authoritative YouTube metadata before
enabling editing. Failed loading offers Retry. Failed saving preserves the draft
and offers Save again. Form-level errors sit above all fields below the header.
**User-authorized deviation:** reuse the application's existing pink error alert
with Dismiss, not the mock's custom alert. Delete remains separate in the page header.

Cancel, close, and Escape ask before discarding a dirty draft. Keep editing returns
to the draft; Discard closes without writing. Clicking the scrim does not dismiss.
Tab/Shift+Tab stay inside the dialog; closing restores previous focus. Background
navigation and player controls are inert while the dialog is open.

GPUI 0.2.2's `Hsla` has a public alpha channel (`a`). `color::scrim()` uses
`Hsla { a: 0.55, ..color::ink() }` as the backdrop fill, so the scrim needs no
whole-window opacity, blur, or custom shader. This API was verified in the pinned
crate source. Preview with `cargo run --release --example playlist_editor_gallery
-- dirty` (also `pristine`, `saving`, `error`, `empty`, `discard`, and `local`).

## Catalog search and browsing

Source: the approved [Claude artifact](https://claude.ai/chat/517ad159-e9a2-4198-8264-048db6d51b55?artifact=c4853a45-6ad1-438a-8052-0db787d8db8e), from Search through the detail, integration, and no-query artboards. The final designer layouts govern these components.

### Entry and shared fields

Search is a sidebar navigation destination beside Your Library and Queue.
It replaces the main content while preserving the player, queue, and prior
library browsing context. Ctrl+K opens/focuses catalog search; Back/Escape
returns through detail navigation. The existing full player retains its seek
controls and account panels; integration mocks illustrate their placement.
The Add YouTube workaround is removed. Sidebar and track inputs are explicitly
labeled “Filter playlists…” and “Filter tracks…”, distinct from catalog search.

Closing and reopening the app preserves the active catalog screen, query,
category, loaded results, detail navigation history, and each page's scroll
position. Restore loaded content without requiring a network request; retry a
page that was still loading without content. Transient selections, dialogs,
feedback, and account operations are not restored.

The catalog field is 56px high with a leading search icon and the placeholder
“Search songs, videos, albums, artists…”. Search content has a 16px inset in the
application, 32px in standalone component previews. All search/filter fields,
including the destination picker, have an inline × only while their input is
nonempty. Clearing retains field focus and updates its results. The clear control
is 32px in the catalog field, 24px in compact 36px fields, with an 18px icon,
3px border, and no shadow or redundant tooltip. Name-entry fields are not searches.

Category chips are 36px controls with 13px bold Space Mono labels, 12px gaps,
and the standard 3px hard shadow. Selection uses ink/lime inversion. Chips wrap
without clipping. Show actual service categories, including Songs, Videos,
Albums, Artists, Community playlists, Featured playlists, Podcasts, Episodes,
and Profiles, plus All. Preserve additional named service categories; omit empty
labels. The category set is a functional exception to the mock, not a closed list.

### No-query and request states

With no query, show the chips. With no saved history, show the first-use panel:
3px ink outline, surface fill, 24px padding, a 56px canvas icon frame containing
a 28px search icon, and a 20px gap to text. The heading is 18px bold Space Grotesk,
“Find something to play”. Supporting copy is 14px medium, muted, line height 1.4,
with a maximum width of 560px and an 8px heading gap:
“Search for songs, videos, albums, artists, playlists and episodes. Your recent
searches will appear here.” Do not show an empty Recent searches heading.
Opening this state or selecting a chip without a query does not fetch results.

With history, show Recent searches and a right-aligned Clear all action above
48px rows with clock icons and individual remove controls. Clear all, or removing
the last entry, restores the first-use panel. Clearing a query restores the
appropriate no-query state; the field’s × disappears.

Loading uses Searching… and three 64px skeleton rows, with 48px thumbnail blocks
and 12px/10px text bars. No-results and connection-error states use bordered
surface panels with 20px padding, a 24px semantic icon, 18px bold heading,
13px muted explanation, and a 36px lime recovery action. No-results offers
Search all types; connection failure offers Retry. Hide result chips during
these request states as in the Search states mock. Error detail remains available
where useful. These catalog recovery panels are distinct from pink error banners.

### Results

**User-authorized navigation:** clicking a Video search result opens its uploader's
Channel detail page. Reuse the artist page's 160px portrait, header typography,
spacing, and whole-page scrolling, with Videos in selectable track rows and
Playlists in the existing card grid. The separate Play control plays audio.
Place a standard compact search field 24px below the header, labelled “Search
channel videos and playlists…”. Match titles case-insensitively across all public
videos and playlists after loading; clearing restores both sections. Search is
local to the channel and does not change the main catalog query. Clear selection
when filtering and restrict range selection to visible matches. Show loading,
retryable errors, no public content, and no matching content explicitly. Back
restores the previous catalog page. This channel page is a user-authorized
extension of the artist reference; it has no artist shuffle or subscribe actions.

Mixed results use a ruled surface list without a redundant query heading. Rows are
64px high, with 48px bordered real thumbnails, 14px medium titles, and 13px muted
secondary descriptions. **Put the record category in the description, not in a
separate badge**—the user's approved exception. Durations or collection counts
occupy an 84px trailing column in 14px regular Space Mono. Do not duplicate this
metadata in the description or invent missing values.

Track tables in library, queue, and catalog details use plain click to select one occurrence, Ctrl-click to toggle
an occurrence, Shift-click for an anchored range, and Ctrl+Shift-click to add a
range. Selection is independent per table. Clicking a track row never starts
playback; its Play button does. Row actions do not change selection or start
a drag. Playlist reordering retains its existing restrictions and deferred
selection collapse during dragging; catalog and queue rows cannot reorder.
Catalog selection resets on page replacement and survives append-only loading.
Queue selection resets when queue contents/order change, so duplicate entries
never transfer selection to a different occurrence.

Playable rows reveal a play overlay on the thumbnail and 32px queue/save actions
with 18px icons on hover or keyboard focus. Reserve action space to prevent text
movement. Collections open their detail page. Real thumbnails replace the mocks'
icon placeholders; bundled icons are fallbacks only. Use the persistent 20px
white scrollbar rail with an ink divider and a thumb proportional to actual
overflow; illustrative fixed mock thumb lengths are not normative.

### Multi-selection actions

Source: the final **Multi-selection bar** and **Selection bar variants** boards.
The first selected occurrence reveals a minimum 66px canvas strip directly above its
track table. It has a 3px ink outline in flow; when pinned it spans the content
viewport with only its 3px bottom rule. It remains reachable during wheel,
scrollbar, and programmatic scrolling. Clear removes the selection and the strip.
Library and queue keep their bounded virtualized lists; their strip stays above
the scrolling rows. Search results have no selection strip.

The inverted count tag uses label-caps, ink fill, lime text, and 8px horizontal
padding. Its height matches the 36px action buttons, with text vertically centered,
as in the approved mock. It reuses the control-height token.
Neutral 36px Play, Queue, and Add to playlist buttons use
13px bold mono text and 18px icons, with 12px gaps. Clear sits at the far right.
The header retains its lime primary. Owned library playlists add pink Remove,
separated by 24px; removal asks for confirmation and reports partial failures.
Unavailable selected tracks count toward selection and show `N UNAVAILABLE,
SKIPPED`; playback, queueing, and saving use the remaining available occurrences
in displayed order, including duplicates. No available tracks makes those actions
inactive. Single-selection uses the identical bar with `1 SELECTED`.
Unavailable summaries use a 128px column and a 90px strip so the note can wrap
without displacing Clear. At narrow library widths, action buttons wrap within
their group and the strip grows vertically; Clear remains at the far right.
Count tags retain their intrinsic width. Shared actions have no bottom margin;
the strip's 12px vertical inset provides clearance for their hard shadows.

Hide per-row queue/save/remove actions while that table has a selection. Keep
single-track hover Play. Compensate the 6px left selection rule in the number
slot to preserve title alignment. Collection and artist cards are not selectable.
Search results, including filtered song results, are navigation-only and never
support multi-selection. Clicking an album opens its details; clicking a song
opens the album linked in its metadata. If search omits that link, resolve it
from the song’s next metadata without starting playback; stale responses cannot
replace newer navigation. If the service still provides no album, report that
album information is unavailable. Play, Queue, and Add to playlist
remain independent row actions and do not trigger navigation.

### Detail pages

Detail pages replace the search field with Back. **User-authorized correction
to the selection mock:** album, single, EP, playlist, and podcast details keep
Back, artwork, metadata, and actions fixed. The track table fills the remaining
height and scrolls internally with the same scrollbar as search results.
The selection strip stays fixed directly above the scrolling rows; selecting
or clearing tracks changes the table viewport height, not the header position.
Supplementary sections remain inside the scroll area.
**User-authorized correction:** paginated results load automatically as the viewport
approaches the end, including short pages that do not yet overflow. Never show a
Load more button. Catalog rows reserve their specified heights and construct
controls only in the visible viewport. Refresh tokens are not pagination; empty
pages and consumed continuation tokens terminate loading. Errors retain explicit
Retry recovery rather than automatically retrying.
Artist pages also retain whole-page scrolling. Use
32px page padding, 24px after the 36px Back control, and a 32px cover/text gap.
Album and playlist covers are 192px squares; artist portraits are 160px. Covers
have 3px borders and 5px hard shadows. Align header information to the cover's
bottom edge. Titles use page-title with 1.1 line height; category labels use
label-caps, and byline/metadata use body-md/body-sm. Navigable creator credits
are underlined. Metadata reflects the service; missing values are omitted.

Header actions are 44px high, with 18px horizontal padding, 10px icon/text gaps,
18px icons, and 13px bold Space Mono labels. Allow wrapping. Album and playlist
actions are Play, Shuffle, Queue, Add to playlist, and Add to library. Artist
actions are Shuffle and Subscribe. Play or artist Shuffle is the single lime
primary action. Add to library saves a local playlist snapshot; Add to playlist
opens the destination picker.

**User-authorized correction:** action feedback spans the full content width
beneath the entire detail header, separated by 24px. Only completed queue,
playlist, and library additions appear here; an already-saved playlist uses the
info variant. Every playlist-related alert includes the destination playlist's
name, including library additions and already-saved feedback. Use definite
articles: “Added {count} tracks to the queue”, “Added {count} tracks to the
playlist ‘{name}’”, “Added the playlist ‘{name}’ to your library”, and “The
playlist ‘{name}’ is already in your library”. Display names in double curly
quotes. Success alerts use lime, info alerts use neutral, both with a 3px
ink border, 12px padding/gap, body-md text, wrapping message, and Dismiss button.
Reuse the shared error-alert geometry and existing theme tokens. Do not render
playback, preparation/progress, or cancellation messages as bare text beneath
artwork. Loading and error states retain their dedicated UI. Catalog search
uses the same alerts for these action outcomes. Preview both variants with
`catalog_gallery -- alert-success` and `catalog_gallery -- alert-info`.

Collection tables start 32px below the header, with a 3px outer border and 56px
rows separated by 1px rules. Number columns are 56px, duration columns 60px,
both 14px regular Space Mono. Album rows omit redundant artist subtitles;
playlist rows retain them. These catalog rows retain their distinct layout from the existing
library/queue rows while sharing track selection and action isolation.

Artist pages separate Top songs and Albums. Section headings have 32px height,
32px top spacing, 12px before their content, and a right-aligned See all control.
Songs use the numbered table. Album cards have 176px square covers, 24px gaps,
and title/secondary metadata below. Additional service sections retain their
appropriate row or card presentation. Catalog body line height is 1.2 unless
an explicit heading or first-use-copy rule overrides it.

Preview these production components with `cargo run --release --example
catalog_gallery -- search` and the `album`, `playlist`, `artist`, `first-use`,
`empty` (history), `loading`, `no-results`, and `error` states. Verification outcomes belong in the task or PR summary; this specification and
`AGENTS.md` retain the enduring design and contributor rules.

## Do's and Don'ts

| Do | Don't |
|---|---|
| Use tokens from this file via `theme.rs` | Hardcode hex values, px sizes or shadows |
| One lime primary action per view | Fill whole panels with lime |
| Invert for "on" and "selected" | Use pink for selection |
| Reveal row actions on hover | Repeat four buttons on every row |
| Embed fonts and icons as assets | Rely on system fonts or unicode glyphs |
| Keep shadows on controls only | Put shadows on rows or scroll content |
| Preserve keyboard access; add tooltips only when they clarify ambiguity | Repeat obvious actions or visible labels in tooltips |
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
handling. Tooltips explain ambiguous actions, destinations, errors, or truncated
content; obvious Play, Pause, and clear-input controls do not need them.
Accessibility labels and keyboard behavior are independent of tooltips.

## Verification

### Native development workbench

The user-authorized development workbench unifies the existing galleries in a
native window. Its shell reuses the shared controls and theme: 272px sidebar,
8px sidebar padding and story gaps, 16px toolbar padding, and 12px toolbar gaps.
Story rows have no shadow; the active story uses ink/lime inversion. Group labels
use label-caps. The initial window is the 1280px preview width plus the sidebar,
with an 840px height. Compact mode fixes the preview width at the existing 1100px
minimum desktop width, with horizontal scrolling when needed; Fill uses available
space. These dimensions reuse `size::sidebar`, `layout::viewport_width`,
`layout::viewport_height`, and `layout::min_width` without new tokens.

The searchable story list, Reset, and Fill/Compact controls belong to the
development shell. The preview renders the production components and existing
approved gallery states. Search, selected story, and viewport mode survive
rebuilds; transient interaction state resets. Catalog service actions are
simulated or remain on their fixture, with isolated local data. No new product
screen design is implied by the workbench shell.

### Checks

1. Run `npx @google/design.md lint DESIGN.md`. It must report no errors.
2. Add or update the component in `examples/gallery.rs` in every state:
   default, hover, pressed, focused, disabled, on.
3. Run the gallery and compare against this file, not against memory.
4. Commit gallery screenshots to `docs/` so reviewers can see the result
   without running the app.


## Account and landing screens

The landing card is 520px wide with 48px padding, a 100px logo, a 56px bold
Mousa title with a 56px line height, and the 18px tagline “Just a fast music client.”
The tagline uses an 18px line height, separated from the title by the md (12px) gap. Its centered canvas,
surface fill, 3px border, and large hard shadow follow the desktop reference.
The Google sign-in action is 56px tall and uses the primary fill.

Provider brand icons retain their original colors. Connected account buttons
show Google's full profile name or Last.fm's username. Hover preserves the
icon, replaces the text with “Sign out”, and uses the danger background.
Disconnected Last.fm shows “Connect Last.fm”. The playlist refresh button is
removed; the existing periodic sync remains.

Preview these shared components with `cargo run --example gallery -- --landing`
or `cargo run --example gallery -- --accounts`.
