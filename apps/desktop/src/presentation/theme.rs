//! Design tokens.
//!
//! The app is painted from a single token set with two appearances ("Midnight"
//! and "Daylight"), the same arrangement komet uses: colors are data, numbers
//! are functions of the layout, and no view ever hardcodes a hex value.
//!
//! Daylight is designed, not inverted. Chrome (the sidebar) goes *darker* than
//! the chat plane in light mode — the opposite of a literal inversion — and
//! floating surfaces earn their separation from a hairline instead of from a
//! lighter fill, because there is nothing lighter than white to climb to.
//!
//! Hierarchy comes from three ink levels and size, never from a fourth grey.
//! Accent is rare: links, focus, the selected row. Bot colors belong to bots.

use gpui::{App, Global, Hsla, SharedString, hsla};

/// Which skin the app is painting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Appearance {
    #[default]
    Dark,
    Light,
}

impl Appearance {
    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }

    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Midnight",
            Self::Light => "Daylight",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }

    /// Follow the OS, so the first paint is never the wrong skin.
    pub fn from_window(appearance: gpui::WindowAppearance) -> Self {
        use gpui::WindowAppearance::*;
        match appearance {
            Light | VibrantLight => Self::Light,
            Dark | VibrantDark => Self::Dark,
        }
    }
}

/// Every color the UI is allowed to name.
///
/// The table is complete on purpose: a menu or card surface that has not landed
/// yet still has a token, so the next view picks one instead of inventing a hex.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Theme {
    pub appearance: Appearance,

    // Surfaces
    /// The chat plane.
    pub bg: Hsla,
    /// The sidebar plane.
    pub panel: Hsla,
    /// A card resting in the page (bot panel sections).
    pub card: Hsla,
    /// Selected sidebar row.
    pub raised: Hsla,
    pub raised_hover: Hsla,
    /// The person's message plate, and the ink that reads on it.
    pub user_plate: Hsla,
    pub on_user_plate: Hsla,
    /// Recessed field (search).
    pub inset: Hsla,
    /// A small control plate (chips, icon buttons).
    pub control: Hsla,
    pub menu: Hsla,
    pub menu_hover: Hsla,
    pub menu_active: Hsla,
    /// The composer plate.
    pub composer: Hsla,
    /// A dialog's surface: the highest plane in the app.
    pub dialog: Hsla,
    /// What sits behind a dialog. Nothing else dims the app.
    pub scrim: Hsla,

    // Ink
    pub ink: Hsla,
    pub ink_secondary: Hsla,
    pub ink_tertiary: Hsla,

    // Lines
    pub hairline: Hsla,
    pub edge: Hsla,

    // Accent
    pub accent: Hsla,
    pub accent_text: Hsla,
    /// Label on an accent fill.
    pub on_accent: Hsla,
    pub focus: Hsla,

    // Status
    pub success: Hsla,
    pub warning: Hsla,
    pub danger: Hsla,
    pub danger_ink: Hsla,

    // Bot identity
    identity: [Hsla; 8],

    // Type
    pub font_sans: SharedString,
    pub font_mono: SharedString,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Global for Theme {}

impl Theme {
    pub fn dark() -> Self {
        let ink = hsla(0.11, 0.05, 0.95, 1.0);
        Self {
            appearance: Appearance::Dark,
            // Match Komet's neutral shell: #191919 canvas and #1f1f1f sidebars.
            bg: hsla(0.0, 0.0, 25.0 / 255.0, 1.0),
            panel: hsla(0.0, 0.0, 31.0 / 255.0, 1.0),
            card: hsla(0.0, 0.0, 26.0 / 255.0, 1.0),
            raised: hsla(0.0, 0.0, 0.27, 1.0),
            raised_hover: hsla(0.0, 0.0, 0.32, 1.0),
            user_plate: hsla(0.0, 0.0, 0.27, 1.0),
            on_user_plate: hsla(0.0, 0.0, 0.968, 1.0),
            inset: hsla(0.0, 0.0, 0.095, 1.0),
            control: hsla(0.0, 0.0, 0.16, 1.0),
            menu: hsla(0.0, 0.0, 0.15, 1.0),
            menu_hover: hsla(0.0, 0.0, 0.215, 1.0),
            menu_active: hsla(0.0, 0.0, 0.255, 1.0),
            composer: hsla(0.0, 0.0, 0.13, 1.0),
            dialog: hsla(0.0, 0.0, 0.155, 1.0),
            scrim: hsla(0.0, 0.0, 0.0, 0.62),
            ink,
            ink_secondary: hsla(0.09, 0.03, 0.65, 1.0),
            ink_tertiary: hsla(0.09, 0.03, 0.54, 1.0),
            hairline: hsla(0.0, 0.0, 1.0, 0.10),
            edge: hsla(0.0, 0.0, 1.0, 0.15),
            accent: ink,
            accent_text: ink,
            on_accent: hsla(0.0, 0.0, 0.075, 1.0),
            // Focus is ink, not a hue: the accent stays reserved for links and
            // the selected state, and a neutral ring reads calm on every skin.
            focus: hsla(0.0, 0.0, 1.0, 0.50),
            success: hsla(0.42, 0.55, 0.55, 1.0),
            warning: hsla(0.11, 0.85, 0.62, 1.0),
            danger: hsla(0.02, 0.72, 0.62, 1.0),
            danger_ink: hsla(0.02, 0.35, 0.12, 1.0),
            identity: identity_palette(true),
            font_sans: "Adwaita Sans".into(),
            font_mono: "Adwaita Mono".into(),
        }
    }

    pub fn light() -> Self {
        let ink = hsla(0.11, 0.06, 0.115, 1.0);
        Self {
            appearance: Appearance::Light,
            bg: hsla(0.0, 0.0, 1.0, 1.0),
            panel: hsla(0.0, 0.0, 0.968, 1.0),
            card: hsla(0.0, 0.0, 1.0, 1.0),
            raised: hsla(0.0, 0.0, 0.94, 1.0),
            raised_hover: hsla(0.0, 0.0, 0.90, 1.0),
            user_plate: hsla(0.0, 0.0, 0.94, 1.0),
            on_user_plate: ink,
            inset: hsla(0.12, 0.06, 0.945, 1.0),
            control: hsla(0.12, 0.05, 0.93, 1.0),
            menu: hsla(0.0, 0.0, 1.0, 1.0),
            menu_hover: hsla(0.12, 0.05, 0.945, 1.0),
            menu_active: hsla(0.12, 0.05, 0.915, 1.0),
            composer: hsla(0.12, 0.08, 0.975, 1.0),
            dialog: hsla(0.0, 0.0, 1.0, 1.0),
            scrim: hsla(0.0, 0.0, 0.0, 0.30),
            ink,
            ink_secondary: hsla(0.09, 0.04, 0.36, 1.0),
            ink_tertiary: hsla(0.09, 0.03, 0.44, 1.0),
            hairline: hsla(0.0, 0.0, 0.0, 0.10),
            edge: hsla(0.0, 0.0, 0.0, 0.145),
            accent: ink,
            accent_text: ink,
            on_accent: hsla(0.0, 0.0, 0.99, 1.0),
            focus: hsla(0.0, 0.0, 0.0, 0.50),
            success: hsla(0.42, 0.62, 0.36, 1.0),
            warning: hsla(0.10, 0.78, 0.40, 1.0),
            danger: hsla(0.01, 0.70, 0.47, 1.0),
            danger_ink: hsla(0.0, 0.0, 0.99, 1.0),
            identity: identity_palette(false),
            font_sans: "Adwaita Sans".into(),
            font_mono: "Adwaita Mono".into(),
        }
    }

    pub fn for_appearance(appearance: Appearance) -> Self {
        match appearance {
            Appearance::Dark => Self::dark(),
            Appearance::Light => Self::light(),
        }
    }

    /// Install as the gpui global. The only place the appearance may change.
    #[allow(dead_code)]
    pub fn install(appearance: Appearance, cx: &mut App) {
        cx.set_global(Self::for_appearance(appearance));
    }

    pub fn of(cx: &App) -> &Theme {
        cx.global::<Theme>()
    }

    pub fn is_dark(&self) -> bool {
        self.appearance.is_dark()
    }

    /// A bot's own color. `slot` wraps, so any bot always gets one.
    pub fn identity(&self, slot: usize) -> Hsla {
        self.identity[slot % self.identity.len()]
    }

    /// Hover/pressed fills are ink at low opacity, so they work on every skin.
    pub fn hover_wash(&self) -> Hsla {
        self.ink.opacity(0.055)
    }

    /// The user's own message plate: a tint, not a second sidebar.
    #[allow(dead_code)]
    pub fn bubble(&self) -> Hsla {
        self.ink.opacity(if self.is_dark() { 0.07 } else { 0.05 })
    }
}

#[cfg(test)]
mod contrast_tests {
    use super::Theme;
    use gpui::{Hsla, Rgba};

    fn luminance(color: Hsla) -> f32 {
        let rgb = color.to_rgb();
        let linear = |channel: f32| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(rgb.r) + 0.7152 * linear(rgb.g) + 0.0722 * linear(rgb.b)
    }

    fn contrast(foreground: Hsla, background: Hsla) -> f32 {
        let (lighter, darker) = {
            let foreground = luminance(foreground);
            let background = luminance(background);
            if foreground >= background {
                (foreground, background)
            } else {
                (background, foreground)
            }
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    fn blend(foreground: Hsla, background: Hsla) -> Hsla {
        let foreground = foreground.to_rgb();
        let background = background.to_rgb();
        let alpha = foreground.a;
        Hsla::from(Rgba {
            r: foreground.r * alpha + background.r * (1.0 - alpha),
            g: foreground.g * alpha + background.g * (1.0 - alpha),
            b: foreground.b * alpha + background.b * (1.0 - alpha),
            a: 1.0,
        })
    }

    #[test]
    fn body_and_secondary_text_meet_wcag_aa_on_their_surfaces() {
        for theme in [Theme::dark(), Theme::light()] {
            for surface in [theme.bg, theme.panel, theme.card, theme.composer] {
                assert!(
                    contrast(theme.ink, surface) >= 4.5,
                    "primary text contrast: {:?}",
                    theme.appearance
                );
                assert!(
                    contrast(blend(theme.focus, surface), surface) >= 3.0,
                    "focus indicator contrast {:?}: {}",
                    theme.appearance,
                    contrast(blend(theme.focus, surface), surface)
                );
                assert!(
                    contrast(theme.ink_secondary, surface) >= 4.5,
                    "secondary text contrast: {:?}",
                    theme.appearance
                );
                assert!(
                    contrast(theme.ink_tertiary, surface) >= 4.5,
                    "tertiary text contrast {:?}: {}",
                    theme.appearance,
                    contrast(theme.ink_tertiary, surface)
                );
            }
            assert!(contrast(theme.on_user_plate, theme.user_plate) >= 4.5);
        }
    }
}

fn identity_palette(dark: bool) -> [Hsla; 8] {
    // Aucteris agent identity palette: warm red, orange and yellow alongside
    // green, cyan, blue, violet and magenta. Keep colors stable across skins.
    let _ = dark;
    [
        hsla(0.98310, 1.0, 0.57451, 1.0),
        hsla(0.06732, 1.0, 0.5, 1.0),
        hsla(0.09935, 1.0, 0.5, 1.0),
        hsla(0.42786, 1.0, 0.39412, 1.0),
        hsla(0.48050, 1.0, 0.36863, 1.0),
        hsla(0.58543, 0.99167, 0.52941, 1.0),
        hsla(0.72323, 0.98802, 0.67255, 1.0),
        hsla(0.91385, 1.0, 0.59412, 1.0),
    ]
}

/// Layout numbers. Spacing is a 4px grid (with a 2px half-step for hairlines),
/// control heights are 20/24/28/30/32.
pub mod metrics {
    /// Low-level pixel scale for one-off layout details. Prefer semantic
    /// tokens below when a control or spacing role is known.
    pub const ZERO: f32 = 0.0;
    pub const PX_1: f32 = 1.0;
    pub const PX_1_5: f32 = 1.5;
    pub const PX_2: f32 = 2.0;
    pub const PX_3: f32 = 3.0;
    pub const PX_5: f32 = 5.0;
    pub const PX_6: f32 = 6.0;
    pub const PX_9: f32 = 9.0;
    pub const PX_10: f32 = 10.0;
    pub const PX_10_5: f32 = 10.5;
    pub const PX_12_5: f32 = 12.5;
    pub const PX_18: f32 = 18.0;
    pub const PX_19: f32 = 19.0;
    pub const PX_23: f32 = 23.0;
    pub const PX_26: f32 = 26.0;
    pub const PX_32: f32 = 32.0;
    pub const PX_34: f32 = 34.0;
    pub const PX_36: f32 = 36.0;
    pub const PX_38: f32 = 38.0;
    pub const PX_40: f32 = 40.0;
    pub const PX_42: f32 = 42.0;
    pub const PX_56: f32 = 56.0;
    pub const PX_66: f32 = 66.0;
    pub const PX_111: f32 = 111.0;
    pub const PX_130: f32 = 130.0;
    pub const PX_160: f32 = 160.0;
    pub const PX_190: f32 = 190.0;
    pub const PX_220: f32 = 220.0;
    pub const PX_250: f32 = 250.0;
    pub const PX_320: f32 = 320.0;
    pub const PX_360: f32 = 360.0;

    pub const SIDEBAR_WIDTH: f32 = 256.0;
    pub const PANEL_WIDTH: f32 = 332.0;
    pub const PANEL_COLLAPSE_WIDTH: f32 = 1_100.0;
    /// Shared centered width for the transcript, status line, and composer.
    pub const CHAT_COLUMN_WIDTH: f32 = 900.0;
    pub const HEADER_HEIGHT: f32 = 44.0;

    pub const SPACE_1: f32 = 4.0;
    /// Half a step: stacked lines inside one chip or card.
    pub const SPACE_HALF: f32 = 2.0;
    pub const SPACE_2: f32 = 8.0;
    pub const SPACE_3: f32 = 12.0;
    pub const SPACE_4: f32 = 16.0;
    pub const SPACE_5: f32 = 24.0;

    pub const RADIUS_MENU_ROW: f32 = 6.0;
    pub const RADIUS_CONTROL: f32 = 6.0;
    pub const RADIUS_CARD: f32 = 12.0;
    pub const RADIUS_DIALOG: f32 = 14.0;
    pub const RADIUS_BUBBLE: f32 = 18.0;
    pub const RADIUS_COMPOSER: f32 = 26.0;

    pub const CONTROL_MD: f32 = 28.0;
    /// Form fields, which need room for a caret and a value.
    pub const CONTROL_LG: f32 = 32.0;
    /// A tag on a row: undo, counters.
    pub const CONTROL_XS: f32 = 20.0;
    pub const CONTROL_SM: f32 = 24.0;
    pub const MENU_ROW: f32 = 30.0;

    /// The composer capsule: prompt area above a compact controls rail.
    pub const COMPOSER_HEIGHT: f32 = 98.0;
    /// The composer's inner rail, between attach/input/send.
    pub const COMPOSER_GAP: f32 = 8.0;
    pub const COMPOSER_INSET_START: f32 = 16.0;
    pub const COMPOSER_INSET_END: f32 = 8.0;

    pub const MENU_WIDTH: f32 = 200.0;
    pub const MENU_ENTRY_HEIGHT: f32 = 44.0;
    pub const SIDEBAR_ROW_HEIGHT: f32 = 50.0;
    pub const SIDEBAR_BOT_ROW_HEIGHT: f32 = 68.0;
    /// The role field beside a bot's name.
    pub const ROLE_FIELD_WIDTH: f32 = 150.0;
    pub const DIALOG_MAX_HEIGHT: f32 = 600.0;

    /// One state mark on a row or in the status strip.
    pub const PRESENCE_DOT: f32 = 6.0;
    /// The streaming caret under a reply.
    pub const CARET_WIDTH: f32 = 7.0;

    pub const ICON_SM: f32 = 12.0;
    pub const ICON_MD: f32 = 16.0;
    pub const ICON_HERO: f32 = 34.0;
    pub const ICON_BUTTON: f32 = 26.0;
    pub const SWITCH_WIDTH: f32 = 34.0;
    pub const SWITCH_HEIGHT: f32 = 20.0;

    /// The mascot has one size per context, so a bot mark cannot drift.
    pub const MASCOT_STACK: f32 = 16.0;
    pub const MASCOT_HEADER: f32 = 20.0;
    pub const MASCOT_SWATCH: f32 = 22.0;
    pub const MASCOT_PICK: f32 = 26.0;
    pub const MASCOT_ROW: f32 = 28.0;
    pub const MASCOT_PANEL: f32 = 40.0;

    /// Widest a person's message plate can get.
    pub const BUBBLE_MAX_WIDTH: f32 = 760.0;
    /// The agent's cards: approval, question.
    pub const CARD_WIDTH: f32 = 422.0;
    /// Inset inside a card.
    pub const CARD_INSET: f32 = 16.0;
    /// A centered block of prose on the empty state.
    pub const PROSE_WIDTH: f32 = 440.0;

    /// One inset per surface: everything starts on this left edge.
    pub const GUTTER: f32 = 12.0;
}

/// Motion. GPUI has one duration per `Animation` and an easing function;
/// `AnimationExt` already renders a static frame when `App::reduce_motion` is
/// set, so these values never need a second reduced-motion branch.
#[allow(dead_code)]
pub mod motion {
    use std::time::Duration;

    /// Press, hover and other immediate acknowledgements.
    pub const FAST: Duration = Duration::from_millis(120);
    /// Cards, menus and panel chrome.
    pub const BASE: Duration = Duration::from_millis(180);
    /// Surfaces that travel further (dialogs, sheets).
    pub const SLOW: Duration = Duration::from_millis(260);

    /// The one deceleration curve: starts quickly, settles gently.
    pub fn ease_out() -> impl Fn(f32) -> f32 {
        gpui::ease_out_quint()
    }
}

/// The type table. No other sizes exist in the app.
pub mod text {
    pub const SHEET_TITLE: f32 = 17.0;
    pub const BUBBLE: f32 = 15.0;
    pub const BUBBLE_LINE: f32 = 22.0;
    #[allow(dead_code)]
    pub const MENU_ITEM: f32 = 14.0;
    pub const BODY: f32 = 13.0;
    pub const META: f32 = 12.0;
    pub const META_LINE: f32 = 16.0;
    pub const TINY: f32 = 11.0;
}
