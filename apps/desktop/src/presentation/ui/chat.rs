//! The chat: header, transcript and composer.
//!
//! A reply is one bubble-less column: the bot's words and the steps it took sit
//! in the same stream, because the work is the answer. Only the person's own
//! messages get a plate, so the eye always knows who is talking.

mod cards;
mod message;
mod table;

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, KeyDownEvent, ParentElement, Styled, Window, div,
    prelude::*, px,
};

use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::dialog::SettingsPage;
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmptyStateAction {
    CreateBot,
    OpenProviders,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EmptyStateContent {
    title: &'static str,
    body: &'static str,
    action_label: &'static str,
    action: EmptyStateAction,
}

fn empty_state_content(has_cli: bool, has_bots: bool) -> EmptyStateContent {
    match (has_cli, has_bots) {
        (false, _) => EmptyStateContent {
            title: "Connect an agent CLI",
            body: "Aucteris runs local agent CLIs. Open Providers to see what is available, install one, then return here to create your first bot.",
            action_label: "Choose a provider",
            action: EmptyStateAction::OpenProviders,
        },
        (true, false) => EmptyStateContent {
            title: "Create your first bot",
            body: "1. Give it a name. 2. Choose an installed CLI. 3. Send it a task in your working folder.",
            action_label: "Create your first bot",
            action: EmptyStateAction::CreateBot,
        },
        (true, true) => EmptyStateContent {
            title: "Start a conversation",
            body: "Choose a bot in the sidebar, or create another bot to start a new conversation.",
            action_label: "Create a bot",
            action: EmptyStateAction::CreateBot,
        },
    }
}

impl Shell {
    pub(crate) fn view_chat(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.workspace.chats.is_empty() {
            let has_cli = !self.available_runtimes.is_empty();
            let content = empty_state_content(has_cli, !self.workspace.bots.is_empty());
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(metrics::SPACE_3))
                .bg(theme.bg)
                .child(widgets::icon("bot", metrics::ICON_HERO, theme.ink_tertiary))
                .child(
                    div()
                        .text_size(px(typo::SHEET_TITLE))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.ink)
                        .child(content.title),
                )
                .child(
                    div()
                        .max_w(px(metrics::PROSE_WIDTH))
                        .text_center()
                        .text_size(px(typo::BODY))
                        .text_color(theme.ink_secondary)
                        .child(content.body),
                )
                .child(
                    div()
                        .id("empty-primary-action")
                        .mt(px(metrics::SPACE_2))
                        .px(px(metrics::SPACE_4))
                        .h(px(metrics::CONTROL_LG))
                        .flex()
                        .items_center()
                        .rounded_full()
                        .bg(theme.accent)
                        .text_color(theme.on_accent)
                        .cursor_pointer()
                        .tab_index(0)
                        .role(gpui::Role::Button)
                        .aria_label(content.action_label)
                        .focus_visible(|style| style.border_1().border_color(theme.focus))
                        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                match content.action {
                                    EmptyStateAction::CreateBot => this.open_new_bot(window, cx),
                                    EmptyStateAction::OpenProviders => {
                                        this.open_settings_page(SettingsPage::Providers, window, cx)
                                    }
                                }
                            }
                        }))
                        .on_click(cx.listener(
                            move |this, _event, window, cx| match content.action {
                                EmptyStateAction::CreateBot => this.open_new_bot(window, cx),
                                EmptyStateAction::OpenProviders => {
                                    this.open_settings_page(SettingsPage::Providers, window, cx)
                                }
                            },
                        ))
                        .child(content.action_label),
                )
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
            .child(
                div()
                    .id("toggle-sidebar")
                    .size(px(metrics::CONTROL_MD))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(metrics::RADIUS_CONTROL))
                    .when(self.sidebar_collapsed, |this| this.bg(theme.raised))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.raised))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.sidebar_collapsed = !this.sidebar_collapsed;
                        cx.notify();
                    }))
                    .child(widgets::icon("sidebar", 16.0, theme.ink_secondary)),
            )
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
                                metrics::MASCOT_HEADER,
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
                                        metrics::MASCOT_STACK,
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
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.dispatch(crate::presentation::ui::action::UiAction::TogglePanel, cx)
                    }))
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

#[cfg(test)]
mod tests {
    use super::{EmptyStateAction, empty_state_content};

    #[test]
    fn no_detected_cli_guides_the_person_to_provider_setup() {
        let content = empty_state_content(false, false);

        assert_eq!(content.action, EmptyStateAction::OpenProviders);
        assert_eq!(content.action_label, "Choose a provider");
        assert!(content.body.contains("install one"));
    }

    #[test]
    fn installed_cli_guides_a_first_time_person_to_create_a_bot() {
        let content = empty_state_content(true, false);

        assert_eq!(content.action, EmptyStateAction::CreateBot);
        assert_eq!(content.action_label, "Create your first bot");
        assert!(content.body.contains("working folder"));
    }

    #[test]
    fn existing_bots_get_a_conversation_empty_state() {
        let content = empty_state_content(true, true);

        assert_eq!(content.title, "Start a conversation");
        assert_eq!(content.action, EmptyStateAction::CreateBot);
    }
}
