use super::code::{public_tool_detail, public_tool_label, pulsing_icon};
use crate::domain::model::{Message, Part, is_internal_tool};
use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets;
use gpui::{
    Animation, AnimationExt, AnyElement, Context, IntoElement, ParentElement, Styled, div,
    prelude::*, px,
};
use std::time::Duration;

pub(super) fn activity_summary(
    message: &Message,
    is_active: bool,
    status: Option<&str>,
    fallback_index: usize,
) -> Option<(String, &'static str, Option<u64>, usize)> {
    if !message.streaming
        && let Some(duration_ms) = message.duration_ms
    {
        return Some((
            format!("Durée d’exécution : {}", human_duration(duration_ms)),
            "clock",
            None,
            fallback_index,
        ));
    }
    if is_active && matches!(message.parts.last(), Some(Part::Text(_))) {
        return Some((
            "Writing response".into(),
            "edit",
            message
                .started_at
                .map(|start| start.elapsed().as_millis() as u64),
            fallback_index,
        ));
    }
    if is_active && let Some(status) = status {
        let status = status.to_ascii_lowercase();
        let current_phase = if status.contains("think") {
            Some(("Reasoning", "sparkle"))
        } else if status.contains("writ") || status.contains("stream") {
            Some(("Writing response", "edit"))
        } else if status.contains("approval") {
            Some(("Waiting for approval", "check"))
        } else if status.contains("answer") {
            Some(("Waiting for your answer", "mail"))
        } else {
            None
        };
        if let Some((label, icon)) = current_phase {
            return Some((
                label.into(),
                icon,
                message
                    .started_at
                    .map(|start| start.elapsed().as_millis() as u64),
                fallback_index,
            ));
        }
    }
    for (index, part) in message.parts.iter().enumerate().rev() {
        match part {
            Part::Thought { .. } => {
                return Some(("Reasoning".into(), "sparkle", None, index));
            }
            Part::Tool {
                icon,
                label,
                detail,
                duration_ms,
                started_at,
                ..
            } if !is_internal_tool(label, detail.as_deref()) => {
                let label = public_tool_label(label);
                let detail = public_tool_detail(label.as_str(), detail.as_deref());
                let title = detail.map_or(label.clone(), |detail| format!("{label} · {detail}"));
                let elapsed = duration_ms
                    .or_else(|| started_at.map(|start| start.elapsed().as_millis() as u64));
                return Some((title, icon, elapsed, index));
            }
            Part::Files { path, .. } => {
                let file = std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(path);
                return Some((format!("Edited {file}"), "file", None, index));
            }
            Part::Note { .. } => return Some(("Updated workspace".into(), "sparkle", None, index)),
            Part::Approval { .. } => {
                return Some(("Waiting for approval".into(), "check", None, index));
            }
            Part::Question { .. } => {
                return Some(("Waiting for your answer".into(), "mail", None, index));
            }
            _ => {}
        }
    }
    if is_active {
        return Some(("Working".into(), "sparkle", None, fallback_index));
    }
    None
}

fn human_duration(duration_ms: u64) -> String {
    let seconds = duration_ms / 1000;
    if seconds >= 3600 {
        format!("{}h {}min", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}min {}s", seconds / 60, seconds % 60)
    } else {
        format!("{:.1}s", duration_ms as f64 / 1000.0)
    }
}

pub(super) fn activity_row(
    summary: (String, &'static str, Option<u64>, usize),
    expanded: bool,
    live: bool,
    theme: &Theme,
    cx: &mut Context<Shell>,
    chat_index: usize,
    message_index: usize,
) -> AnyElement {
    let (label, icon, duration_ms, event_index) = summary;
    let label_salt = label.bytes().fold(0usize, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(byte as usize)
    });
    let motion_id = chat_index * 1_000_000 + message_index * 1_000 + event_index + label_salt;
    let mark = widgets::icon(
        icon,
        metrics::ICON_SM,
        if live {
            theme.accent
        } else {
            theme.ink_secondary
        },
    );
    let mark = if live {
        pulsing_icon(mark, ("activity-pulse", motion_id))
    } else {
        mark
    };
    let chat = chat_index;
    let message = message_index;
    let label_view = div()
        .min_w_0()
        .truncate()
        .text_size(px(typo::META))
        .text_color(theme.ink_secondary)
        .child(label)
        .with_animation(
            ("activity-swap", motion_id),
            Animation::new(Duration::from_millis(180)),
            |element, progress| element.opacity(0.65 + progress * 0.35),
        );
    div()
        .id(("activity-toggle", message_index))
        .w_full()
        .min_w_0()
        .h(px(metrics::CONTROL_MD))
        .mt(px(metrics::SPACE_1))
        .flex()
        .items_center()
        .gap(px(metrics::SPACE_2))
        .cursor_pointer()
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.dispatch(
                crate::presentation::ui::action::UiAction::ToggleActivity { chat, message },
                cx,
            );
        }))
        .child(mark)
        .child(
            div()
                .min_w_0()
                .max_w(px(metrics::BUBBLE_MAX_WIDTH - 72.0))
                .child(label_view),
        )
        .child(widgets::icon(
            if expanded {
                "chevron-down"
            } else {
                "chevron-right"
            },
            9.0,
            theme.ink_tertiary,
        ))
        .child(div().flex_1())
        .when_some(duration_ms, |this, duration_ms| {
            this.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(typo::TINY))
                    .text_color(theme.ink_tertiary)
                    .child(format!("{:.1}s", duration_ms as f64 / 1000.0)),
            )
        })
        .into_any_element()
}
