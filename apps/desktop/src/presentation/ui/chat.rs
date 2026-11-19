//! The chat: header, transcript, status strip, composer.
//!
//! A reply is one bubble-less column: the bot's words and the steps it took sit
//! in the same stream, because the work is the answer. Only the person's own
//! messages get a plate, so the eye always knows who is talking.

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement, SharedString, Styled, Window, div,
    prelude::*, px,
};

use crate::domain::model::{Author, Message, Part};
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

    fn message_view(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        chat_index: usize,
        message_index: usize,
        message: &Message,
    ) -> AnyElement {
        match message.author {
            Author::You => div()
                .w_full()
                .flex()
                .justify_end()
                .child(
                    div()
                        .max_w(px(metrics::BUBBLE_MAX_WIDTH))
                        .px(px(metrics::SPACE_3))
                        .py(px(metrics::SPACE_2))
                        .rounded(px(metrics::RADIUS_BUBBLE))
                        .bg(theme.user_plate)
                        .text_size(px(typo::BUBBLE))
                        .line_height(px(typo::BUBBLE_LINE))
                        .text_color(theme.on_user_plate)
                        .child(message.text()),
                )
                .into_any_element(),
            Author::System => div()
                .w_full()
                .flex()
                .justify_center()
                .child(
                    div()
                        .text_size(px(typo::META))
                        .text_color(theme.ink_tertiary)
                        .child(message.text()),
                )
                .into_any_element(),
            Author::Bot => {
                let has_approval = message
                    .parts
                    .iter()
                    .any(|part| matches!(part, Part::Approval { .. }));
                let mut body: Vec<AnyElement> = Vec::new();
                for (part_index, part) in message.parts.iter().enumerate() {
                    body.push(match part {
                        Part::Text(text) => div()
                            .when(has_approval, |this| {
                                this.max_w(px(metrics::BUBBLE_MAX_WIDTH))
                                    .px(px(metrics::SPACE_3))
                                    .py(px(metrics::SPACE_2))
                                    .rounded(px(metrics::RADIUS_BUBBLE))
                                    .bg(theme.card)
                            })
                            .text_size(px(typo::BUBBLE))
                            .line_height(px(typo::BUBBLE_LINE))
                            .text_color(theme.ink)
                            .child(text.clone())
                            .into_any_element(),
                        Part::Thought { text, collapsed } => div()
                            .flex()
                            .flex_col()
                            .gap(px(metrics::SPACE_1))
                            .px(px(metrics::SPACE_3))
                            .py(px(metrics::SPACE_2))
                            .rounded(px(metrics::RADIUS_CARD))
                            .bg(theme.ink.opacity(0.035))
                            .border_1()
                            .border_color(theme.hairline)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(metrics::SPACE_2))
                                    .child(widgets::icon("sparkle", 12.0, theme.accent))
                                    .child(
                                        div()
                                            .text_size(px(typo::META))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.ink_secondary)
                                            .child("Reasoning process"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(typo::TINY))
                                            .text_color(theme.ink_tertiary)
                                            .child(if *collapsed { "Show" } else { "Collapse" }),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(typo::BODY))
                                    .line_height(px(typo::BUBBLE_LINE))
                                    .text_color(theme.ink_secondary)
                                    .child(text.clone()),
                            )
                            .into_any_element(),
                        Part::Tool {
                            icon,
                            label,
                            detail,
                            done,
                            result,
                        } => div()
                            .flex()
                            .flex_col()
                            .gap(px(metrics::SPACE_HALF))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(metrics::SPACE_2))
                                    .h(px(metrics::CONTROL_SM))
                                    .px(px(metrics::SPACE_2))
                                    .rounded(px(metrics::RADIUS_MENU_ROW))
                                    .bg(theme.ink.opacity(0.045))
                                    .border_1()
                                    .border_color(theme.hairline)
                                    .child(widgets::icon(
                                        icon,
                                        13.0,
                                        if *done {
                                            theme.ink_tertiary
                                        } else {
                                            theme.success
                                        },
                                    ))
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .font_family(theme.font_mono.clone())
                                            .text_size(px(typo::META))
                                            .text_color(if *done {
                                                theme.ink_secondary
                                            } else {
                                                theme.ink
                                            })
                                            .child(label.clone()),
                                    )
                                    .when_some(detail.clone(), |this, detail| {
                                        this.child(
                                            div()
                                                .truncate()
                                                .text_size(px(typo::TINY))
                                                .text_color(theme.ink_tertiary)
                                                .child(format!("· {detail}")),
                                        )
                                    }),
                            )
                            .when_some(result.clone(), |this, result| {
                                this.child(
                                    div()
                                        .px(px(metrics::SPACE_1))
                                        .text_size(px(typo::TINY))
                                        .text_color(theme.ink_tertiary)
                                        .child(result),
                                )
                            })
                            .into_any_element(),
                        Part::Files {
                            path,
                            added,
                            removed,
                            undone,
                        } => div()
                            .flex()
                            .items_center()
                            .gap(px(metrics::SPACE_2))
                            .h(px(metrics::CONTROL_SM))
                            .child(widgets::icon("file", 13.0, theme.ink_tertiary))
                            .child(
                                div()
                                    .flex_1()
                                    .truncate()
                                    .font_family(theme.font_mono.clone())
                                    .text_size(px(typo::META))
                                    .text_color(if *undone {
                                        theme.ink_tertiary
                                    } else {
                                        theme.ink_secondary
                                    })
                                    .child(path.clone()),
                            )
                            .when(!*undone, |this| {
                                this.child(
                                    div()
                                        .text_size(px(typo::TINY))
                                        .text_color(theme.success)
                                        .child(format!("+{added}")),
                                )
                                .child(
                                    div()
                                        .text_size(px(typo::TINY))
                                        .text_color(theme.danger)
                                        .child(format!("-{removed}")),
                                )
                                .child(self.undo_button(
                                    theme,
                                    cx,
                                    chat_index,
                                    message_index,
                                    part_index,
                                ))
                            })
                            .when(*undone, |this| {
                                this.child(
                                    div()
                                        .text_size(px(typo::TINY))
                                        .text_color(theme.ink_tertiary)
                                        .child("Undone"),
                                )
                            })
                            .into_any_element(),
                        Part::Note { text, undone } => div()
                            .flex()
                            .items_center()
                            .gap(px(metrics::SPACE_2))
                            .h(px(metrics::CONTROL_SM))
                            .child(widgets::icon("sparkle", 13.0, theme.ink_tertiary))
                            .child(
                                div()
                                    .flex_1()
                                    .truncate()
                                    .text_size(px(typo::META))
                                    .text_color(if *undone {
                                        theme.ink_tertiary
                                    } else {
                                        theme.ink_secondary
                                    })
                                    .child(text.clone()),
                            )
                            .when(!*undone, |this| {
                                this.child(self.undo_button(
                                    theme,
                                    cx,
                                    chat_index,
                                    message_index,
                                    part_index,
                                ))
                            })
                            .when(*undone, |this| {
                                this.child(
                                    div()
                                        .text_size(px(typo::TINY))
                                        .text_color(theme.ink_tertiary)
                                        .child("Undone"),
                                )
                            })
                            .into_any_element(),
                        Part::Approval {
                            kicker,
                            title,
                            detail,
                            request,
                            choices,
                            resolved,
                        } => {
                            let pending = resolved.is_none() && request.is_some();
                            let resolution = resolved.clone();
                            let buttons = choices.clone();
                            div()
                                .id(("approval-card", message_index * 32 + part_index))
                                .w(px(metrics::CARD_WIDTH))
                                .max_w(px(metrics::BUBBLE_MAX_WIDTH))
                                .p(px(metrics::CARD_INSET))
                                .rounded(px(metrics::RADIUS_BUBBLE))
                                .bg(theme.card)
                                .border_1()
                                .border_color(theme.hairline)
                                .flex()
                                .flex_col()
                                .gap(px(metrics::SPACE_1))
                                .child(
                                    div()
                                        .text_size(px(typo::TINY))
                                        .text_color(theme.ink_tertiary)
                                        .child(kicker.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(typo::BODY))
                                        .text_color(theme.ink)
                                        .child(title.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(typo::META))
                                        .text_color(theme.ink_tertiary)
                                        .child(detail.clone()),
                                )
                                .child(
                                    div()
                                        .mt(px(metrics::SPACE_1))
                                        .flex()
                                        .flex_wrap()
                                        .items_center()
                                        .gap(px(metrics::SPACE_2))
                                        .when_some(resolution, |this, resolution| {
                                            this.child(
                                                div()
                                                    .text_size(px(typo::META))
                                                    .text_color(theme.success)
                                                    .child(resolution),
                                            )
                                        })
                                        .when(pending, |this| {
                                            this.children(buttons.iter().enumerate().map(
                                                |(choice_index, (label, allow))| {
                                                    let primary = *allow;
                                                    div()
                                                        .id((
                                                            "approval-choice",
                                                            message_index * 512
                                                                + part_index * 16
                                                                + choice_index,
                                                        ))
                                                        .h(px(metrics::MENU_ROW))
                                                        .px(px(metrics::SPACE_3))
                                                        .rounded_full()
                                                        .when(primary, |this| {
                                                            this.bg(theme.accent)
                                                                .text_color(theme.on_accent)
                                                        })
                                                        .when(!primary, |this| {
                                                            this.border_1()
                                                                .border_color(theme.hairline)
                                                                .text_color(theme.ink_secondary)
                                                        })
                                                        .cursor_pointer()
                                                        .flex()
                                                        .items_center()
                                                        .text_size(px(typo::META))
                                                        .on_click(cx.listener(
                                                            move |this, _event, _window, cx| {
                                                                this.answer_approval(
                                                                    chat_index,
                                                                    message_index,
                                                                    part_index,
                                                                    choice_index,
                                                                    cx,
                                                                )
                                                            },
                                                        ))
                                                        .child(label.clone())
                                                },
                                            ))
                                        }),
                                )
                                .into_any_element()
                        }
                        Part::Question {
                            request,
                            index,
                            header,
                            question,
                            options,
                            multi_select,
                            allow_free_text,
                            secret: _,
                            selected,
                            resolved,
                        } => {
                            let pending = resolved.is_none();
                            let resolution = resolved.clone();
                            let options = options.clone();
                            let selected = selected.clone();
                            let request = *request;
                            let question_index = *index;
                            let multi = *multi_select;
                            let free_text = *allow_free_text;
                            div()
                                .id(("question-card", message_index * 32 + part_index))
                                .w(px(metrics::CARD_WIDTH))
                                .max_w(px(metrics::BUBBLE_MAX_WIDTH))
                                .p(px(metrics::CARD_INSET))
                                .rounded(px(metrics::RADIUS_BUBBLE))
                                .bg(theme.card)
                                .border_1()
                                .border_color(theme.hairline)
                                .flex()
                                .flex_col()
                                .gap(px(metrics::SPACE_1))
                                .child(
                                    div()
                                        .text_size(px(typo::TINY))
                                        .text_color(theme.ink_tertiary)
                                        .child(header.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(typo::BODY))
                                        .text_color(theme.ink)
                                        .child(question.clone()),
                                )
                                .child(
                                    div()
                                        .mt(px(metrics::SPACE_1))
                                        .flex()
                                        .flex_wrap()
                                        .items_center()
                                        .gap(px(metrics::SPACE_2))
                                        .when_some(resolution, |this, resolution| {
                                            this.child(
                                                div()
                                                    .text_size(px(typo::META))
                                                    .text_color(theme.success)
                                                    .child(resolution),
                                            )
                                        })
                                        .when(pending, |this| {
                                            this.children(options.iter().enumerate().map(
                                                |(option_index, option)| {
                                                    let chosen =
                                                        selected.iter().any(|s| s == option);
                                                    div()
                                                        .id((
                                                            "question-option",
                                                            message_index * 512
                                                                + part_index * 16
                                                                + option_index,
                                                        ))
                                                        .h(px(metrics::MENU_ROW))
                                                        .px(px(metrics::SPACE_3))
                                                        .rounded_full()
                                                        .border_1()
                                                        .border_color(if chosen {
                                                            theme.ink
                                                        } else {
                                                            theme.hairline
                                                        })
                                                        .cursor_pointer()
                                                        .flex()
                                                        .items_center()
                                                        .text_size(px(typo::META))
                                                        .text_color(theme.ink_secondary)
                                                        .on_click(cx.listener(
                                                            move |this, _event, _window, cx| {
                                                                this.answer_question_choice(
                                                                    chat_index,
                                                                    message_index,
                                                                    part_index,
                                                                    option_index,
                                                                    cx,
                                                                )
                                                            },
                                                        ))
                                                        .child(option.clone())
                                                },
                                            ))
                                            .when(multi, |this| {
                                                this.child(
                                                    div()
                                                        .id((
                                                            "question-send",
                                                            message_index * 32 + part_index,
                                                        ))
                                                        .h(px(metrics::MENU_ROW))
                                                        .px(px(metrics::SPACE_3))
                                                        .rounded_full()
                                                        .bg(theme.accent)
                                                        .cursor_pointer()
                                                        .flex()
                                                        .items_center()
                                                        .text_size(px(typo::META))
                                                        .text_color(theme.on_accent)
                                                        .on_click(cx.listener(
                                                            move |this, _event, _window, cx| {
                                                                this.answer_question_multi(
                                                                    chat_index,
                                                                    message_index,
                                                                    part_index,
                                                                    cx,
                                                                )
                                                            },
                                                        ))
                                                        .child("Send"),
                                                )
                                            })
                                            .when(
                                                free_text,
                                                |this| {
                                                    this.child(
                                                        div()
                                                            .id((
                                                                "question-type",
                                                                message_index * 32 + part_index,
                                                            ))
                                                            .h(px(metrics::MENU_ROW))
                                                            .px(px(metrics::SPACE_3))
                                                            .rounded_full()
                                                            .border_1()
                                                            .border_color(theme.hairline)
                                                            .cursor_pointer()
                                                            .flex()
                                                            .items_center()
                                                            .text_size(px(typo::META))
                                                            .text_color(theme.ink_secondary)
                                                            .on_click(cx.listener(
                                                                move |this, _event, window, cx| {
                                                                    this.free_text_question = Some(
                                                                        (request, question_index),
                                                                    );
                                                                    cx.notify();
                                                                    window.focus(&this.focus, cx);
                                                                },
                                                            ))
                                                            .child("Type an answer"),
                                                    )
                                                },
                                            )
                                        }),
                                )
                                .into_any_element()
                        }
                    });
                }

                if message.streaming {
                    body.push(
                        div()
                            .mt(px(metrics::SPACE_HALF))
                            .w(px(7.0))
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
                            .max_w(px(metrics::BUBBLE_MAX_WIDTH))
                            .when(!has_approval, |this| {
                                this.px(px(metrics::SPACE_3))
                                    .py(px(metrics::SPACE_2))
                                    .rounded(px(metrics::RADIUS_BUBBLE))
                                    .bg(theme.card)
                            })
                            .flex()
                            .flex_col()
                            .gap(px(metrics::SPACE_2))
                            .children(body),
                    )
                    .into_any_element()
            }
        }
    }

    fn undo_button(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        chat_index: usize,
        message_index: usize,
        part_index: usize,
    ) -> AnyElement {
        let hover = theme.hover_wash();
        let color = theme.ink_secondary;
        div()
            .id(("undo", message_index * 32 + part_index))
            .flex_shrink_0()
            .h(px(metrics::CONTROL_XS))
            .px(px(metrics::SPACE_2))
            .flex()
            .items_center()
            .rounded_full()
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .text_size(px(typo::TINY))
            .text_color(color)
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.undo_part(chat_index, message_index, part_index, cx)
            }))
            .child("Undo")
            .into_any_element()
    }

    /// Answer a permission card: the clicked button says what it means.
    pub(crate) fn answer_approval(
        &mut self,
        chat_index: usize,
        message_index: usize,
        part_index: usize,
        choice_index: usize,
        cx: &mut Context<Self>,
    ) {
        let answered = self
            .workspace
            .chats
            .get_mut(chat_index)
            .and_then(|chat| chat.messages.get_mut(message_index))
            .and_then(|message| message.parts.get_mut(part_index))
            .and_then(|part| match part {
                Part::Approval {
                    request,
                    choices,
                    resolved,
                    ..
                } => {
                    let (label, allow) = choices.get(choice_index)?.clone();
                    let request = request.take()?;
                    *resolved = Some(label.clone());
                    Some((request, allow))
                }
                _ => None,
            });
        let Some((request, allow)) = answered else {
            return;
        };
        self.engine.answer_permission(request, allow);
        self.resume_after_answer();
        cx.notify();
    }

    /// Pick an option on a question card. Multi-select toggles instead.
    pub(crate) fn answer_question_choice(
        &mut self,
        chat_index: usize,
        message_index: usize,
        part_index: usize,
        option_index: usize,
        cx: &mut Context<Self>,
    ) {
        self.free_text_question = None;
        let picked = self
            .workspace
            .chats
            .get_mut(chat_index)
            .and_then(|chat| chat.messages.get_mut(message_index))
            .and_then(|message| message.parts.get_mut(part_index))
            .and_then(|part| match part {
                Part::Question {
                    request,
                    index,
                    options,
                    multi_select,
                    selected,
                    ..
                } => {
                    let option = options.get(option_index)?.clone();
                    if *multi_select {
                        if let Some(position) = selected.iter().position(|s| *s == option) {
                            selected.remove(position);
                        } else {
                            selected.push(option);
                        }
                        return Some(None);
                    }
                    Some(Some((*request, *index, option)))
                }
                _ => None,
            });
        match picked {
            Some(None) => cx.notify(),
            Some(Some((request, index, label))) => {
                self.engine
                    .answer_question(request, index, vec![label.clone()], None);
                self.mark_question(request, index, label);
                cx.notify();
            }
            None => {}
        }
    }

    /// Send a multi-select answer: everything toggled on, in click order.
    pub(crate) fn answer_question_multi(
        &mut self,
        chat_index: usize,
        message_index: usize,
        part_index: usize,
        cx: &mut Context<Self>,
    ) {
        let picked = self
            .workspace
            .chats
            .get_mut(chat_index)
            .and_then(|chat| chat.messages.get_mut(message_index))
            .and_then(|message| message.parts.get_mut(part_index))
            .and_then(|part| match part {
                Part::Question {
                    request,
                    index,
                    selected,
                    ..
                } => Some((*request, *index, selected.clone())),
                _ => None,
            });
        let Some((request, index, labels)) = picked else {
            return;
        };
        let label = if labels.is_empty() {
            "Skipped".to_string()
        } else {
            labels.join(", ")
        };
        self.engine.answer_question(request, index, labels, None);
        self.mark_question(request, index, label);
        cx.notify();
    }

    /// Mark every card of that question as answered, wherever it lives.
    pub(crate) fn mark_question(&mut self, request: u64, index: usize, label: String) {
        for chat in &mut self.workspace.chats {
            for message in &mut chat.messages {
                for part in &mut message.parts {
                    if let Part::Question {
                        request: part_request,
                        index: part_index,
                        secret,
                        resolved,
                        ..
                    } = part
                        && *part_request == request
                        && *part_index == index
                    {
                        // A secret answer never lands in the transcript.
                        *resolved = Some(if *secret {
                            "Answered privately".to_string()
                        } else {
                            label.clone()
                        });
                    }
                }
            }
        }
        self.resume_after_answer();
    }

    /// The agent can run again once it has its answer.
    fn resume_after_answer(&mut self) {
        if let Some(bot) = self.run_bot.and_then(|id| self.workspace.bot_mut(id)) {
            bot.presence = crate::domain::model::Presence::Working;
            bot.activity = "Resumed".into();
        }
        if self.status.is_some() {
            self.status = Some("Resumed".into());
        }
        self.workspace.replace_last_preview();
    }

    /// A run that ended leaves no clickable card behind: the harness is gone,
    /// so an answer would go nowhere.
    pub(crate) fn close_cards(&mut self) {
        for chat in &mut self.workspace.chats {
            for message in &mut chat.messages {
                for part in &mut message.parts {
                    match part {
                        Part::Approval {
                            request, resolved, ..
                        } if resolved.is_none() => {
                            request.take();
                            *resolved = Some("Run ended".to_string());
                        }
                        Part::Question { resolved, .. } if resolved.is_none() => {
                            *resolved = Some("Run ended".to_string());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn status_strip(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
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
                                .on_click(
                                    cx.listener(|this, _event, _window, cx| this.stop_run(cx)),
                                )
                                .child(widgets::icon("stop", 9.0, stop_color))
                                .child("Stop"),
                        )
                    })
                    .into_any_element(),
            )
            .into_any_element()
    }
}
