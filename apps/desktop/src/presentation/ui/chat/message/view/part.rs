use super::super::code::{file_diff_view, public_tool_detail, public_tool_label, pulsing_icon};
use super::super::markdown::markdown_view;
use crate::domain::model::Part;
use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets;
use gpui::{
    AnyElement, ClipboardItem, Context, FontWeight, IntoElement, ParentElement, Styled, div,
    prelude::*, px,
};

impl Shell {
    pub(super) fn part_view(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        location: (usize, usize, usize),
        part: &Part,
        has_approval: bool,
    ) -> AnyElement {
        let (chat_index, message_index, part_index) = location;
        match part {
            Part::Text(text) => div()
                .w_full()
                .min_w_0()
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
                .child(markdown_view(
                    text,
                    theme,
                    chat_index
                        .wrapping_mul(1_000_000)
                        .wrapping_add(message_index.wrapping_mul(1_000)),
                    &self.expanded_code_blocks,
                    cx,
                ))
                .into_any_element(),
            Part::Thought { text, collapsed } => {
                let chat_for_click = chat_index;
                let message_for_click = message_index;
                let part_for_click = part_index;
                div()
                    .flex()
                    .flex_col()
                    .gap(px(metrics::SPACE_2))
                    .py(px(metrics::SPACE_1))
                    .child(
                        div()
                            .id(("thought-toggle", message_index * 64 + part_index))
                            .h(px(metrics::CONTROL_SM))
                            .flex()
                            .items_center()
                            .gap(px(metrics::SPACE_2))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.dispatch(
                                    crate::presentation::ui::action::UiAction::ToggleThought {
                                        chat: chat_for_click,
                                        message: message_for_click,
                                        part: part_for_click,
                                    },
                                    cx,
                                );
                            }))
                            .child(
                                div()
                                    .text_size(px(typo::META))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.ink_secondary)
                                    .child("Reasoning"),
                            )
                            .child(widgets::icon(
                                if *collapsed {
                                    "chevron-right"
                                } else {
                                    "chevron-down"
                                },
                                9.0,
                                theme.ink_tertiary,
                            )),
                    )
                    .when(!*collapsed, |this| {
                        this.child(
                            div()
                                .pl(px(metrics::PX_23))
                                .text_size(px(typo::BODY))
                                .line_height(px(typo::BUBBLE_LINE))
                                .text_color(theme.ink_secondary)
                                .child(text.clone()),
                        )
                    })
                    .into_any_element()
            }
            Part::Tool {
                icon,
                label,
                detail,
                done,
                failed,
                result,
                duration_ms,
                started_at,
                ..
            } => {
                let display_label = public_tool_label(label);
                let display_detail = public_tool_detail(label, detail.as_deref());
                let elapsed_ms = duration_ms
                    .or_else(|| started_at.map(|start| start.elapsed().as_millis() as u64));
                let elapsed = elapsed_ms.map(|ms| format!("{:.1}s", ms as f64 / 1000.0));
                let state_color = if *failed {
                    theme.danger
                } else {
                    theme.ink_tertiary
                };
                let motion_id = chat_index * 1_000_000 + message_index * 1_000 + part_index;
                let output_id = 9_000_000_000usize
                    .wrapping_add(chat_index * 1_000_000)
                    .wrapping_add(message_index * 1_000)
                    .wrapping_add(part_index);
                let output_expanded = self.expanded_code_blocks.contains(&output_id);
                let tool_mark = widgets::icon(icon, metrics::ICON_SM, state_color);
                let tool_mark = if !*done {
                    pulsing_icon(tool_mark, ("tool-pulse", motion_id))
                } else {
                    tool_mark
                };
                let row = div()
                    .id(("tool-output", motion_id))
                    .w_full()
                    .min_w_0()
                    .h(px(metrics::CONTROL_MD))
                    .py(px(metrics::SPACE_1))
                    .flex()
                    .items_center()
                    .gap(px(metrics::SPACE_2))
                    .when(result.is_some(), |this| {
                        this.cursor_pointer().on_click(cx.listener(
                            move |this, _event, _window, cx| {
                                if !this.expanded_code_blocks.remove(&output_id) {
                                    this.expanded_code_blocks.insert(output_id);
                                }
                                cx.notify();
                            },
                        ))
                    })
                    .child(tool_mark)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(metrics::SPACE_2))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(typo::META))
                                    .text_color(if *done {
                                        theme.ink_secondary
                                    } else {
                                        theme.ink
                                    })
                                    .child(display_label),
                            )
                            .when_some(display_detail, |this, detail| {
                                this.child(
                                    div()
                                        .truncate()
                                        .text_size(px(typo::TINY))
                                        .text_color(theme.ink_tertiary)
                                        .child(format!("· {detail}")),
                                )
                            }),
                    )
                    .when_some(elapsed, |this, elapsed| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(typo::TINY))
                                .text_color(state_color)
                                .child(elapsed),
                        )
                    });
                let mut tool_view = div().w_full().min_w_0().flex().flex_col().child(row);
                if output_expanded && let Some(output) = result.as_ref() {
                    let output_for_copy = output.clone();
                    let copy_output = div()
                        .id(("copy-tool-output", motion_id))
                        .px(px(metrics::SPACE_2))
                        .h(px(metrics::CONTROL_XS))
                        .flex()
                        .items_center()
                        .rounded_full()
                        .cursor_pointer()
                        .hover(|style| style.bg(theme.hover_wash()))
                        .text_size(px(typo::TINY))
                        .text_color(theme.ink_secondary)
                        .on_click(cx.listener(move |_this, _event, _window, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                output_for_copy.clone(),
                            ));
                        }))
                        .child("Copy");
                    tool_view = tool_view.child(
                        div()
                            .w_full()
                            .ml(px(metrics::ICON_SM + metrics::SPACE_2))
                            .mb(px(metrics::SPACE_2))
                            .p(px(metrics::SPACE_2))
                            .rounded(px(metrics::RADIUS_CONTROL))
                            .bg(if *failed {
                                theme.danger.opacity(0.10)
                            } else {
                                theme.inset
                            })
                            .flex()
                            .flex_col()
                            .gap(px(metrics::SPACE_1))
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(typo::TINY))
                                            .text_color(if *failed {
                                                theme.danger
                                            } else {
                                                theme.ink_tertiary
                                            })
                                            .child(if *failed { "Error output" } else { "Output" }),
                                    )
                                    .child(copy_output),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .font_family(theme.font_mono.clone())
                                    .text_size(px(typo::TINY))
                                    .line_height(px(typo::BODY))
                                    .text_color(if *failed {
                                        theme.danger
                                    } else {
                                        theme.ink_secondary
                                    })
                                    .child(output.clone()),
                            ),
                    );
                }
                tool_view.into_any_element()
            }
            Part::Files {
                path,
                added,
                removed,
                old_text,
                new_text,
                diff_expanded,
                undo_error,
                undone,
            } => {
                let file_name = std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(path)
                    .to_string();
                let open_path = path.clone();
                let open = div()
                    .id(("open-changed-file", message_index * 32 + part_index))
                    .px(px(metrics::SPACE_2))
                    .h(px(metrics::CONTROL_XS))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.hover_wash()))
                    .text_size(px(typo::TINY))
                    .text_color(theme.ink_secondary)
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.dispatch(
                            crate::presentation::ui::action::UiAction::OpenFile {
                                chat: chat_index,
                                path: open_path.clone(),
                            },
                            cx,
                        );
                    }))
                    .child("Open");
                let toggle = div()
                    .id(("toggle-file-diff", message_index * 32 + part_index))
                    .px(px(metrics::SPACE_2))
                    .h(px(metrics::CONTROL_XS))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.hover_wash()))
                    .text_size(px(typo::TINY))
                    .text_color(theme.ink_secondary)
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.dispatch(
                            crate::presentation::ui::action::UiAction::ToggleFileDiff {
                                chat: chat_index,
                                message: message_index,
                                part: part_index,
                            },
                            cx,
                        );
                    }))
                    .child(if *diff_expanded {
                        "Hide diff"
                    } else {
                        "Preview"
                    });
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(metrics::SPACE_1))
                    .py(px(metrics::SPACE_1))
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(metrics::SPACE_2))
                            .child(widgets::icon("file", 13.0, theme.ink_tertiary))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(typo::META))
                                            .text_color(if *undone {
                                                theme.ink_tertiary
                                            } else {
                                                theme.ink
                                            })
                                            .child(file_name),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(typo::TINY))
                                            .text_color(theme.ink_tertiary)
                                            .child(path.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(typo::TINY))
                                    .text_color(theme.success)
                                    .child(format!("+{added}")),
                            )
                            .child(
                                div()
                                    .text_size(px(typo::TINY))
                                    .text_color(theme.danger)
                                    .child(format!("−{removed}")),
                            )
                            .when(!*undone, |this| {
                                this.child(toggle)
                                    .child(open)
                                    .when(new_text.is_some(), |this| {
                                        this.child(self.undo_button(
                                            theme,
                                            cx,
                                            chat_index,
                                            message_index,
                                            part_index,
                                        ))
                                    })
                            })
                            .when(*undone, |this| {
                                this.child(
                                    div()
                                        .text_size(px(typo::TINY))
                                        .text_color(theme.ink_tertiary)
                                        .child("Undone"),
                                )
                            }),
                    )
                    .when(*diff_expanded, |this| {
                        let preview = match (old_text.as_deref(), new_text.as_deref()) {
                            (Some(old), Some(new)) => file_diff_view(old, new, theme),
                            (None, Some(new)) => file_diff_view("", new, theme),
                            _ => div()
                                .text_size(px(typo::TINY))
                                .text_color(theme.ink_tertiary)
                                .child("Diff preview unavailable for this large file.")
                                .into_any_element(),
                        };
                        this.child(preview)
                    })
                    .when_some(undo_error.as_ref(), |this, error| {
                        this.child(
                            div()
                                .text_size(px(typo::TINY))
                                .text_color(theme.danger)
                                .child(error.clone()),
                        )
                    })
                    .into_any_element()
            }
            Part::Note { text, undone } => div()
                .flex()
                .items_center()
                .gap(px(metrics::SPACE_2))
                .h(px(metrics::CONTROL_MD))
                .py(px(metrics::SPACE_1))
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
                    this.child(self.undo_button(theme, cx, chat_index, message_index, part_index))
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
            Part::Attachment { .. } => div().into_any_element(),
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
                this.dispatch(
                    crate::presentation::ui::action::UiAction::UndoPart {
                        chat: chat_index,
                        message: message_index,
                        part: part_index,
                    },
                    cx,
                )
            }))
            .child("Undo")
            .into_any_element()
    }
}
