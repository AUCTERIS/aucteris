//! The strip under the transcript: live status while a run is in flight,
//! sandbox/folder context otherwise, with the Stop button.

use gpui::{AnyElement, Context, SharedString, Styled, div, prelude::*, px};

use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets;

impl Shell {
    pub(super) fn status_strip(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let running = self.running();
        let label: SharedString = match (&self.status, running) {
            (Some(status), true) => status.clone(),
            _ => {
                let label = self
                    .workspace
                    .selected_bot()
                    .and_then(|id| self.workspace.bot(id))
                    .map(|bot| {
                        format!(
                            "{} · {}",
                            crate::domain::model::sandbox_label(bot.sandbox),
                            bot.folder
                        )
                    })
                    .unwrap_or_else(|| "A room, shared by everyone in it".to_string());
                SharedString::from(label)
            }
        };

        let hover = theme.hover_wash();
        let stop_color = theme.ink_secondary;

        div()
            .h(px(metrics::STATUS_HEIGHT))
            .flex_shrink_0()
            .child(
                div()
                    .w_full()
                    .max_w(px(metrics::CHAT_COLUMN_WIDTH))
                    .mx_auto()
                    .px(px(metrics::SPACE_4))
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(metrics::SPACE_2))
                    .child(if running {
                        widgets::presence_dot(crate::domain::model::Presence::Working, theme, 6.0)
                    } else {
                        div()
                            .size(px(6.0))
                            .rounded_full()
                            .bg(theme.ink_tertiary.opacity(0.35))
                            .into_any_element()
                    })
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(typo::TINY))
                            .text_color(theme.ink_tertiary)
                            .child(label),
                    )
                    .when(running, |this| {
                        this.child(
                            div()
                                .id("stop-run")
                                .flex_shrink_0()
                                .h(px(metrics::CONTROL_XS))
                                .px(px(metrics::SPACE_2))
                                .flex()
                                .items_center()
                                .gap(px(metrics::SPACE_1))
                                .rounded_full()
                                .border_1()
                                .border_color(theme.hairline)
                                .cursor_pointer()
                                .hover(move |style| style.bg(hover))
                                .text_size(px(typo::TINY))
                                .text_color(stop_color)
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.dispatch(
                                        crate::presentation::ui::action::UiAction::StopRun,
                                        cx,
                                    )
                                }))
                                .child(widgets::icon("stop", 9.0, stop_color))
                                .child("Stop"),
                        )
                    })
                    .into_any_element(),
            )
            .into_any_element()
    }
}
