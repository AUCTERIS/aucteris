//! The composer: a floating pill with the prompt, attach and send controls —
//! the same chrome Komet uses (`rounded-[26px]` hairline over a faint wash,
//! controls inside the pill, Send morphing into Stop while a run is live).

use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, Window, div, prelude::*, px};

use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets;

impl Shell {
    pub(crate) fn view_composer(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focus.is_focused(window);
        let name = self.chat_name();
        let empty = self.draft.is_empty();
        let running = self.running_here();

        let placeholder = if name.is_empty() {
            "Ask anything".to_string()
        } else {
            format!("Message {name}…")
        };
        let shown = if empty {
            placeholder
        } else {
            self.draft.clone()
        };

        // Keep the editable copy in its own text node. The blinking caret is
        // a separate painted element, so changing its visibility cannot make
        // GPUI rebuild or flicker the draft glyphs themselves.
        let input = div()
            .w_full()
            .min_w(px(metrics::ZERO))
            .min_h(px(metrics::PX_32))
            .flex()
            .items_start()
            .overflow_hidden()
            .font_family(theme.font_sans.clone())
            .text_size(px(typo::BUBBLE))
            .line_height(px(typo::BUBBLE_LINE))
            .text_color(if empty { theme.ink_tertiary } else { theme.ink })
            .child(div().min_w(px(metrics::ZERO)).child(shown))
            .when(focused && self.caret_visible, |this| {
                this.child(
                    div()
                        .w(px(metrics::PX_1))
                        .h(px(metrics::PX_18))
                        .flex_shrink_0()
                        .bg(theme.ink_secondary),
                )
            });

        // Komet's send button morphs with the run: a live run with an empty
        // draft turns it into Stop; with text it stays Send — steering the
        // run when the engine allows, dimmed and inert otherwise (there is
        // no queue). Both modes are the same ink circle with a `bg` glyph.
        let send = if running && empty {
            div()
                .id("composer-stop")
                .size(px(metrics::CONTROL_MD))
                .flex_shrink_0()
                .rounded_full()
                .bg(theme.ink)
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|style| style.opacity(0.85))
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.dispatch(crate::presentation::ui::action::UiAction::StopRun, cx)
                }))
                .child(widgets::icon("stop", metrics::ICON_SM, theme.bg))
                .into_any_element()
        } else {
            let enabled = !empty && (!running || self.can_steer());
            div()
                .id("composer-send")
                .size(px(metrics::CONTROL_MD))
                .flex_shrink_0()
                .rounded_full()
                .bg(theme.ink)
                .flex()
                .items_center()
                .justify_center()
                .when(enabled, |this| {
                    this.cursor_pointer().hover(|style| style.opacity(0.85))
                })
                .when(!enabled, |this| this.opacity(0.35))
                .on_click(cx.listener(|this, _event, window, cx| {
                    if !this.draft.trim().is_empty() {
                        this.submit(window, cx);
                        window.focus(&this.focus.clone(), cx);
                    }
                }))
                .child(widgets::icon("send", metrics::ICON_MD - 2.0, theme.bg))
                .into_any_element()
        };

        let attach_hover = theme.hover_wash();
        let attach = div()
            .id("attach")
            .size(px(metrics::CONTROL_MD))
            .flex_shrink_0()
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(move |style| style.bg(attach_hover))
            .on_click(cx.listener(|this, _event, window, cx| {
                this.pick_attachments(window, cx);
            }))
            .child(widgets::icon("plus", metrics::ICON_MD, theme.ink_secondary));

        let attachment_chips: Vec<AnyElement> = self
            .draft_attachments
            .get(&self.workspace.chat().id)
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, path)| {
                let path = path.clone();
                let name = std::path::Path::new(&path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or(&path)
                    .to_string();
                div()
                    .id(("draft-attachment", index))
                    .px(px(metrics::SPACE_2))
                    .h(px(metrics::CONTROL_SM))
                    .flex()
                    .items_center()
                    .gap(px(metrics::SPACE_1))
                    .rounded(px(metrics::RADIUS_CONTROL))
                    .bg(theme.raised)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.hover_wash()))
                    .on_click(cx.listener(move |_this, _event, _window, cx| {
                        cx.open_with_system(std::path::Path::new(&path))
                    }))
                    .child(widgets::icon("file", metrics::ICON_SM, theme.ink_tertiary))
                    .child(
                        div()
                            .max_w(px(metrics::PX_160))
                            .truncate()
                            .text_size(px(typo::TINY))
                            .text_color(theme.ink_secondary)
                            .child(name),
                    )
                    .into_any_element()
            })
            .collect();

        div()
            .flex_shrink_0()
            .when(!attachment_chips.is_empty(), |this| {
                this.child(
                    div()
                        .w_full()
                        .max_w(px(metrics::CHAT_COLUMN_WIDTH))
                        .mx_auto()
                        .px(px(metrics::SPACE_4))
                        .pt(px(metrics::SPACE_1))
                        .flex()
                        .gap(px(metrics::SPACE_1))
                        .children(attachment_chips),
                )
            })
            .child(
                div()
                    .w_full()
                    .max_w(px(metrics::CHAT_COLUMN_WIDTH))
                    .mx_auto()
                    .px(px(metrics::SPACE_4))
                    .pb(px(metrics::SPACE_2))
                    .pt(px(metrics::SPACE_1))
                    .child(
                        div()
                            .id("composer")
                            .w_full()
                            .h(px(metrics::COMPOSER_HEIGHT))
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .justify_between()
                            .px(px(metrics::COMPOSER_INSET_START))
                            .pr(px(metrics::COMPOSER_INSET_END))
                            .py(px(metrics::SPACE_2))
                            .rounded(px(metrics::RADIUS_COMPOSER))
                            .bg(theme.composer)
                            .border_1()
                            .border_color(if focused { theme.edge } else { theme.hairline })
                            .shadow_md()
                            .track_focus(&self.focus)
                            .on_key_down(cx.listener(Self::on_composer_key))
                            .on_click(cx.listener(|this, _event, window, cx| {
                                window.focus(&this.focus.clone(), cx);
                                cx.notify();
                            }))
                            .child(input)
                            .child(
                                div()
                                    .w_full()
                                    .h(px(metrics::CONTROL_MD))
                                    .flex()
                                    .items_center()
                                    .gap(px(metrics::COMPOSER_GAP))
                                    .child(attach)
                                    .child(div().flex_1())
                                    .child(send),
                            ),
                    ),
            )
            .into_any_element()
    }
}
