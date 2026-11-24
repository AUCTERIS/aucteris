//! App-wide preferences, grouped by what they change.

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement, Styled, div, prelude::*, px,
};

use crate::domain::model::{AppearancePreference, Mode};
use crate::presentation::theme::{Appearance, Theme, metrics, text as typo};
use crate::presentation::ui::action::UiAction;
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
            Some("Preferences for this Aucteris workspace.".into()),
        );

        let mut navigation = Vec::new();
        for entry in SettingsPage::ALL {
            let selected = entry == page;
            let (glyph, detail) = match entry {
                SettingsPage::General => ("panel", "Information density"),
                SettingsPage::Appearance => ("sun", "Color theme"),
                SettingsPage::Providers => ("sparkle", "Agent CLIs and models"),
                SettingsPage::Behavior => ("sliders", "Message composer"),
                SettingsPage::Shortcuts => ("terminal", "Keyboard commands"),
                SettingsPage::About => ("bot", "App details"),
            };
            let hover = theme.hover_wash();
            navigation.push(
                div()
                    .id(("settings-tab", entry as usize))
                    .w_full()
                    .px(px(metrics::SPACE_2))
                    .py(px(metrics::SPACE_2))
                    .flex()
                    .items_center()
                    .gap(px(metrics::SPACE_2))
                    .rounded(px(metrics::RADIUS_MENU_ROW))
                    .when(selected, |this| this.bg(theme.raised))
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dialog = Some(Dialog::Settings(entry));
                        cx.notify();
                    }))
                    .child(widgets::icon(
                        glyph,
                        15.0,
                        if selected {
                            theme.ink
                        } else {
                            theme.ink_tertiary
                        },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(typo::META))
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
                                    .child(entry.label()),
                            )
                            .child(
                                div()
                                    .text_size(px(typo::TINY))
                                    .text_color(theme.ink_tertiary)
                                    .child(detail),
                            ),
                    )
                    .into_any_element(),
            );
        }

        let content = match page {
            SettingsPage::General => self.settings_general(theme, cx),
            SettingsPage::Appearance => self.settings_appearance(theme, cx),
            SettingsPage::Providers => self.settings_providers(theme, cx),
            SettingsPage::Behavior => self.settings_behavior(theme, cx),
            SettingsPage::Shortcuts => self.settings_shortcuts(theme),
            SettingsPage::About => self.settings_about(theme),
        };

        let body = div()
            .flex()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .child(
                div()
                    .w(px(190.0))
                    .h_full()
                    .flex_shrink_0()
                    .p(px(metrics::SPACE_2))
                    .flex()
                    .flex_col()
                    .gap(px(metrics::SPACE_1))
                    .bg(theme.panel)
                    .children(navigation),
            )
            .child(widgets::edge(theme.hairline))
            .child(
                div()
                    .id("settings-body")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .px(px(metrics::SPACE_4))
                    .py(px(metrics::SPACE_4))
                    .child(content),
            )
            .into_any_element();

        let footer = div()
            .flex_shrink_0()
            .px(px(metrics::SPACE_4))
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
                    .child("Changes save automatically · Esc closes"),
            )
            .child(
                div()
                    .id("settings-done")
                    .h(px(metrics::CONTROL_MD))
                    .px(px(metrics::SPACE_3))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .bg(theme.accent)
                    .cursor_pointer()
                    .text_size(px(typo::META))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.on_accent)
                    .on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx)))
                    .child("Done")
                    .into_any_element(),
            )
            .into_any_element();

        self.dialog_shell(theme, cx, 760.0, header, body, footer)
    }

    fn settings_general(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let mode = div()
            .flex()
            .gap(px(metrics::SPACE_1))
            .child(widgets::segment(
                ("settings-mode", 0),
                Mode::Simple.label(),
                self.settings.mode == Mode::Simple,
                theme,
                cx.listener(|this, _, _, cx| this.dispatch(UiAction::SetMode(Mode::Simple), cx)),
            ))
            .child(widgets::segment(
                ("settings-mode", 1),
                Mode::Advanced.label(),
                self.settings.mode == Mode::Advanced,
                theme,
                cx.listener(|this, _, _, cx| this.dispatch(UiAction::SetMode(Mode::Advanced), cx)),
            ));

        div()
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_3))
            .child(Self::settings_section(
                theme,
                "Information density",
                "Advanced mode reveals usage and runtime details in bot panels.",
                mode.into_any_element(),
            ))
            .into_any_element()
    }

    fn settings_appearance(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let appearance = div()
            .flex()
            .gap(px(metrics::SPACE_1))
            .child(widgets::segment(
                ("settings-skin", 0),
                "Midnight",
                self.appearance == Appearance::Dark,
                theme,
                cx.listener(|this, _, _, cx| {
                    this.dispatch(UiAction::SetAppearance(AppearancePreference::Dark), cx)
                }),
            ))
            .child(widgets::segment(
                ("settings-skin", 1),
                "Daylight",
                self.appearance == Appearance::Light,
                theme,
                cx.listener(|this, _, _, cx| {
                    this.dispatch(UiAction::SetAppearance(AppearancePreference::Light), cx)
                }),
            ));
        div()
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_3))
            .child(Self::settings_section(
                theme,
                "Color theme",
                "Applies to the sidebar, chat, panels and dialogs. The choice is saved on this device.",
                appearance.into_any_element(),
            ))
            .into_any_element()
    }

    fn settings_providers(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let available = self.engine.available_runtimes();
        let enabled_count = available
            .iter()
            .filter(|cli| {
                !self
                    .settings
                    .disabled_runtimes
                    .iter()
                    .any(|id| id == cli.id)
            })
            .count();
        let mut cards = Vec::new();
        for (index, cli) in crate::application::engine::SUPPORTED.iter().enumerate() {
            let installed = available.iter().any(|candidate| candidate.id == cli.id);
            let enabled = installed
                && !self
                    .settings
                    .disabled_runtimes
                    .iter()
                    .any(|id| id == cli.id);
            let is_default = enabled
                && crate::application::engine::Engine::runtime_base(&self.settings.default_runtime)
                    == cli.id;
            let expanded = self.provider_expanded.contains(cli.id);
            let cli_id = cli.id.to_string();
            let header_cli_id = cli_id.clone();
            let hover = theme.hover_wash();
            let mut card = div()
                .id(("settings-provider", index))
                .w_full()
                .rounded(px(metrics::RADIUS_CARD))
                .bg(theme.card)
                .border_1()
                .border_color(theme.hairline)
                .overflow_hidden()
                .child(
                    div()
                        .id(("settings-provider-header", index))
                        .w_full()
                        .px(px(metrics::SPACE_3))
                        .py(px(metrics::SPACE_2))
                        .flex()
                        .items_center()
                        .gap(px(metrics::SPACE_2))
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.provider_expanded.remove(&header_cli_id) {
                                this.provider_expanded.insert(header_cli_id.clone());
                            }
                            cx.notify();
                        }))
                        .child(widgets::icon(cli.icon, 19.0, theme.ink_secondary))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(metrics::SPACE_1))
                                        .child(
                                            div()
                                                .text_size(px(typo::META))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.ink)
                                                .child(cli.label),
                                        )
                                        .when(is_default, |row| {
                                            row.child(widgets::icon("check", 12.0, theme.accent))
                                        }),
                                )
                                .child(
                                    div()
                                        .text_size(px(typo::TINY))
                                        .text_color(if installed {
                                            theme.success
                                        } else {
                                            theme.ink_tertiary
                                        })
                                        .child(if installed {
                                            format!(
                                                "{} · {} · {}",
                                                cli.provider,
                                                cli.executable,
                                                if enabled {
                                                    "Visible in pickers"
                                                } else {
                                                    "Hidden from pickers"
                                                }
                                            )
                                        } else {
                                            format!(
                                                "{} · {} · Not detected",
                                                cli.provider, cli.executable
                                            )
                                        }),
                                ),
                        )
                        .when(installed && (!enabled || enabled_count > 1), |header| {
                            header.child(widgets::switch(
                                ("settings-provider-enabled", index),
                                enabled,
                                theme,
                                cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if enabled {
                                        if enabled_count <= 1 {
                                            return;
                                        }
                                        if !this.settings.disabled_runtimes.contains(&cli_id) {
                                            this.settings.disabled_runtimes.push(cli_id.clone());
                                        }
                                        if crate::application::engine::Engine::runtime_base(
                                            &this.settings.default_runtime,
                                        ) == cli_id
                                        {
                                            if let Some(next) =
                                                this.engine.available_runtimes().iter().find(
                                                    |candidate| {
                                                        candidate.id != cli_id
                                                            && !this
                                                                .settings
                                                                .disabled_runtimes
                                                                .iter()
                                                                .any(|id| id == candidate.id)
                                                    },
                                                )
                                            {
                                                this.settings.default_runtime = next.id.to_string();
                                            }
                                        }
                                    } else {
                                        this.settings.disabled_runtimes.retain(|id| id != &cli_id);
                                    }
                                    this.persist_workspace();
                                    cx.notify();
                                }),
                            ))
                        })
                        .when(installed && enabled && enabled_count == 1, |header| {
                            header.child(
                                div()
                                    .text_size(px(typo::TINY))
                                    .text_color(theme.ink_tertiary)
                                    .child("Keep one enabled"),
                            )
                        })
                        .child(widgets::icon(
                            if expanded {
                                "chevron-down"
                            } else {
                                "chevron-right"
                            },
                            13.0,
                            theme.ink_tertiary,
                        )),
                );

            if expanded {
                let body = if !installed {
                    div()
                        .px(px(metrics::SPACE_3))
                        .py(px(metrics::SPACE_2))
                        .text_size(px(typo::TINY))
                        .text_color(theme.ink_tertiary)
                        .child(format!(
                            "Install `{}` and restart Aucteris to make this provider available.",
                            cli.executable
                        ))
                        .into_any_element()
                } else if !enabled {
                    div()
                        .px(px(metrics::SPACE_3))
                        .py(px(metrics::SPACE_2))
                        .text_size(px(typo::TINY))
                        .text_color(theme.ink_tertiary)
                        .child("Show this provider in pickers to choose it for new bots.")
                        .into_any_element()
                } else {
                    let choices = super::provider_choices(
                        &self.engine,
                        &self.settings.disabled_runtimes,
                        None,
                    )
                    .into_iter()
                    .filter(|choice| choice.provider == cli.provider)
                    .collect::<Vec<_>>();
                    let mut rows = Vec::new();
                    for choice in choices {
                        let selected = self.settings.default_runtime == choice.id;
                        let runtime_choice = choice.id.clone();
                        let choice_element_id = format!("settings-provider-choice-{}", choice.id);
                        let row_hover = theme.hover_wash();
                        rows.push(
                            div()
                                .id(choice_element_id)
                                .w_full()
                                .min_w_0()
                                .px(px(metrics::SPACE_2))
                                .py(px(metrics::SPACE_1))
                                .flex()
                                .items_center()
                                .gap(px(metrics::SPACE_2))
                                .rounded(px(metrics::RADIUS_CONTROL))
                                .when(selected, |row| row.bg(theme.raised))
                                .cursor_pointer()
                                .hover(move |style| style.bg(row_hover))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.settings.default_runtime = runtime_choice.clone();
                                    this.persist_workspace();
                                    cx.notify();
                                }))
                                .child(widgets::icon(
                                    choice.icon,
                                    16.0,
                                    if selected {
                                        theme.ink
                                    } else {
                                        theme.ink_secondary
                                    },
                                ))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap(px(1.0))
                                        .child(
                                            div()
                                                .text_size(px(typo::META))
                                                .text_color(theme.ink)
                                                .child(choice.label),
                                        )
                                        .when(!choice.detail.is_empty(), |column| {
                                            column.child(
                                                div()
                                                    .text_size(px(typo::TINY))
                                                    .text_color(theme.ink_tertiary)
                                                    .child(choice.detail),
                                            )
                                        }),
                                )
                                .when(selected, |row| {
                                    row.child(widgets::icon("check", 13.0, theme.accent))
                                })
                                .into_any_element(),
                        );
                    }
                    div()
                        .px(px(metrics::SPACE_3))
                        .py(px(metrics::SPACE_2))
                        .flex()
                        .flex_col()
                        .gap(px(metrics::SPACE_1))
                        .border_t_1()
                        .border_color(theme.hairline)
                        .child(
                            div()
                                .mb(px(metrics::SPACE_1))
                                .text_size(px(typo::TINY))
                                .text_color(theme.ink_tertiary)
                                .child("Default for new bots · Choose a model or the CLI default"),
                        )
                        .children(rows)
                        .into_any_element()
                };
                card = card.child(body);
            }
            cards.push(card.into_any_element());
        }

        div()
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_3))
            .child(Self::settings_section(
                theme,
                "Providers",
                "Show installed CLIs in provider pickers and choose the default provider or model for new bots. Existing bots keep their own selection.",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(metrics::SPACE_1))
                    .children(cards)
                    .into_any_element(),
            ))
            .into_any_element()
    }

    fn settings_behavior(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let send_behavior = div()
            .flex()
            .items_center()
            .gap(px(metrics::SPACE_3))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(typo::META))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.ink)
                            .child("Enter sends the message"),
                    )
                    .child(
                        div()
                            .text_size(px(typo::TINY))
                            .line_height(px(typo::META_LINE))
                            .text_color(theme.ink_tertiary)
                            .child(if self.settings.send_on_enter {
                                "Turn off to use Enter for new lines."
                            } else {
                                "When off, Ctrl/Cmd+Enter sends the message."
                            }),
                    ),
            )
            .child(widgets::switch(
                ("settings-send", 0),
                self.settings.send_on_enter,
                theme,
                cx.listener(|this, _, _, cx| {
                    this.settings.send_on_enter = !this.settings.send_on_enter;
                    this.persist_workspace();
                    cx.notify();
                }),
            ));

        div()
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_3))
            .child(Self::settings_section(
                theme,
                "Message composer",
                "This changes how Enter behaves in every chat.",
                send_behavior.into_any_element(),
            ))
            .into_any_element()
    }

    fn settings_shortcuts(&self, theme: &Theme) -> AnyElement {
        let modifier = if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Ctrl"
        };
        let send_combos = if self.settings.send_on_enter {
            vec![vec!["Enter"]]
        } else {
            vec![vec![modifier, "Enter"]]
        };
        let newline_combos = if self.settings.send_on_enter {
            vec![vec!["Shift", "Enter"]]
        } else {
            vec![vec!["Enter"], vec!["Shift", "Enter"]]
        };
        let rows: Vec<(&str, Vec<Vec<&str>>)> = vec![
            ("Send the message", send_combos),
            ("Insert a new line", newline_combos),
            ("Create a bot", vec![vec![modifier, "N"]]),
            ("Search chats", vec![vec![modifier, "F"]]),
            ("Toggle bot panel", vec![vec![modifier, "I"]]),
            ("Switch appearance", vec![vec![modifier, "J"]]),
            ("Close a dialog", vec![vec!["Esc"]]),
        ];
        let mut elements = Vec::new();
        for (label, combos) in rows {
            let mut combo_elements = Vec::new();
            for (combo_index, keys) in combos.iter().enumerate() {
                if combo_index > 0 {
                    combo_elements.push(
                        div()
                            .px(px(2.0))
                            .text_size(px(typo::TINY))
                            .text_color(theme.ink_tertiary)
                            .child("or")
                            .into_any_element(),
                    );
                }
                let mut keycaps = Vec::new();
                for (index, key) in keys.iter().enumerate() {
                    if index > 0 {
                        keycaps.push(
                            div()
                                .text_size(px(typo::TINY))
                                .text_color(theme.ink_tertiary)
                                .child("+")
                                .into_any_element(),
                        );
                    }
                    keycaps.push(widgets::keycap(*key, theme));
                }
                combo_elements.push(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(metrics::SPACE_1))
                        .children(keycaps)
                        .into_any_element(),
                );
            }
            elements.push(
                div()
                    .h(px(34.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme.hairline)
                    .child(
                        div()
                            .text_size(px(typo::META))
                            .text_color(theme.ink_secondary)
                            .child(label),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(metrics::SPACE_1))
                            .children(combo_elements),
                    )
                    .into_any_element(),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_3))
            .child(Self::settings_section(
                theme,
                "Keyboard shortcuts",
                "Shortcuts use the system modifier key.",
                div()
                    .flex()
                    .flex_col()
                    .children(elements)
                    .into_any_element(),
            ))
            .into_any_element()
    }

    fn settings_about(&self, theme: &Theme) -> AnyElement {
        let version = env!("CARGO_PKG_VERSION").to_string();
        let data_dir = self.engine.data_dir().display().to_string();
        let runtimes = self
            .engine
            .available_runtimes()
            .iter()
            .map(|cli| cli.label)
            .collect::<Vec<_>>();
        let runtime_summary = if runtimes.is_empty() {
            "None detected".to_string()
        } else {
            runtimes.join(", ")
        };
        div()
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_3))
            .child(Self::settings_section(
                theme,
                "Aucteris",
                "Application and local workspace information.",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(metrics::SPACE_1))
                    .child(Self::info_row(theme, "Version", version))
                    .child(Self::info_row(theme, "Available agents", runtime_summary))
                    .child(Self::info_row(theme, "Data folder", data_dir))
                    .into_any_element(),
            ))
            .into_any_element()
    }

    fn settings_section(
        theme: &Theme,
        title: &'static str,
        description: &'static str,
        body: AnyElement,
    ) -> AnyElement {
        div()
            .w_full()
            .p(px(metrics::SPACE_3))
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_3))
            .rounded(px(metrics::RADIUS_CARD))
            .bg(theme.card)
            .border_1()
            .border_color(theme.hairline)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(typo::BODY))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.ink)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(typo::TINY))
                            .line_height(px(typo::META_LINE))
                            .text_color(theme.ink_tertiary)
                            .child(description),
                    ),
            )
            .child(body)
            .into_any_element()
    }

    fn info_row(theme: &Theme, label: &'static str, value: String) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .py(px(metrics::SPACE_1))
            .flex()
            .items_start()
            .justify_between()
            .gap(px(metrics::SPACE_3))
            .border_b_1()
            .border_color(theme.hairline)
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(typo::META))
                    .text_color(theme.ink_secondary)
                    .child(label),
            )
            .child(
                div()
                    .min_w_0()
                    .max_w(px(320.0))
                    .text_right()
                    .text_size(px(typo::TINY))
                    .text_color(theme.ink)
                    .truncate()
                    .child(value),
            )
            .into_any_element()
    }
}
