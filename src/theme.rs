//! Mousa design tokens: the single source of truth for the neobrutalist look.
//!
//! Rules for agents:
//! - Never hardcode a color, border width, shadow, font or spacing value
//!   anywhere else in the app. Add a token here first, then use it.
//! - Written against the `gpui` crate; verify field names against the
//!   version pinned in Cargo.toml (BoxShadow, rgb, px have changed before).
//! - See DESIGN.md for what each token means and where it is allowed.

use gpui::{BoxShadow, Hsla, Pixels, hsla, point, px, rgb};

pub mod playlist_editor {
    use super::*;
    pub fn width() -> Pixels {
        px(520.)
    }
    pub fn header() -> Pixels {
        px(56.)
    }
    pub fn title_height() -> Pixels {
        px(44.)
    }
    pub fn title_text() -> Pixels {
        px(16.)
    }
    pub fn description_height() -> Pixels {
        px(150.)
    }
    pub fn field_gap() -> Pixels {
        px(20.)
    }
    pub fn field_inset() -> Pixels {
        px(14.)
    }
    pub fn counter_height() -> Pixels {
        px(22.)
    }
    pub fn icon() -> Pixels {
        px(16.)
    }
    pub fn close_icon() -> Pixels {
        px(18.)
    }
    pub fn description_inset() -> Pixels {
        px(10.)
    }
    pub const DESCRIPTION_LINE_HEIGHT: f32 = 1.45;
}

/// Colors. Lime and pink carry meaning, see DESIGN.md "Color semantics".
pub mod color {
    use super::*;

    pub fn ink() -> Hsla {
        rgb(0x111111).into()
    } // text, borders, shadows
    pub fn scrim() -> Hsla {
        Hsla { a: 0.55, ..ink() }
    }
    pub fn surface() -> Hsla {
        rgb(0xFBF7EE).into()
    } // cards, controls, rows
    pub fn canvas() -> Hsla {
        rgb(0xE6DFD0).into()
    } // window / panel background
    pub fn muted() -> Hsla {
        rgb(0x3B3B3B).into()
    } // secondary text (>= 4.5:1 on surface)
    pub fn lime() -> Hsla {
        rgb(0xC8F560).into()
    } // playing / primary action
    pub fn pink() -> Hsla {
        rgb(0xFF8FC7).into()
    } // destructive actions and error banners; accepted designer palette
    pub fn white() -> Hsla {
        rgb(0xFFFFFF).into()
    } // input fields, bar tracks
    pub fn transparent() -> Hsla {
        hsla(0., 0., 0., 0.)
    }
}

/// Border widths. Radius is always zero: use `.rounded_none()`.
pub mod border {
    use super::*;
    pub fn strong() -> Pixels {
        px(3.)
    } // buttons, cards, bars, inputs
    pub fn hairline() -> Pixels {
        px(1.)
    } // row dividers only
}

/// Hard shadows: offset down and right; ink by default, lime for active toggles.
/// A control with a shadow must reserve margin equal to the offset.
pub mod shadow {
    use super::*;

    // GPUI 0.2.2's Linux and Windows Gaussian shaders divide by sigma without a zero branch.
    // A subpixel sigma renders a hard edge while avoiding NaNs; the design blur is zero.
    fn raster_blur() -> Pixels {
        if cfg!(any(target_os = "linux", target_os = "windows")) {
            px(0.01)
        } else {
            px(0.)
        }
    }
    fn hard(offset: f32) -> Vec<BoxShadow> {
        vec![BoxShadow {
            color: color::ink(),
            offset: point(px(offset), px(offset)),
            blur_radius: raster_blur(),
            spread_radius: px(0.),
        }]
    }
    pub fn sm() -> Vec<BoxShadow> {
        hard(3.)
    } // buttons
    pub fn md() -> Vec<BoxShadow> {
        hard(5.)
    } // hero control (play)
    pub fn lg() -> Vec<BoxShadow> {
        hard(8.)
    } // cards, player bar, dialogs

    /// Inverted toggle depth.
    pub fn on() -> Vec<BoxShadow> {
        let mut shadows = sm();
        shadows[0].color = color::lime();
        shadows
    }
    /// Keyboard focus ring: thick ink outline outside the border.
    pub fn focus_ring() -> BoxShadow {
        BoxShadow {
            color: color::ink(),
            offset: point(px(0.), px(0.)),
            blur_radius: raster_blur(),
            spread_radius: px(3.),
        }
    }
}

/// Offset applied while a control is pressed. Equal to its shadow offset,
/// with the shadow removed, so it looks pushed into the page.
pub mod press {
    use super::*;
    pub fn duration() -> std::time::Duration {
        std::time::Duration::from_millis(150)
    }
    pub fn progress(elapsed: std::time::Duration) -> f32 {
        let t = elapsed.as_secs_f32() / duration().as_secs_f32();
        let t = t.clamp(0., 1.);
        t * t * (3. - 2. * t)
    }
    pub fn sm() -> Pixels {
        px(3.)
    }
    pub fn md() -> Pixels {
        px(5.)
    }
}

/// Fonts are embedded and loaded at startup. Never rely on system fonts.
pub mod font {
    pub const SANS: &str = "Space Grotesk"; // 500 and 700 only
    pub const MONO: &str = "Space Mono"; // 400 and 700
}

/// Type scale in px.
pub mod text {
    use super::*;
    pub fn tracking() -> Pixels {
        px(0.96)
    } // 0.08em at 12px
    pub fn label() -> Pixels {
        px(12.)
    } // mono, uppercase, letter-spaced
    pub fn body() -> Pixels {
        px(14.)
    }
    pub fn secondary() -> Pixels {
        px(13.)
    }
    pub fn track_title() -> Pixels {
        px(18.)
    }
    pub fn page_title() -> Pixels {
        px(28.)
    }
}

/// Exact measurements from the Search, Album, Playlist and Artist artboards.
pub mod catalog {
    use super::*;
    pub fn search_width() -> Pixels {
        px(224.)
    }
    pub fn action_padding() -> Pixels {
        px(18.)
    }
    pub fn icon() -> Pixels {
        px(18.)
    }
    pub fn nav_gap() -> Pixels {
        px(10.)
    }
    pub fn nav_height() -> Pixels {
        px(40.)
    }
    pub fn field() -> Pixels {
        px(56.)
    }
    pub fn result_row() -> Pixels {
        px(64.)
    }
    pub fn track_row() -> Pixels {
        px(56.)
    }
    pub fn recent_row() -> Pixels {
        px(48.)
    }
    pub fn thumbnail() -> Pixels {
        px(48.)
    }
    pub fn cover() -> Pixels {
        px(192.)
    }
    pub fn portrait() -> Pixels {
        px(160.)
    }
    pub fn card() -> Pixels {
        px(176.)
    }
    pub fn action() -> Pixels {
        px(44.)
    }
    pub fn number() -> Pixels {
        px(56.)
    }
    pub fn duration() -> Pixels {
        px(84.)
    }
    pub fn track_duration() -> Pixels {
        px(60.)
    }
    pub fn state_padding() -> Pixels {
        px(20.)
    }
    pub fn skeleton_title() -> Pixels {
        px(12.)
    }
    pub fn skeleton_subtitle() -> Pixels {
        px(10.)
    }
}

/// Spacing scale. Use these, not ad hoc numbers.
pub mod space {
    use super::*;
    pub fn xs() -> Pixels {
        px(4.)
    }
    pub fn sm() -> Pixels {
        px(8.)
    }
    pub fn md() -> Pixels {
        px(12.)
    }
    pub fn lg() -> Pixels {
        px(16.)
    }
    pub fn xl() -> Pixels {
        px(24.)
    }
    pub fn xxl() -> Pixels {
        px(32.)
    }
}

/// Control sizes. Desktop and mouse first, so denser than touch targets.
pub mod size {
    pub fn availability_width() -> gpui::Pixels {
        gpui::px(150.)
    }
    pub fn availability_height() -> gpui::Pixels {
        gpui::px(28.)
    }
    use super::*;
    pub fn row_action() -> Pixels {
        px(32.)
    } // icon buttons inside list rows
    pub fn control() -> Pixels {
        px(36.)
    } // default buttons and inputs
    pub fn transport() -> Pixels {
        px(44.)
    } // player bar buttons
    pub fn hero() -> Pixels {
        px(72.)
    } // play / pause
    pub fn bar_height() -> Pixels {
        px(22.)
    } // progress and volume tracks
    pub fn bar_thumb() -> Pixels {
        px(28.)
    }
    pub fn row() -> Pixels {
        px(56.)
    } // track row height
    pub fn sidebar() -> Pixels {
        px(272.)
    }
}

/// Layout tokens extending the supplied desktop control scale.
pub mod layout {
    use super::*;
    pub fn zero() -> Pixels {
        px(0.)
    }
    pub fn icon() -> Pixels {
        px(24.)
    }
    pub fn hero_icon() -> Pixels {
        px(32.)
    }
    pub fn player_info() -> Pixels {
        px(280.)
    }
    pub fn player_volume() -> Pixels {
        px(224.)
    }
    pub fn player_center() -> Pixels {
        px(512.)
    }
    pub fn compact_center() -> Pixels {
        px(432.)
    }
    pub fn player_breakpoint() -> Pixels {
        px(1200.)
    }
    pub fn time_label() -> Pixels {
        px(48.)
    }
    pub fn row_index() -> Pixels {
        px(36.)
    }
    pub fn viewport_width() -> Pixels {
        px(1280.)
    }
    pub fn viewport_height() -> Pixels {
        px(840.)
    }
    pub fn min_width() -> Pixels {
        px(1100.)
    }
    pub fn min_height() -> Pixels {
        px(700.)
    }
    pub fn list_overscan() -> Pixels {
        px(200.)
    }
}

// Landing page proportions from the supplied desktop reference.
pub mod landing {
    use super::*;
    pub fn width() -> Pixels {
        px(520.)
    }
    pub fn padding() -> Pixels {
        px(48.)
    }
    pub fn logo() -> Pixels {
        px(100.)
    }
    pub fn title() -> Pixels {
        px(56.)
    }
}

pub mod scrollbar {
    use super::*;
    pub fn width() -> Pixels {
        px(20.)
    }
    pub fn min_thumb() -> Pixels {
        px(48.)
    }
    pub fn readout_height() -> Pixels {
        px(30.)
    }
}

/// Shared track multi-selection marker (track-row-selected in DESIGN.md).
pub mod selection {
    use super::*;
    pub fn bar_height(unavailable: usize) -> Pixels {
        if unavailable > 0 { px(90.) } else { px(66.) }
    }
    pub fn summary_width() -> Pixels {
        px(128.)
    }
    pub fn marker() -> Pixels {
        px(6.)
    }
}

/// Playlist drag geometry, kept independent of playback's lime treatment.
pub mod reorder {
    use super::*;
    pub fn drag_threshold() -> Pixels {
        px(6.)
    }
    pub fn line() -> Pixels {
        px(6.)
    }
    pub fn cap() -> Pixels {
        px(16.)
    }
    pub fn scroll_step() -> Pixels {
        px(8.)
    }
}
