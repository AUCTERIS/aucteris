//! The composer: a floating pill with the prompt, attach and send controls —
//! the same chrome Komet uses (`rounded-[26px]` hairline over a faint wash,
//! controls inside the pill, Send morphing into Stop while a run is live).

use gpui::{
    AnyElement, Context, HighlightStyle, IntoElement, ParentElement, Styled, StyledText, Window,
    div, prelude::*, px,
};

use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::dialog::caret_spans;
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
        let running = self.running();

        let placeholder = format!("Message {name}…");

        // The caret lives in the text flow, same recipe as the dialog fields:
        // after the last character — the last line for multiline drafts —
        // leading the placeholder when empty, hidden on the blink's off
        // phase and whenever the composer is not focused.
        let (display, caret_ranges) =
            caret_spans(&self.draft, &placeholder, focused, self.caret_visible);
        let content = StyledText::new(display).with_highlights(
            caret_ranges
                .into_iter()
                .map(|range| (range, HighlightStyle::color(theme.ink_secondary))),
        );

        let input = div()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(typo::BUBBLE_LINE))
            .flex()
            .items_end()
            .overflow_hidden()
            .font_family(theme.font_sans.clone())
            .text_size(px(typo::BUBBLE))
            .line_height(px(typo::BUBBLE_LINE))
            .text_color(if empty { theme.ink_tertiary } else { theme.ink })
            .child(content);

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
                .on_click(cx.listener(|this, _event, _window, cx| this.stop_run(cx)))
                .child(widgets::icon("stop", 12.0, theme.bg))
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
                .child(widgets::icon("send", 14.0, theme.bg))
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
            .on_click(cx.listener(|this, _event, _window, cx| {
                if !this.draft.is_empty() && !this.draft.ends_with(' ') {
                    this.draft.push(' ');
                }
                this.draft.push_str("[attached: file] ");
                cx.notify();
            }))
            .child(widgets::icon("plus", 16.0, theme.ink_secondary));

        div()
            .flex_shrink_0()
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
                            .h(px(48.0))
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .pl(px(16.0))
                            .pr(px(8.0))
                            .rounded(px(metrics::RADIUS_COMPOSER))
                            .bg(theme.composer)
                            .border_1()
                            .border_color(theme.hairline)
                            .shadow_lg()
                            .track_focus(&self.focus)
                            .on_key_down(cx.listener(Self::on_composer_key))
                            .on_click(cx.listener(|this, _event, window, cx| {
                                window.focus(&this.focus.clone(), cx);
                                cx.notify();
                            }))
                            .child(attach)
                            .child(input)
                            .child(send),
                    ),
            )
            .into_any_element()
    }
}
