use crate::domain::model::{Message, Part};
use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets;
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, img, prelude::*, px};

impl Shell {
    pub(super) fn user_message_view(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        message_index: usize,
        message: &Message,
    ) -> AnyElement {
        let attachments: Vec<AnyElement> = message
            .parts
            .iter()
            .enumerate()
            .filter_map(|(attachment_index, part)| {
                let Part::Attachment { path } = part else {
                    return None;
                };
                let path_for_open = path.clone();
                let name = std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(path)
                    .to_string();
                let extension = std::path::Path::new(path)
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let is_image = matches!(
                    extension.as_str(),
                    "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"
                );
                let file_body = if is_image {
                    img(std::path::PathBuf::from(path.clone()))
                        .max_w(px(320.0))
                        .max_h(px(220.0))
                        .rounded(px(metrics::RADIUS_CONTROL))
                        .into_any_element()
                } else {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(metrics::SPACE_2))
                        .child(widgets::icon("file", metrics::ICON_MD, theme.ink_secondary))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(px(typo::META))
                                        .text_color(theme.ink)
                                        .child(name.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(typo::TINY))
                                        .text_color(theme.ink_secondary)
                                        .child(if extension == "pdf" {
                                            "PDF document"
                                        } else {
                                            "Attached file"
                                        }),
                                ),
                        )
                        .into_any_element()
                };
                Some(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(metrics::SPACE_2))
                        .p(px(metrics::SPACE_2))
                        .rounded(px(metrics::RADIUS_CONTROL))
                        .bg(theme.card)
                        .border_1()
                        .border_color(theme.hairline)
                        .child(file_body)
                        .child(
                            div()
                                .id((
                                    "open-source-attachment",
                                    message_index * 128 + attachment_index,
                                ))
                                .ml_auto()
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
                                    cx.open_with_system(std::path::Path::new(&path_for_open))
                                }))
                                .child("Open original"),
                        )
                        .into_any_element(),
                )
            })
            .collect();
        div()
            .w_full()
            .flex()
            .justify_end()
            .child(
                div()
                    .max_w(px(metrics::BUBBLE_MAX_WIDTH))
                    .min_w_0()
                    .px(px(metrics::SPACE_3))
                    .py(px(metrics::SPACE_2))
                    .rounded(px(metrics::RADIUS_BUBBLE))
                    .bg(theme.user_plate)
                    .text_size(px(typo::BUBBLE))
                    .line_height(px(typo::BUBBLE_LINE))
                    .text_color(theme.on_user_plate)
                    .when(!message.text().is_empty(), |this| {
                        this.child(message.text())
                    })
                    .children(attachments),
            )
            .into_any_element()
    }

    pub(super) fn system_message_view(&self, theme: &Theme, message: &Message) -> AnyElement {
        div()
            .w_full()
            .flex()
            .justify_center()
            .child(
                div()
                    .text_size(px(typo::META))
                    .text_color(theme.ink_secondary)
                    .child(message.text()),
            )
            .into_any_element()
    }
}
