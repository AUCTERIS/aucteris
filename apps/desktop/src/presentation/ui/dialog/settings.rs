//! The settings dialog: General, Behavior, Shortcuts and About pages.

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement, Styled, div, prelude::*, px,
};

use crate::domain::model::Mode;
use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets;

use super::{Dialog, SettingsPage};

impl Shell {
    pub(super) fn settings_dialog(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        page: SettingsPage,
    ) -> AnyElement {
        let header = self.dialog_header(
            theme,
            cx,
            "Settings",
            Some("App-wide only. A single bot's settings live in its panel.".into()),
        );

        let mut rail: Vec<AnyElement> = Vec::new();
        for entry in SettingsPage::ALL {
            let selected = entry == page;
            let hover = theme.hover_wash();
            rail.push(
                div()
                    .id(("settings-tab", entry as usize))
                    .h(px(metrics::MENU_ROW))
                    .px(px(metrics::SPACE_2))
                    .mx(px(metrics::SPACE_1))
                    .flex()
                    .items_center()
                    .rounded(px(metrics::RADIUS_MENU_ROW))
                    .when(selected, |this| this.bg(theme.ink.opacity(0.09)))
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .text_size(px(typo::BODY))
                    .font_weight(if selected {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(if selected {
                        theme.ink
                    } else {
                        theme.ink_secondary
                    })
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.dialog = Some(Dialog::Settings(entry));
                        cx.notify();
                    }))
                    .child(entry.label())
                    .into_any_element(),
            );
        }

        let content = match page {
            SettingsPage::General => self.settings_general(theme, cx),
            SettingsPage::Behavior => self.settings_behavior(theme, cx),
            SettingsPage::Shortcuts => Self::settings_shortcuts(theme),
            SettingsPage::About => Self::settings_about(theme),
        };

        let body = div()
            .flex()
            .flex_1()
            .min_w(px(0.0))
            .overflow_hidden()
            .child(
                div()
                    .w(px(168.0))
                    .h_full()
                    .flex_shrink_0()
                    .pt(px(metrics::SPACE_2))
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .bg(theme.panel)
                    .children(rail),
            )
            .child(widgets::edge(theme.hairline))
            .child(
                div()
                    .id("settings-body")
                    .flex_1()
                    // A flex item in a row keeps `min-width: auto` until it is
                    // allowed to shrink, which is what let long copy escape the
                    // dialog instead of wrapping.
                    .min_w(px(0.0))
                    .h_full()
                    .overflow_y_scroll()
                    .px(px(metrics::SPACE_5))
                    .py(px(metrics::SPACE_4))
                    .flex()
                    .flex_col()
                    .gap(px(metrics::SPACE_5))
                    .child(content),
            )
            .into_any_element();

        let footer = div()
            .flex_shrink_0()
            .px(px(metrics::SPACE_5))
            .py(px(metrics::SPACE_3))
            .flex()
            .items_center()
            .gap(px(metrics::SPACE_2))
            .border_t_1()
            .border_color(theme.hairline)
            .child(
                div()
                    .flex_1()
                    .text_size(px(typo::TINY))
                    .text_color(theme.ink_tertiary)
                    .child("Changes apply immediately · Esc closes"),
            )
            .child(
                div()
                    .id("settings-done")
                    .h(px(26.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .bg(theme.accent)
                    .cursor_pointer()
                    .text_size(px(typo::TINY))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.on_accent)
                    .on_click(cx.listener(|this, _event, window, cx| this.close_dialog(window, cx)))
                    .child("Done")
                    .into_any_element(),
            )
            .into_any_element();

        self.dialog_shell(theme, cx, 680.0, header, body, footer)
    }

    fn settings_general(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let skin = div()
            .flex()
            .gap(px(metrics::SPACE_1))
            .child(widgets::segment(
                ("settings-skin", 0),
                "Midnight",
                self.appearance == crate::presentation::theme::Appearance::Dark,
                theme,
                cx.listener(|this, _event, _window, cx| {
                    if this.appearance != crate::presentation::theme::Appearance::Dark {
                        this.dispatch(
                            crate::presentation::ui::action::UiAction::ToggleAppearance,
                            cx,
                        );
                    }
                }),
            ))
            .child(widgets::segment(
                ("settings-skin", 1),
                "Daylight",
                self.appearance == crate::presentation::theme::Appearance::Light,
                theme,
                cx.listener(|this, _event, _window, cx| {
                    if this.appearance != crate::presentation::theme::Appearance::Light {
                        this.dispatch(
                            crate::presentation::ui::action::UiAction::ToggleAppearance,
                            cx,
                        );
                    }
                }),
            ))
            .into_any_element();

        let mode = div()
            .flex()
            .gap(px(metrics::SPACE_1))
            .child(widgets::segment(
                ("settings-mode", 0),
                Mode::Simple.label(),
                self.settings.mode == Mode::Simple,
                theme,
                cx.listener(|this, _event, _window, cx| {
                    this.dispatch(
                        crate::presentation::ui::action::UiAction::SetMode(Mode::Simple),
                        cx,
                    )
                }),
            ))
            .child(widgets::segment(
                ("settings-mode", 1),
                Mode::Advanced.label(),
                self.settings.mode == Mode::Advanced,
                theme,
                cx.listener(|this, _event, _window, cx| {
                    this.dispatch(
                        crate::presentation::ui::action::UiAction::SetMode(Mode::Advanced),
                        cx,
                    )
                }),
            ))
            .into_any_element();

        div()
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_5))
            .child(self.labelled(theme, "Skin", None, skin))
            .child(self.labelled(
                theme,
                "Interface",
                Some("Advanced adds detail; it never rearranges the app"),
                mode,
            ))
            .child(
                div()
                    .text_size(px(typo::META))
                    .line_height(px(typo::META_LINE + 2.0))
                    .text_color(theme.ink_tertiary)
                    .child("Simple keeps token counts and usage meters out of the way. Switching never loses anything: the same rows come back in Advanced."),
            )
            .into_any_element()
    }

    fn settings_behavior(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let hover = theme.hover_wash();
        let runtime_ids = self
            .engine
            .available_runtimes()
            .iter()
            .map(|cli| cli.id.to_string())
            .collect::<Vec<_>>();
        let runtime_label = self.engine.runtime_label(&self.settings.default_runtime);
        let runtime_row = div()
            .id("settings-default-runtime")
            .h(px(metrics::CONTROL_LG))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(metrics::RADIUS_CONTROL))
            .bg(theme.inset)
            .border_1()
            .border_color(theme.hairline)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |this, _event, _window, cx| {
                if !runtime_ids.is_empty() {
                    let current = runtime_ids
                        .iter()
                        .position(|id| id == &this.settings.default_runtime)
                        .unwrap_or(0);
                    this.settings.default_runtime =
                        runtime_ids[(current + 1) % runtime_ids.len()].clone();
                }
                cx.notify();
            }))
            .child(div().text_size(px(typo::BODY)).text_color(theme.ink).child(
                if runtime_label.is_empty() {
                    "No agent CLI found".to_string()
                } else {
                    runtime_label
                },
            ))
            .child(widgets::icon("chevron-down", 12.0, theme.ink_tertiary))
            .into_any_element();

        let send_row = div()
            .flex()
            .items_center()
            .gap(px(metrics::SPACE_3))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .text_size(px(typo::BODY))
                            .text_color(theme.ink)
                            .child("Send on Enter"),
                    )
                    .child(
                        div()
                            .text_size(px(typo::TINY))
                            .text_color(theme.ink_tertiary)
                            .child(if self.settings.send_on_enter {
                                "Off means Enter starts a new line and Ctrl+Enter sends"
                            } else {
                                "Enter starts a new line · Ctrl+Enter sends"
                            }),
                    ),
            )
            .child(widgets::switch(
                ("settings-send", 0),
                self.settings.send_on_enter,
                theme,
                cx.listener(|this, _event, _window, cx| {
                    this.settings.send_on_enter = !this.settings.send_on_enter;
                    cx.notify();
                }),
            ))
            .into_any_element();

        div()
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_5))
            .child(self.labelled(theme, "Default agent CLI for new bots", None, runtime_row))
            .child(self.labelled(theme, "Composer", None, send_row))
            .into_any_element()
    }

    fn settings_shortcuts(theme: &Theme) -> AnyElement {
        let rows: [(&str, [&str; 2]); 6] = [
            ("Send the message", ["Enter", ""]),
            ("New line", ["Shift", "Enter"]),
            ("New bot", ["Ctrl", "N"]),
            ("Search the sidebar", ["Ctrl", "F"]),
            ("Bot panel", ["Ctrl", "I"]),
            ("Switch skin", ["Ctrl", "J"]),
        ];
        let mut list: Vec<AnyElement> = Vec::new();
        for (action, keys) in rows {
            let mut caps: Vec<AnyElement> = Vec::new();
            for key in keys.iter().filter(|key| !key.is_empty()) {
                caps.push(widgets::keycap(*key, theme));
            }
            list.push(
                div()
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(typo::BODY))
                            .text_color(theme.ink_secondary)
                            .child(action),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(metrics::SPACE_1))
                            .children(caps),
                    )
                    .into_any_element(),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_5))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(widgets::section_label("Keyboard", theme))
                    .children(list),
            )
            .child(
                div()
                    .text_size(px(typo::META))
                    .line_height(px(typo::META_LINE + 2.0))
                    .text_color(theme.ink_tertiary)
                    .child("On this computer Ctrl is the secondary modifier; on a Mac it is Cmd."),
            )
            .into_any_element()
    }

    fn settings_about(theme: &Theme) -> AnyElement {
        let line = |label: &'static str, value: String| {
            div()
                .h(px(28.0))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(typo::BODY))
                        .text_color(theme.ink_secondary)
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(typo::META))
                        .text_color(theme.ink)
                        .child(value),
                )
                .into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_5))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(widgets::section_label("This app", theme))
                    .child(line("Version", env!("CARGO_PKG_VERSION").to_string()))
                    .child(line("Engine", "Embedded Aucteris Core".to_string()))
                    .child(line("UI stack", "gpui".to_string())),
            )
            .child(
                div()
                    .text_size(px(typo::META))
                    .line_height(px(typo::META_LINE + 2.0))
                    .text_color(theme.ink_tertiary)
                    .child("Bots and conversations are stored locally. Agent runs execute through the selected CLI in the bot's working folder."),
            )
            .into_any_element()
    }
}
