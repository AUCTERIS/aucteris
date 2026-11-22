//! Rendering one bot message: author plate, parts stream, undo buttons.

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement, Styled, div, prelude::*, px,
};

use crate::domain::model::{Author, Message, Part};
use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets;

impl Shell {
    pub(super) fn message_view(
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
                        Part::Approval { .. } => {
                            self.card_view(theme, cx, chat_index, message_index, part_index, part)
                        }
                        Part::Question { .. } => {
                            self.card_view(theme, cx, chat_index, message_index, part_index, part)
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
}
