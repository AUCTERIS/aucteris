use super::super::activity::{activity_row, activity_summary};
use crate::domain::model::{Message, Part, is_internal_tool};
use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, px};

impl Shell {
    pub(super) fn bot_message_view(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        chat_index: usize,
        message_index: usize,
        message: &Message,
    ) -> AnyElement {
        let has_approval = message
            .parts
            .iter()
            .any(|part| matches!(part, Part::Approval { .. }));
        let is_active_run = message.streaming
            && self.run_chat.is_some_and(|run_chat| {
                self.workspace
                    .chats
                    .get(chat_index)
                    .is_some_and(|chat| chat.id == run_chat)
            });
        let mut response_body: Vec<AnyElement> = Vec::new();
        let mut activity_details: Vec<AnyElement> = Vec::new();
        for (part_index, part) in message.parts.iter().enumerate() {
            let is_activity = matches!(
                part,
                Part::Thought { .. } | Part::Tool { .. } | Part::Files { .. } | Part::Note { .. }
            );
            if is_activity && !message.activity_expanded {
                continue;
            }
            if matches!(part, Part::Tool { label, detail, .. } if is_internal_tool(label, detail.as_deref()))
            {
                continue;
            }
            let rendered = self.part_view(
                theme,
                cx,
                (chat_index, message_index, part_index),
                part,
                has_approval,
            );
            if is_activity {
                activity_details.push(rendered);
            } else {
                response_body.push(rendered);
            }
        }

        let mut body: Vec<AnyElement> = Vec::new();
        if let Some(summary) = activity_summary(
            message,
            is_active_run,
            self.status.as_deref(),
            message.parts.len(),
        ) {
            body.push(activity_row(
                summary,
                message.activity_expanded,
                is_active_run,
                theme,
                cx,
                chat_index,
                message_index,
            ));
        }
        if message.activity_expanded && !activity_details.is_empty() {
            body.push(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(metrics::SPACE_1))
                    .children(activity_details)
                    .into_any_element(),
            );
        }
        body.extend(response_body);
        if message.streaming {
            body.push(
                div()
                    .mt(px(metrics::SPACE_HALF))
                    .w(px(metrics::CARET_WIDTH))
                    .h(px(typo::BUBBLE))
                    .rounded(px(2.0))
                    .bg(theme.ink_secondary.opacity(0.55))
                    .into_any_element(),
            );
        }
        if message.stopped {
            body.push(
                div()
                    .text_size(px(typo::META))
                    .text_color(theme.ink_tertiary)
                    .child("Stopped on request")
                    .into_any_element(),
            );
        }
        div()
            .w_full()
            .flex()
            .justify_start()
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .max_w(px(metrics::BUBBLE_MAX_WIDTH))
                    .flex()
                    .flex_col()
                    .gap(px(metrics::SPACE_2))
                    .children(body),
            )
            .into_any_element()
    }
}
