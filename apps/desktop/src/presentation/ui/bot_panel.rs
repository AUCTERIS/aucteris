//! The bot panel: everything about one bot, in one place.
//!
//! Settings that belong to a single bot live here, not in app settings. Rows
//! that look interactive are interactive — each one changes something and the
//! change is visible immediately.

use aucteris_harness::komet_proto::SandboxLevel;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, IntoElement, ParentElement, Styled, div,
    prelude::*, px,
};

use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::dialog::PickerHost;
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets::{self, compact_count};

impl Shell {
    /// Popovers render at the shell root so the panel's scroll viewport cannot
    /// clip the model list or the permissions menu.
    pub(crate) fn view_panel_popovers(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let selected_bot_id = self.workspace.selected_bot();
        let current_bot = selected_bot_id.and_then(|id| self.workspace.bot(id));
        let provider_popup = if self.panel_picker.open {
            current_bot.and_then(|bot| {
                self.panel_picker_position.map(|position| {
                    // Keep the menu in the conversation column, to the left
                    // of the details panel, and align it around its trigger.
                    let top = (position.y - px(130.0)).max(px(8.0));
                    div()
                        .absolute()
                        .right(px(metrics::PANEL_WIDTH + metrics::SPACE_4))
                        .top(top)
                        .w(px(metrics::PANEL_WIDTH))
                        .child(self.provider_picker_view(
                            theme,
                            cx,
                            &self.panel_picker,
                            &bot.runtime,
                            PickerHost::Panel,
                        ))
                        .into_any_element()
                })
            })
        } else {
            None
        };

        let permissions_popup = if self.permissions_open {
            current_bot.and_then(|bot| {
                let bot_id = bot.id;
                self.permissions_position.map(|position| {
                    let levels = [
                        (
                            SandboxLevel::ReadOnly,
                            "Read only",
                            "Can inspect files without changing them.",
                        ),
                        (
                            SandboxLevel::WorkspaceWrite,
                            "Workspace write",
                            "Can edit the private workspace and approved folders. Other writes need your approval.",
                        ),
                        (
                            SandboxLevel::DangerFullAccess,
                            "Full access",
                            "Can access files and tools without sandbox limits.",
                        ),
                    ];
                    let mut options: Vec<AnyElement> = Vec::new();
                    for (level, label, detail) in levels {
                        let selected = bot.sandbox == level;
                        options.push(
                            div()
                                .id(format!("permission-{label}"))
                                .px(px(metrics::SPACE_2))
                                .py(px(metrics::SPACE_1))
                                .flex()
                                .items_center()
                                .gap(px(metrics::SPACE_2))
                                .rounded(px(metrics::RADIUS_MENU_ROW))
                                .cursor_pointer()
                                .hover(|style| style.bg(theme.raised_hover))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    if let Some(bot) = this.workspace.bot_mut(bot_id) {
                                        bot.sandbox = level;
                                    }
                                    this.permissions_open = false;
                                    this.permissions_position = None;
                                    this.persist_workspace();
                                    cx.notify();
                                }))
                                .child(widgets::icon(
                                    if selected { "check" } else { "panel" },
                                    14.0,
                                    if selected {
                                        theme.accent
                                    } else {
                                        theme.ink_tertiary
                                    },
                                ))
                                .child(
                                    div()
                                        .flex_1()
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.0))
                                        .child(
                                            div()
                                                .text_size(px(typo::META))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.ink)
                                                .child(label),
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

                    let top = (position.y - px(111.0)).max(px(8.0));
                    div()
                        .absolute()
                        .right(px(metrics::PANEL_WIDTH + metrics::SPACE_4))
                        .top(top)
                        .w(px(metrics::PANEL_WIDTH))
                        .p(px(metrics::SPACE_2))
                        .flex()
                        .flex_col()
                        .gap(px(metrics::SPACE_1))
                        .rounded(px(metrics::RADIUS_CONTROL))
                        .bg(theme.menu)
                        .border_1()
                        .border_color(theme.edge)
                        .shadow_md()
                        .child(
                            div()
                                .px(px(metrics::SPACE_2))
                                .pb(px(metrics::SPACE_1))
                                .text_size(px(typo::META))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.ink_secondary)
                                .child("Permission level"),
                        )
                        .children(options)
                        .into_any_element()
                })
            })
        } else {
            None
        };

        if provider_popup.is_none() && permissions_popup.is_none() {
            return None;
        }

        Some(
            div()
                .id("panel-popovers")
                .absolute()
                .inset_0()
                .occlude()
                .child(
                    div()
                        .id("panel-popover-dismiss")
                        .absolute()
                        .inset_0()
                        .on_click(cx.listener(|this, _event, window, cx| {
                            this.panel_picker.open = false;
                            this.panel_picker_position = None;
                            this.permissions_open = false;
                            this.permissions_position = None;
                            window.focus(&this.focus.clone(), cx);
                            cx.notify();
                        })),
                )
                .children(provider_popup)
                .children(permissions_popup)
                .into_any_element(),
        )
    }

    pub(crate) fn view_bot_panel(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let chat = self.workspace.chat().clone();
        let Some(bot_id) = chat.bot else {
            return self.room_panel(theme, cx, &chat);
        };
        let Some(bot) = self.workspace.bot(bot_id).cloned() else {
            return div().w(px(metrics::PANEL_WIDTH)).into_any_element();
        };

        let color = theme.identity(bot.color);
        let hover = theme.hover_wash();

        let header = div()
            .h(px(metrics::HEADER_HEIGHT))
            .flex_shrink_0()
            .px(px(metrics::SPACE_4))
            .flex()
            .items_center()
            .gap(px(metrics::SPACE_1))
            .child(div().flex_1().child(widgets::section_label("Bot", theme)))
            .child(widgets::icon_button(
                ("edit-bot", 0),
                "edit",
                theme,
                cx.listener(move |this, _event, window, cx| this.open_edit_bot(bot_id, window, cx)),
            ))
            .child(widgets::icon_button(
                ("close-panel", 0),
                "x",
                theme,
                cx.listener(|this, _event, _window, cx| {
                    this.dispatch(crate::presentation::ui::action::UiAction::TogglePanel, cx)
                }),
            ));

        let identity = div()
            .flex()
            .gap(px(metrics::SPACE_3))
            .items_center()
            .child(widgets::mascot(
                bot.mascot,
                color,
                bot.presence,
                metrics::MASCOT_PANEL,
                bot.id as usize * 1000 + 5,
            ))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(typo::SHEET_TITLE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(bot.name.to_string()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(typo::META))
                            .text_color(theme.ink_tertiary)
                            .child(format!("{} · {}", bot.role, bot.presence.label())),
                    ),
            );

        // Answers with — a chip that opens the same provider/model picker
        // the bot form uses, writing the choice to the bot immediately.
        let runtime_name = self.engine.runtime_label(&bot.runtime);
        let runtime_icon = crate::application::engine::Engine::runtime_icon(&bot.runtime);
        let picker_open = self.panel_picker.open;
        let chip_hover = theme.hover_wash();
        let runtime_chip = div()
            .id("panel-runtime-chip")
            .h(px(metrics::CONTROL_LG))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .rounded(px(metrics::RADIUS_CONTROL))
            .bg(theme.inset)
            .border_1()
            .border_color(if picker_open {
                theme.edge
            } else {
                theme.hairline
            })
            .cursor_pointer()
            .hover(move |style| style.bg(chip_hover))
            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                this.panel_picker.open = !this.panel_picker.open;
                if this.panel_picker.open {
                    this.panel_picker_position = Some(event.position());
                    this.permissions_open = false;
                    window.focus(&this.panel_picker_focus.clone(), cx);
                } else {
                    this.panel_picker_position = None;
                    window.focus(&this.focus.clone(), cx);
                }
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .min_w_0()
                    .child(widgets::icon(runtime_icon, 16.0, theme.ink))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(typo::BODY))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.ink)
                            .child(runtime_name),
                    ),
            )
            .child(widgets::icon(
                if picker_open { "x" } else { "chevron-down" },
                12.0,
                theme.ink_tertiary,
            ))
            .into_any_element();

        let folder_row = div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(metrics::MENU_ROW))
            .child(
                div()
                    .text_size(px(typo::BODY))
                    .text_color(theme.ink_secondary)
                    .child("Private workspace"),
            )
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_right()
                    .font_family(theme.font_mono.clone())
                    .text_size(px(typo::TINY))
                    .text_color(theme.ink_tertiary)
                    .child(bot.folder.to_string()),
            );

        let can_add_folders =
            crate::application::engine::Engine::supports_scoped_folders(&bot.runtime)
                && bot.sandbox == SandboxLevel::WorkspaceWrite;
        let allowed_folders = bot.allowed_folders.clone();
        let bot_id = bot.id;
        let mut allowed_rows: Vec<AnyElement> = Vec::new();
        for (index, folder) in allowed_folders.iter().enumerate() {
            allowed_rows.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(metrics::SPACE_2))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(theme.font_mono.clone())
                            .text_size(px(typo::TINY))
                            .text_color(theme.ink_secondary)
                            .child(folder.clone()),
                    )
                    .child(widgets::icon_button(
                        ("remove-allowed-folder", bot_id as usize * 1000 + index),
                        "x",
                        theme,
                        cx.listener(move |this, _event, _window, cx| {
                            this.remove_allowed_folder(bot_id, index, cx)
                        }),
                    ))
                    .into_any_element(),
            );
        }
        let add_folder_button = div()
            .id("add-allowed-folder")
            .h(px(metrics::CONTROL_MD))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(metrics::RADIUS_CONTROL))
            .bg(theme.inset)
            .cursor_pointer()
            .text_size(px(typo::META))
            .text_color(theme.ink_secondary)
            .when(can_add_folders, |this| {
                this.hover(|style| style.bg(theme.hover_wash()))
            })
            .when(can_add_folders, |this| {
                this.on_click(cx.listener(move |this, _event, window, cx| {
                    this.add_allowed_folder(bot_id, window, cx)
                }))
            })
            .child(widgets::icon("plus", 13.0, theme.ink_secondary))
            .child("Add folder");
        let allowed_section = div()
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_1))
            .child(widgets::section_label("Allowed folders", theme))
            .children(allowed_rows)
            .when(allowed_folders.is_empty(), |this| {
                this.child(
                    div()
                        .text_size(px(typo::TINY))
                        .text_color(theme.ink_tertiary)
                        .child(if can_add_folders {
                            "No additional folders"
                        } else if crate::application::engine::Engine::supports_scoped_folders(
                            &bot.runtime,
                        ) {
                            "Set permission level to Workspace write to add folders"
                        } else {
                            "This runtime does not support persistent folder access"
                        }),
                )
            })
            .when(can_add_folders, |this| this.child(add_folder_button));

        let permissions_row = self.panel_row(
            theme,
            cx,
            "permissions-row",
            "Permissions",
            crate::domain::model::sandbox_label(bot.sandbox).to_string(),
            true,
            move |this, event, cx| {
                this.permissions_open = !this.permissions_open;
                if this.permissions_open {
                    this.permissions_position = Some(event.position());
                    this.panel_picker.open = false;
                    this.panel_picker_position = None;
                } else {
                    this.permissions_position = None;
                }
                cx.notify();
            },
        );

        let instructions = div()
            .p(px(metrics::SPACE_3))
            .rounded(px(metrics::RADIUS_CARD))
            .bg(theme.inset)
            .text_size(px(typo::META))
            .line_height(px(typo::META_LINE + 2.0))
            .text_color(theme.ink_secondary)
            .child(bot.instructions.to_string());

        // Unknown window stays an empty meter: inventing a denominator
        // would make the bar a lie.
        let usage_fraction = bot
            .context_limit
            .map(|limit| (bot.tokens as f32 / limit.max(1) as f32).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let usage = div()
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_2))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(typo::META))
                            .text_color(theme.ink_secondary)
                            .child(format!("{} tokens this month", compact_count(bot.tokens))),
                    )
                    .child(
                        div()
                            .id("reset-usage")
                            .h(px(20.0))
                            .px(px(metrics::SPACE_2))
                            .flex()
                            .items_center()
                            .rounded_full()
                            .cursor_pointer()
                            .hover(move |style| style.bg(hover))
                            .text_size(px(typo::TINY))
                            .text_color(theme.ink_tertiary)
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.dispatch(
                                    crate::presentation::ui::action::UiAction::ResetUsage(bot_id),
                                    cx,
                                )
                            }))
                            .child("Reset"),
                    ),
            )
            .child(widgets::meter(usage_fraction, theme));

        div()
            .w(px(metrics::PANEL_WIDTH))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(theme.panel)
            .child(header)
            .child(
                div()
                    .id("bot-panel-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .px(px(metrics::SPACE_4))
                    .pb(px(metrics::SPACE_4))
                    .flex()
                    .flex_col()
                    .gap(px(metrics::SPACE_5))
                    .child(identity)
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_col()
                            .gap(px(metrics::SPACE_1))
                            .child(widgets::section_label("Provider", theme))
                            .child(runtime_chip),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(metrics::SPACE_1))
                            .child(widgets::section_label("Private workspace", theme))
                            .child(folder_row),
                    )
                    .child(allowed_section)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(metrics::SPACE_1))
                            .child(permissions_row),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(metrics::SPACE_2))
                            .child(widgets::section_label("Instructions", theme))
                            .child(instructions),
                    )
                    // Token counts are an Advanced detail.
                    .when(!self.settings.mode.is_simple(), |this| {
                        this.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(metrics::SPACE_2))
                                .child(widgets::section_label("Usage", theme))
                                .child(usage),
                        )
                    }),
            )
            .into_any_element()
    }

    fn room_panel(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        chat: &crate::domain::model::Chat,
    ) -> AnyElement {
        let room_id = chat.id;
        let mut members: Vec<AnyElement> = Vec::new();
        for id in &chat.members {
            let Some(bot) = self.workspace.bot(*id) else {
                continue;
            };
            let bot_id = bot.id;
            let color = theme.identity(bot.color);
            members.push(
                div()
                    .h(px(40.0))
                    .flex()
                    .items_center()
                    .gap(px(metrics::SPACE_2))
                    .child(widgets::mascot(
                        bot.mascot,
                        color,
                        bot.presence,
                        26.0,
                        bot.id as usize * 1000 + 6,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(typo::BODY))
                            .text_color(theme.ink)
                            .child(bot.name.to_string()),
                    )
                    .child(widgets::presence_dot(bot.presence, theme, 6.0))
                    .child(widgets::icon_button(
                        ("remove-room-member", bot.id as usize),
                        "x",
                        theme,
                        cx.listener(move |this, _event, _window, cx| {
                            this.dispatch(
                                crate::presentation::ui::action::UiAction::RemoveBotFromRoom {
                                    room: room_id,
                                    bot: bot_id,
                                },
                                cx,
                            )
                        }),
                    ))
                    .into_any_element(),
            );
        }

        let mut available_bots: Vec<AnyElement> = Vec::new();
        for bot in self
            .workspace
            .bots
            .iter()
            .filter(|bot| !chat.members.contains(&bot.id))
        {
            let bot_id = bot.id;
            available_bots.push(
                div()
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .gap(px(metrics::SPACE_2))
                    .child(widgets::mascot(
                        bot.mascot,
                        theme.identity(bot.color),
                        bot.presence,
                        24.0,
                        bot.id as usize * 1000 + 7,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(typo::META))
                            .text_color(theme.ink_secondary)
                            .child(bot.name.to_string()),
                    )
                    .child(widgets::icon_button(
                        ("add-room-member", bot_id as usize),
                        "plus",
                        theme,
                        cx.listener(move |this, _event, _window, cx| {
                            this.dispatch(
                                crate::presentation::ui::action::UiAction::AddBotToRoom {
                                    room: room_id,
                                    bot: bot_id,
                                },
                                cx,
                            )
                        }),
                    ))
                    .into_any_element(),
            );
        }

        let header = div()
            .h(px(metrics::HEADER_HEIGHT))
            .flex_shrink_0()
            .px(px(metrics::SPACE_4))
            .flex()
            .items_center()
            .justify_between()
            .child(widgets::section_label("Team", theme))
            .child(widgets::icon_button(
                ("close-panel", 1),
                "x",
                theme,
                cx.listener(|this, _event, _window, cx| {
                    this.dispatch(crate::presentation::ui::action::UiAction::TogglePanel, cx)
                }),
            ));

        div()
            .w(px(metrics::PANEL_WIDTH))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(theme.panel)
            .child(header)
            .child(
                div()
                    .flex_1()
                    .px(px(metrics::SPACE_4))
                    .flex()
                    .flex_col()
                    .gap(px(metrics::SPACE_4))
                    .child(
                        div()
                            .text_size(px(typo::SHEET_TITLE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(chat.title.to_string()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(metrics::SPACE_1))
                            .child(widgets::section_label("Bots in this team", theme))
                            .children(members)
                            .when(chat.members.is_empty(), |this| {
                                this.child(
                                    div()
                                        .py(px(metrics::SPACE_1))
                                        .text_size(px(typo::META))
                                        .text_color(theme.ink_tertiary)
                                        .child("Add bots below to build this team."),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(metrics::SPACE_1))
                            .child(widgets::section_label("Add bots", theme))
                            .children(available_bots),
                    )
                    .child(
                        div()
                            .text_size(px(typo::META))
                            .line_height(px(typo::META_LINE + 2.0))
                            .text_color(theme.ink_tertiary)
                            .child("Everyone in a team shares one chat. Add or remove bots as the work changes."),
                    ),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn panel_row<F>(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        id: &'static str,
        label: &'static str,
        value: String,
        interactive: bool,
        on_click: F,
    ) -> AnyElement
    where
        F: Fn(&mut Shell, &ClickEvent, &mut Context<Shell>) + 'static,
    {
        let hover = theme.hover_wash();
        div()
            .id(id)
            .h(px(metrics::MENU_ROW))
            .px(px(metrics::SPACE_1))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(metrics::SPACE_2))
            .rounded(px(metrics::RADIUS_MENU_ROW))
            .when(interactive, |this| {
                this.cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(
                        cx.listener(move |this, event, _window, cx| on_click(this, event, cx)),
                    )
            })
            .child(
                div()
                    .text_size(px(typo::BODY))
                    .text_color(theme.ink_secondary)
                    .child(label),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(metrics::SPACE_1))
                    .child(
                        div()
                            .max_w(px(190.0))
                            .truncate()
                            .text_size(px(typo::META))
                            .text_color(theme.ink)
                            .child(value),
                    )
                    .child(widgets::icon("chevron-right", 12.0, theme.ink_tertiary)),
            )
            .into_any_element()
    }
}
