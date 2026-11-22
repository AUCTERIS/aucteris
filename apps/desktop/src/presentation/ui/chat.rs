//! The chat: header, transcript, status strip, composer.
//!
//! A reply is one bubble-less column: the bot's words and the steps it took sit
//! in the same stream, because the work is the answer. Only the person's own
//! messages get a plate, so the eye always knows who is talking.

mod cards;
mod message;
mod status;

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement, Styled, Window, div, prelude::*,
    px,
};

use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets;

impl Shell {
    pub(crate) fn view_chat(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.workspace.chats.is_empty() {
            let has_cli = !self.available_runtimes.is_empty();
            let body = if has_cli {
                "Create a bot, choose an installed agent CLI, and send it a task in your working folder."
            } else {
                "Install Claude Code, Codex CLI, OpenCode, or Gemini CLI and make it available on PATH to start."
            };
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(metrics::SPACE_3))
                .bg(theme.bg)
                .child(widgets::icon("bot", 34.0, theme.ink_tertiary))
                .child(
                    div()
                        .text_size(px(typo::SHEET_TITLE))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.ink)
                        .child("Your agent workspace is ready"),
                )
                .child(
                    div()
                        .max_w(px(metrics::PROSE_WIDTH))
                        .text_center()
                        .text_size(px(typo::BODY))
                        .text_color(theme.ink_secondary)
                        .child(body),
                )
                .when(has_cli, |this| {
                    this.child(
                        div()
                            .id("empty-create-bot")
                            .mt(px(metrics::SPACE_2))
                            .px(px(metrics::SPACE_4))
                            .h(px(metrics::CONTROL_LG))
                            .flex()
                            .items_center()
                            .rounded_full()
                            .bg(theme.accent)
                            .text_color(theme.on_accent)
                            .cursor_pointer()
                            .on_click(
                                cx.listener(|this, _event, window, cx| {
                                    this.open_new_bot(window, cx)
                                }),
                            )
                            .child("Create your first bot"),
                    )
                })
                .into_any_element();
        }
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(theme.bg)
            .child(self.chat_header(theme, cx))
            .child(self.transcript(theme, cx))
            .when(self.running(), |this| {
                this.child(self.status_strip(theme, cx))
            })
            .child(self.view_composer(theme, window, cx))
            .into_any_element()
    }

    pub(crate) fn chat_name(&self) -> String {
        self.workspace.chat().title.to_string()
    }

    fn chat_header(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let chat = self.workspace.chat().clone();
        div()
            .h(px(metrics::HEADER_HEIGHT))
            .flex_shrink_0()
            .px(px(metrics::SPACE_3))
            .bg(theme.panel)
            .border_b_1()
            .border_color(theme.hairline)
            .flex()
            .items_center()
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(metrics::SPACE_2))
                    .h(px(metrics::CONTROL_MD))
                    .px(px(metrics::SPACE_2))
                    .rounded_full()
                    .bg(theme.card)
                    .child(
                        if let Some(bot) = chat.bot.and_then(|id| self.workspace.bot(id)) {
                            widgets::mascot(
                                bot.mascot,
                                theme.identity(bot.color),
                                bot.presence,
                                20.0,
                                bot.id as usize * 1000 + 4,
                            )
                        } else if !chat.members.is_empty() {
                            let mut stack = div().flex().items_center().gap(px(2.0));
                            for id in chat.members.iter().take(3) {
                                if let Some(member) = self.workspace.bot(*id) {
                                    stack = stack.child(widgets::mascot(
                                        member.mascot,
                                        theme.identity(member.color),
                                        member.presence,
                                        16.0,
                                        member.id as usize * 1000 + 4,
                                    ));
                                }
                            }
                            stack.into_any_element()
                        } else {
                            widgets::icon("bot", 16.0, theme.ink_secondary)
                        },
                    )
                    .child(
                        div()
                            .text_size(px(typo::BODY))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.ink)
                            .child(chat.title.to_string()),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("toggle-right-panel")
                    .size(px(metrics::CONTROL_MD))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(metrics::RADIUS_CONTROL))
                    .when(self.panel_open, |this| this.bg(theme.raised))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.raised))
                    .on_click(cx.listener(|this, _event, _window, cx| this.toggle_panel(cx)))
                    .child(widgets::icon("panel", 16.0, theme.ink_secondary)),
            )
            .into_any_element()
    }

    fn transcript(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let chat = self.workspace.chat();
        if chat.messages.is_empty() {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .px(px(metrics::SPACE_5))
                .child(
                    div()
                        .text_size(px(typo::BODY))
                        .text_color(theme.ink_tertiary)
                        .child(format!(
                            "Nothing here yet. Say what you need and {} starts.",
                            self.chat_name()
                        )),
                )
                .into_any_element();
        }

        let chat_index = self.workspace.selected;
        let mut children: Vec<AnyElement> = Vec::new();
        for (message_index, message) in chat.messages.iter().enumerate() {
            children.push(self.message_view(theme, cx, chat_index, message_index, message));
        }

        div()
            .id("transcript")
            .flex_1()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(
                div()
                    .w_full()
                    .max_w(px(metrics::CHAT_COLUMN_WIDTH))
                    .mx_auto()
                    .px(px(metrics::SPACE_4))
                    .py(px(metrics::SPACE_1))
                    .flex()
                    .flex_col()
                    .gap(px(metrics::SPACE_2))
                    .children(children),
            )
            .into_any_element()
    }
}
