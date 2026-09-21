//! The app's design system: the layout, motion, and color/type tokens
//! shared by every overlay surface, kept apart from the AppKit drawing code
//! in `app.rs` so a new visual identity is a new [`Theme`] value rather than
//! a hunt through `draw` calls for magic numbers.
//!
//! [`CLASSIC`] preserves the app's original geometry and motion, with fixed
//! dark reply ink for contrast on its white bubble. [`FABLE_PROTOCOL`] is a
//! second, distinct system built the same
//! way. Pick one per run with `CG_AGENT_THEME=classic` or
//! `CG_AGENT_THEME=fable-protocol` (see [`active`]); `classic` stays the
//! default so existing behavior is unchanged unless asked for otherwise.
//!
//! This module has no macOS dependency, so it builds and its tests run on
//! every platform; only `app.rs` (macOS-only) knows how to turn a [`Rgba`]
//! into an `NSColor`.

use std::sync::OnceLock;

/// A color in the sRGB color space, matching the component order of
/// `NSColor::colorWithSRGBRed_green_blue_alpha`.
#[derive(Debug, Clone, Copy)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Rgba {
    pub const fn new(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }
}

/// Sizes and positions for the overlay's controls, in points.
#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    pub strip_h: f64,
    pub creature_h: f64,
    pub bubble_w: f64,
    pub bubble_h: f64,
    pub max_expanded_reply_h: f64,
    pub reply_line_h: f64,
    pub reply_chars_per_line: usize,
    pub see_more_w: f64,
    pub see_more_h: f64,
    pub input_w: f64,
    pub input_h: f64,
    pub creature_y: f64,
    pub input_y: f64,
}

/// Animation pacing and mood-linked opacity.
#[derive(Debug, Clone, Copy)]
pub struct Motion {
    pub walk_speed: f64,
    /// Seconds between animation ticks (the app's own frame rate).
    pub tick_interval: f64,
    pub thinking_bob_freq: f64,
    pub thinking_bob_amp: f64,
    pub idle_bob_freq: f64,
    pub idle_bob_amp: f64,
    pub talking_bob_offset: f64,
    pub asleep_opacity: f64,
    pub sick_opacity: f64,
    pub awake_opacity: f64,
}

/// Color and type. `text_color: None` keeps the system's adaptive label
/// color (tracks Appearance) instead of a fixed ink.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bubble_background: Rgba,
    pub text_color: Option<Rgba>,
    pub font_size: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub name: &'static str,
    pub metrics: Metrics,
    pub motion: Motion,
    pub palette: Palette,
}

/// The app's original look: a translucent-white bubble with fixed dark ink,
/// so it remains legible in either system Appearance, at a brisk 30fps.
pub const CLASSIC: Theme = Theme {
    name: "classic",
    metrics: Metrics {
        strip_h: 200.0,
        creature_h: 88.0,
        bubble_w: 640.0,
        bubble_h: 112.0,
        max_expanded_reply_h: 420.0,
        reply_line_h: 18.0,
        reply_chars_per_line: 80,
        see_more_w: 84.0,
        see_more_h: 24.0,
        input_w: 280.0,
        input_h: 24.0,
        creature_y: 8.0,
        input_y: 12.0,
    },
    motion: Motion {
        walk_speed: 1.6,
        tick_interval: 1.0 / 30.0,
        thinking_bob_freq: 0.4,
        thinking_bob_amp: 8.0,
        idle_bob_freq: 0.15,
        idle_bob_amp: 2.0,
        talking_bob_offset: 4.0,
        asleep_opacity: 0.45,
        sick_opacity: 0.85,
        awake_opacity: 1.0,
    },
    palette: Palette {
        bubble_background: Rgba::new(1.0, 1.0, 1.0, 0.94),
        text_color: Some(Rgba::new(0.08, 0.08, 0.1, 1.0)),
        font_size: 12.0,
    },
};

/// A second design system: a fixed dark-ink storybook bubble with warm
/// parchment text (no Appearance tracking), a larger stage, and a calmer,
/// more deliberate 24fps gait.
pub const FABLE_PROTOCOL: Theme = Theme {
    name: "fable-protocol",
    metrics: Metrics {
        strip_h: 220.0,
        creature_h: 96.0,
        bubble_w: 680.0,
        bubble_h: 128.0,
        max_expanded_reply_h: 480.0,
        reply_line_h: 20.0,
        reply_chars_per_line: 76,
        see_more_w: 92.0,
        see_more_h: 26.0,
        input_w: 300.0,
        input_h: 26.0,
        creature_y: 10.0,
        input_y: 14.0,
    },
    motion: Motion {
        walk_speed: 1.1,
        tick_interval: 1.0 / 24.0,
        thinking_bob_freq: 0.3,
        thinking_bob_amp: 6.0,
        idle_bob_freq: 0.12,
        idle_bob_amp: 3.0,
        talking_bob_offset: 3.0,
        asleep_opacity: 0.35,
        sick_opacity: 0.75,
        awake_opacity: 1.0,
    },
    palette: Palette {
        bubble_background: Rgba::new(0.098, 0.098, 0.145, 0.92),
        text_color: Some(Rgba::new(0.925, 0.878, 0.769, 1.0)),
        font_size: 13.0,
    },
};

/// Looks up a theme by name, case- and separator-insensitively (`fable`,
/// `fable_protocol`, and `Fable-Protocol` all match `fable-protocol`).
/// Unknown names resolve to `None` rather than silently falling back, so
/// callers can decide how to report a typo.
pub fn by_name(name: &str) -> Option<&'static Theme> {
    match name.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "classic" => Some(&CLASSIC),
        "fable-protocol" | "fable" => Some(&FABLE_PROTOCOL),
        _ => None,
    }
}

fn resolve(requested: Option<&str>) -> &'static Theme {
    requested.and_then(by_name).unwrap_or(&CLASSIC)
}

/// The theme selected for this run, via the `CG_AGENT_THEME` environment
/// variable (default `classic`). Resolved once and cached: changing the
/// variable mid-run has no effect, so the whole overlay stays visually
/// consistent for the app's lifetime.
pub fn active() -> &'static Theme {
    static ACTIVE: OnceLock<&'static Theme> = OnceLock::new();
    ACTIVE.get_or_init(|| resolve(std::env::var("CG_AGENT_THEME").ok().as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_and_unknown_fall_back_to_classic() {
        assert_eq!(resolve(None).name, "classic");
        assert_eq!(resolve(Some("nonsense")).name, "classic");
        assert_eq!(resolve(Some("")).name, "classic");
    }

    #[test]
    fn known_names_resolve_case_and_separator_insensitively() {
        assert_eq!(resolve(Some("classic")).name, "classic");
        assert_eq!(resolve(Some("CLASSIC")).name, "classic");
        assert_eq!(resolve(Some("fable-protocol")).name, "fable-protocol");
        assert_eq!(resolve(Some("Fable-Protocol")).name, "fable-protocol");
        assert_eq!(resolve(Some("FABLE_PROTOCOL")).name, "fable-protocol");
        assert_eq!(resolve(Some("fable")).name, "fable-protocol");
        assert_eq!(by_name("fable-protocol").unwrap().name, "fable-protocol");
    }

    #[test]
    fn classic_preserves_geometry_and_pairs_white_background_with_dark_ink() {
        let m = CLASSIC.metrics;
        assert_eq!(
            (m.strip_h, m.creature_h, m.bubble_w, m.bubble_h),
            (200.0, 88.0, 640.0, 112.0)
        );
        assert_eq!(CLASSIC.motion.walk_speed, 1.6);
        assert_eq!(CLASSIC.motion.tick_interval, 1.0 / 30.0);
        assert_eq!(CLASSIC.palette.font_size, 12.0);
        let ink = CLASSIC.palette.text_color.expect("classic reply ink");
        assert!(ink.r < 0.2 && ink.g < 0.2 && ink.b < 0.2);
    }

    #[test]
    fn fable_protocol_is_a_distinct_fixed_ink_theme() {
        assert_ne!(FABLE_PROTOCOL.metrics.bubble_w, CLASSIC.metrics.bubble_w);
        assert_ne!(FABLE_PROTOCOL.motion.walk_speed, CLASSIC.motion.walk_speed);
        assert!(FABLE_PROTOCOL.palette.text_color.is_some());
    }
}
