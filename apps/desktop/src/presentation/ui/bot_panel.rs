//! The bot panel: everything about one bot, in one place.
//!
//! Settings that belong to a single bot live here, not in app settings. Rows
//! that look interactive are interactive — each one changes something and the
//! change is visible immediately.

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement, Styled, div, prelude::*, px,
};

use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::dialog::PickerHost;
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets::{self, compact_count};

impl Shell {
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
                cx.listener(|this, _event, _window, cx| this.toggle_panel(cx)),
            ));

        let identity = div()
            .flex()
            .gap(px(metrics::SPACE_3))
            .items_center()
            .child(widgets::mascot(
                bot.mascot,
                color,
                bot.presence,
                40.0,
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
            .on_click(cx.listener(|this, _event, window, cx| {
                this.panel_picker.open = !this.panel_picker.open;
                if this.panel_picker.open {
                    window.focus(&this.panel_picker_focus.clone(), cx);
                } else {
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

        let picker_popup = picker_open.then(|| {
            self.provider_picker_view(
                theme,
                cx,
                &self.panel_picker,
                &bot.runtime,
                PickerHost::Panel,
            )
        });

        let folder_row = div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(metrics::MENU_ROW))
            .child(
                div()
                    .text_size(px(typo::BODY))
                    .text_color(theme.ink_secondary)
                    .child("Folder"),
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

        let sandbox_row = self.panel_row(
            theme,
            cx,
            "sandbox-row",
            "Sandbox",
            crate::domain::model::sandbox_label(bot.sandbox).to_string(),
            true,
            move |this, cx| this.cycle_sandbox(bot_id, cx),
        );

        let instructions = div()
            .p(px(metrics::SPACE_3))
            .rounded(px(metrics::RADIUS_CARD))
            .bg(theme.inset)
            .text_size(px(typo::META))
            .line_height(px(typo::META_LINE + 2.0))
            .text_color(theme.ink_secondary)
            .child(bot.instructions.to_string());

        let usage_fraction = (bot.tokens as f32 / 500_000.0).clamp(0.0, 1.0);
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
                                this.reset_usage(bot_id, cx)
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
                            .flex()
                            .flex_col()
                            .gap(px(metrics::SPACE_1))
                            .child(widgets::section_label("Agent CLI", theme))
                            .child(runtime_chip)
                            .when_some(picker_popup, |this, popup| this.child(popup)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(metrics::SPACE_1))
                            .child(widgets::section_label("Working folder", theme))
                            .child(folder_row),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(metrics::SPACE_1))
                            .child(widgets::section_label("Sandbox", theme))
                            .child(sandbox_row),
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
                            this.remove_bot_from_room(room_id, bot_id, cx)
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
                            this.add_bot_to_room(room_id, bot_id, cx)
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
                cx.listener(|this, _event, _window, cx| this.toggle_panel(cx)),
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
        F: Fn(&mut Shell, &mut Context<Shell>) + 'static,
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
                    .on_click(cx.listener(move |this, _event, _window, cx| on_click(this, cx)))
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
