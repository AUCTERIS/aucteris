//! Shared components. Anything used twice lives here, so a row in the sidebar
//! and a row in a menu cannot drift apart.

use gpui::{
    Animation, AnimationExt, AnyElement, App, ClickEvent, FontWeight, Hsla, IntoElement,
    SharedString, Window, div, prelude::*, px, svg,
};
use std::time::Duration;

use crate::domain::model::{MascotShape, Presence};
use crate::presentation::theme::{Theme, metrics, text as typo};

/// A mask-shaped icon from the embedded set, tinted to `color`.
pub fn icon(glyph: &'static str, size: f32, color: Hsla) -> AnyElement {
    svg()
        .path(SharedString::from(format!("icons/{glyph}.svg")))
        .flex_shrink_0()
        .w(px(size))
        .h(px(size))
        .text_color(color)
        .into_any_element()
}

/// A softly beveled mascot cube. Working bots blink; the cube stays still on hover.
pub fn mascot(
    shape: MascotShape,
    color: Hsla,
    presence: Presence,
    size: f32,
    id: usize,
) -> AnyElement {
    let shape_id = shape.id_str();
    let shape_path = SharedString::from(format!("icons/mascots/shape-{shape_id}.svg"));
    let volume_path = SharedString::from(format!("icons/mascots/volume-{shape_id}.svg"));
    let shade = gpui::hsla(
        color.h,
        color.s,
        (color.l * 0.48).clamp(0.05, 0.36),
        color.a,
    );
    let shadow = svg()
        .path(shape_path.clone())
        .absolute()
        .left_0()
        .top(px(size * 0.07))
        .size(px(size))
        .text_color(shade);
    let body = svg()
        .path(shape_path)
        .absolute()
        .left_0()
        .top_0()
        .size(px(size))
        .text_color(color);
    let volume = svg()
        .path(volume_path)
        .absolute()
        .left_0()
        .top_0()
        .size(px(size));
    let (eye_y, eye_x_left, eye_x_right, _) = mascot_eye_geometry(shape);
    let eye_width = size * 0.145;
    let eye_height = size * 0.185;
    let eye_top = eye_y * size / 32.0 - eye_height * 0.5;
    let eye = |left: f32| {
        div()
            .absolute()
            .left(px(left * size / 32.0 - eye_width * 0.5))
            .top(px(eye_top))
            .w(px(eye_width))
            .h(px(eye_height))
            .rounded(px(size * 0.07))
            .bg(gpui::hsla(0.0, 0.0, 1.0, 1.0))
    };
    let eyes = div()
        .absolute()
        .inset_0()
        .child(eye(eye_x_left))
        .child(eye(eye_x_right));
    let eyes: AnyElement = if presence == Presence::Working {
        // A short eyelid close at the end of a slow, per-bot cycle.
        let period = 3_200 + (id as u64 % 5) * 180;
        eyes.with_animation(
            ("mascot-eye-blink", id),
            Animation::new(Duration::from_millis(period))
                .repeat()
                .with_easing(|progress| match progress {
                    p if (0.92..0.94).contains(&p) => 0.45,
                    p if (0.94..0.965).contains(&p) => 0.02,
                    p if (0.965..0.985).contains(&p) => 0.45,
                    _ => 1.0,
                }),
            |element, opacity| element.opacity(opacity),
        )
        .into_any_element()
    } else {
        eyes.into_any_element()
    };
    div()
        .relative()
        .flex_shrink_0()
        .size(px(size))
        .child(shadow)
        .child(body)
        .child(volume)
        .child(eyes)
        .into_any_element()
}

fn mascot_eye_geometry(shape: MascotShape) -> (f32, f32, f32, f32) {
    match shape {
        MascotShape::Circle
        | MascotShape::Diamond
        | MascotShape::Hexagon
        | MascotShape::Octagon
        | MascotShape::Square => (16.0, 12.2, 19.8, 2.8),
        MascotShape::Pentagon => (17.5, 12.2, 19.8, 2.8),
        MascotShape::Star => (17.0, 12.9, 19.1, 2.2),
        MascotShape::Triangle => (20.0, 12.2, 19.8, 2.8),
    }
}

/// A room's mark: a small team glyph colored by its first member.
pub fn room_mark(color: Hsla, size: f32, dark: bool) -> AnyElement {
    div()
        .flex_shrink_0()
        .size(px(size))
        .rounded_full()
        .bg(color.opacity(if dark { 0.16 } else { 0.13 }))
        .flex()
        .items_center()
        .justify_center()
        .child(icon("users", size * 0.52, color))
        .into_any_element()
}

/// Three states, one mark. Resting is nearly silent on purpose.
pub fn presence_dot(presence: Presence, theme: &Theme, size: f32) -> AnyElement {
    let color = match presence {
        Presence::Working => theme.success,
        Presence::Waiting => theme.warning,
        Presence::Resting => theme.ink_tertiary.opacity(0.45),
    };
    div()
        .flex_shrink_0()
        .size(px(size))
        .rounded_full()
        .bg(color)
        .into_any_element()
}

/// A small static pill: a model name, a machine, a word.
#[allow(dead_code)]
pub fn chip(label: impl Into<SharedString>, theme: &Theme) -> AnyElement {
    div()
        .flex_shrink_0()
        .h(px(metrics::CONTROL_XS))
        .px(px(metrics::SPACE_2))
        .flex()
        .items_center()
        .rounded_full()
        .border_1()
        .border_color(theme.hairline)
        .text_size(px(typo::TINY))
        .text_color(theme.ink_secondary)
        .child(label.into())
        .into_any_element()
}

/// A pill with a leading glyph, for "which machine" style facts.
#[allow(dead_code)]
pub fn chip_with_icon(
    glyph: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
) -> AnyElement {
    div()
        .flex_shrink_0()
        .h(px(metrics::CONTROL_XS))
        .px(px(metrics::SPACE_2))
        .flex()
        .items_center()
        .gap(px(metrics::PX_5))
        .rounded_full()
        .border_1()
        .border_color(theme.hairline)
        .text_size(px(typo::TINY))
        .text_color(theme.ink_secondary)
        .child(icon(glyph, 11.0, theme.ink_tertiary))
        .child(label.into())
        .into_any_element()
}

/// The one shape an icon-only action takes.
pub fn icon_button<F>(
    id: (&'static str, usize),
    glyph: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
    on_click: F,
) -> AnyElement
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let hover = theme.hover_wash();
    let color = theme.ink_secondary;
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .tab_index(0)
        .focus_visible({
            let focus = theme.focus;
            move |style| style.border_1().border_color(focus)
        })
        .flex_shrink_0()
        .size(px(metrics::ICON_BUTTON))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(metrics::RADIUS_CONTROL))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .on_click(on_click)
        .child(icon(glyph, 16.0, color))
        .into_any_element()
}

/// A pill that is one choice among a few: skins, interface mode, machines.
/// Selection is a fill plus a stronger edge, never a hue.
pub fn segment<F>(
    id: (&'static str, usize),
    label: impl Into<SharedString>,
    selected: bool,
    theme: &Theme,
    on_click: F,
) -> AnyElement
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let hover = theme.hover_wash();
    let label = label.into();
    let focus = theme.focus;
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .aria_selected(selected)
        .tab_index(0)
        .focus_visible(move |style| style.border_1().border_color(focus))
        .flex_shrink_0()
        .h(px(metrics::CONTROL_XS))
        .px(px(metrics::PX_10))
        .flex()
        .items_center()
        .rounded_full()
        .border_1()
        .border_color(if selected { theme.edge } else { theme.hairline })
        .when(selected, |this| this.bg(theme.ink.opacity(0.07)))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .text_size(px(typo::TINY))
        .font_weight(if selected {
            FontWeight::MEDIUM
        } else {
            FontWeight::NORMAL
        })
        .text_color(if selected {
            theme.ink
        } else {
            theme.ink_secondary
        })
        .on_click(on_click)
        .child(label)
        .into_any_element()
}

/// A two-state switch. The track carries the state; nothing animates.
pub fn switch<F>(
    id: (&'static str, usize),
    on: bool,
    label: impl Into<SharedString>,
    theme: &Theme,
    on_click: F,
) -> AnyElement
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let track = if on {
        theme.ink.opacity(0.85)
    } else {
        theme.ink.opacity(0.16)
    };
    let knob = if on {
        theme.on_accent
    } else {
        theme.ink_secondary
    };
    let focus = theme.focus;
    div()
        .id(id)
        .role(gpui::Role::CheckBox)
        .aria_label(label)
        .aria_toggled(if on {
            gpui::Toggled::True
        } else {
            gpui::Toggled::False
        })
        .tab_index(0)
        .focus_visible(move |style| style.border_1().border_color(focus))
        .flex_shrink_0()
        .w(px(metrics::SWITCH_WIDTH))
        .h(px(metrics::SWITCH_HEIGHT))
        .p(px(metrics::PX_2))
        .flex()
        .items_center()
        .when(on, |this| this.justify_end())
        .rounded_full()
        .bg(track)
        .cursor_pointer()
        .on_click(on_click)
        .child(div().size(px(metrics::SPACE_4)).rounded_full().bg(knob))
        .into_any_element()
}

/// A keyboard key, as text. Mono, so the letters sit on one baseline.
pub fn keycap(label: impl Into<SharedString>, theme: &Theme) -> AnyElement {
    div()
        .flex_shrink_0()
        .h(px(metrics::CONTROL_MD))
        .px(px(metrics::PX_6))
        .flex()
        .items_center()
        .rounded(px(metrics::PX_5))
        .bg(theme.ink.opacity(0.05))
        .border_1()
        .border_color(theme.hairline)
        .font_family(theme.font_mono.clone())
        .text_size(px(typo::TINY))
        .text_color(theme.ink_secondary)
        .child(label.into())
        .into_any_element()
}

/// A group label. Sentence case, 12px, medium — never ALL CAPS.
pub fn section_label(label: impl Into<SharedString>, theme: &Theme) -> AnyElement {
    div()
        .text_size(px(typo::META))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.ink_tertiary)
        .child(label.into())
        .into_any_element()
}

/// A 1px vertical rule between panes.
pub fn edge(color: Hsla) -> AnyElement {
    div()
        .w(px(metrics::PX_1))
        .h_full()
        .flex_shrink_0()
        .bg(color)
        .into_any_element()
}

/// A usage bar: one fill, one track, tabular numbers beside it.
pub fn meter(fraction: f32, theme: &Theme) -> AnyElement {
    let fraction = fraction.clamp(0.0, 1.0);
    let track = theme.ink.opacity(if theme.is_dark() { 0.08 } else { 0.07 });
    div()
        .h(px(metrics::SPACE_1))
        .w_full()
        .rounded_full()
        .bg(track)
        .child(
            div()
                .h(px(metrics::SPACE_1))
                .rounded_full()
                .bg(theme.ink_secondary.opacity(0.85))
                .w(gpui::relative(fraction)),
        )
        .into_any_element()
}

/// `769k` style numbers, so the meter does not jitter.
pub fn compact_count(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{}k", value / 1_000)
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::compact_count;

    #[test]
    fn compact_count_scales_units() {
        assert_eq!(compact_count(0), "0");
        assert_eq!(compact_count(999), "999");
        assert_eq!(compact_count(1_000), "1k");
        assert_eq!(compact_count(12_400), "12k");
        assert_eq!(compact_count(999_999), "999k");
        assert_eq!(compact_count(1_000_000), "1.0M");
        assert_eq!(compact_count(2_500_000), "2.5M");
    }
}
