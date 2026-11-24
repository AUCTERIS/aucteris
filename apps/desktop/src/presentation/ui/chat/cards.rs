//! Answering approval and question cards, rendering them in the transcript,
//! and closing them when a run ends.

use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, prelude::*, px};

use crate::domain::model::Part;
use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use aucteris_harness::komet_proto::PermissionChoice;

impl Shell {
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
                    grant_folder_choice,
                    resolved,
                    ..
                } => {
                    let (label, choice) = choices.get(choice_index)?.clone();
                    let request = request.take()?;
                    *resolved = Some(label.clone());
                    Some((
                        request,
                        choice,
                        (*grant_folder_choice == Some(choice_index)),
                    ))
                }
                _ => None,
            });
        let Some((request, choice, grant_folder)) = answered else {
            return;
        };
        let accepted = self.engine.answer_permission(request, choice.clone());
        if accepted && grant_folder && choice != PermissionChoice::Deny {
            let bot_id = self
                .workspace
                .chats
                .get(chat_index)
                .and_then(|chat| chat.bot.or_else(|| chat.members.first().copied()));
            let folder = self
                .workspace
                .chats
                .get(chat_index)
                .and_then(|chat| chat.messages.get(message_index))
                .and_then(|message| message.parts.get(part_index))
                .and_then(|part| match part {
                    Part::Approval { detail, .. } => detail
                        .lines()
                        .find_map(|line| line.strip_prefix("Approve this folder for future runs: "))
                        .map(str::to_string),
                    _ => None,
                });
            if let (Some(bot_id), Some(folder)) = (bot_id, folder)
                && let Some(bot) = self.workspace.bot_mut(bot_id)
                && !bot.allowed_folders.contains(&folder)
            {
                bot.allowed_folders.push(folder);
            }
        }
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

    /// Render an approval or question card attached to a streamed reply.
    pub(super) fn card_view(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        chat_index: usize,
        message_index: usize,
        part_index: usize,
        part: &Part,
    ) -> AnyElement {
        match part {
            Part::Approval {
                kicker,
                title,
                detail,
                request,
                choices,
                resolved,
                ..
            } => {
                let pending = resolved.is_none() && request.is_some();
                let resolution = resolved.clone();
                let buttons = choices.clone();
                // One primary action only: the first way to allow. Other allow
                // options are secondary, and Deny never looks like the default.
                let primary_choice = buttons
                    .iter()
                    .position(|(_, choice)| !matches!(choice, PermissionChoice::Deny));
                let hover = theme.hover_wash();
                let danger_wash = theme.danger.opacity(0.10);
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
                            .text_color(theme.ink_secondary)
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
                                    |(choice_index, (label, choice))| {
                                        let deny = matches!(choice, PermissionChoice::Deny);
                                        let primary = !deny
                                            && primary_choice == Some(choice_index);
                                        let focus = theme.focus;
                                        div()
                                            .id((
                                                "approval-choice",
                                                message_index * 512
                                                    + part_index * 16
                                                    + choice_index,
                                            ))
                                            .role(gpui::Role::Button)
                                            .aria_label(format!("Permission choice: {label}"))
                                            .tab_index(0)
                                            .focus_visible(move |style| {
                                                style.border_1().border_color(focus)
                                            })
                                            .h(px(metrics::MENU_ROW))
                                            .px(px(metrics::SPACE_3))
                                            .rounded_full()
                                            .when(primary, |this| {
                                                this.bg(theme.accent).text_color(theme.on_accent)
                                            })
                                            .when(!primary && !deny, |this| {
                                                this.border_1()
                                                    .border_color(theme.hairline)
                                                    .text_color(theme.ink_secondary)
                                                    .hover(move |style| style.bg(hover))
                                            })
                                            .when(deny, |this| {
                                                this.text_color(theme.danger)
                                                    .hover(move |style| style.bg(danger_wash))
                                            })
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .text_size(px(typo::META))
                                            .on_click(cx.listener(
                                                move |this, _event, _window, cx| {
                                                    this.dispatch(crate::presentation::ui::action::UiAction::AnswerApproval { chat: chat_index, message: message_index, part: part_index, choice: choice_index }, cx)
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
                selected,
                resolved,
                ..
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
                                        let chosen = selected.iter().any(|s| s == option);
                                        let focus = theme.focus;
                                        div()
                                            .id((
                                                "question-option",
                                                message_index * 512
                                                    + part_index * 16
                                                    + option_index,
                                            ))
                                            .role(if *multi_select {
                                                gpui::Role::CheckBox
                                            } else {
                                                gpui::Role::Button
                                            })
                                            .aria_label(option.clone())
                                            .aria_selected(chosen)
                                            .tab_index(0)
                                            .focus_visible(move |style| {
                                                style.border_1().border_color(focus)
                                            })
                                            .h(px(metrics::MENU_ROW))
                                            .px(px(metrics::SPACE_3))
                                            .rounded_full()
                                            .border_1()
                                            .border_color(if chosen {
                                                theme.ink
                                            } else {
                                                theme.hairline
                                            })
                                            .when(chosen, |this| {
                                                this.bg(theme.ink.opacity(0.06))
                                            })
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .text_size(px(typo::META))
                                            .text_color(theme.ink_secondary)
                                            .on_click(cx.listener(
                                                move |this, _event, _window, cx| {
                                                    this.dispatch(crate::presentation::ui::action::UiAction::AnswerQuestionChoice { chat: chat_index, message: message_index, part: part_index, option: option_index }, cx)
                                                },
                                            ))
                                            .child(option.clone())
                                    },
                                ))
                                .when(multi, |this| {
                                    this.child(
                                        div()
                                            .id(("question-send", message_index * 32 + part_index))
                                            .role(gpui::Role::Button)
                                            .aria_label("Send selected answers")
                                            .tab_index(0)
                                            .focus_visible({
                                                let focus = theme.focus;
                                                move |style| style.border_1().border_color(focus)
                                            })
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
                                                    this.dispatch(crate::presentation::ui::action::UiAction::AnswerQuestionMulti { chat: chat_index, message: message_index, part: part_index }, cx)
                                                },
                                            ))
                                            .child("Send"),
                                    )
                                })
                                .when(free_text, |this| {
                                    this.child(
                                        div()
                                            .id(("question-type", message_index * 32 + part_index))
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
                                                    this.free_text_question =
                                                        Some((request, question_index));
                                                    cx.notify();
                                                    window.focus(&this.focus, cx);
                                                },
                                            ))
                                            .child("Type an answer"),
                                    )
                                })
                            }),
                    )
                    .into_any_element()
            }
            _ => div().into_any_element(),
        }
    }
}
