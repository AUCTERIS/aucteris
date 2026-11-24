//! The new-team dialog: title plus the bots that share the chat.

use gpui::{
    AnyElement, Context, FontWeight, HighlightStyle, IntoElement, ParentElement, Styled,
    StyledText, div, prelude::*, px,
};

use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets;

use super::{Dialog, DialogShellPresentation, TeamForm, caret_spans};

impl Shell {
    /// The team form: a title field with the same caret as the bot form,
    /// then one toggle row per bot.
    pub(super) fn team_dialog(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        form: &TeamForm,
    ) -> AnyElement {
        let header = self.dialog_header(
            theme,
            cx,
            if form.target.is_some() {
                "Edit team"
            } else {
                "New team"
            },
            Some("A title, then the bots that share this chat.".to_string()),
        );

        let empty = form.title.is_empty();
        let (display, caret_ranges) =
            caret_spans(&form.title, "Launch crew", true, self.caret_visible);
        let content =
            StyledText::new(display).with_highlights(caret_ranges.into_iter().map(|range| {
                (
                    range,
                    HighlightStyle::color(if self.caret_visible {
                        theme.ink_secondary
                    } else {
                        theme.inset
                    }),
                )
            }));
        let title_control = div()
            .id("team-title-field")
            .role(gpui::Role::TextInput)
            .aria_label("Team name")
            .tab_index(0)
            .focus_visible({
                let focus = theme.focus;
                move |style| style.border_1().border_color(focus)
            })
            .h(px(metrics::CONTROL_LG))
            .px(px(metrics::PX_10))
            .flex()
            .items_center()
            .gap(px(metrics::PX_2))
            .overflow_hidden()
            .rounded(px(metrics::RADIUS_CONTROL))
            .bg(theme.inset)
            .border_1()
            .border_color(theme.edge)
            .cursor_text()
            .track_focus(&form.focus)
            .on_click(cx.listener(|this, _event, window, cx| {
                this.caret_visible = true;
                if let Some(Dialog::Team(form)) = this.dialog.as_mut() {
                    window.focus(&form.focus.clone(), cx);
                }
                cx.notify();
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(theme.font_sans.clone())
                    .text_size(px(typo::BODY))
                    .line_height(px(metrics::PX_19))
                    .text_color(if empty { theme.ink_tertiary } else { theme.ink })
                    .child(content),
            )
            .into_any_element();

        let mut bot_rows: Vec<AnyElement> = Vec::new();
        for bot in &self.workspace.bots {
            let bot_id = bot.id;
            let selected = form.members.contains(&bot_id);
            let hover = theme.hover_wash();
            let focus = theme.focus;
            let tab_index = bot_rows.len() as isize + 1;
            bot_rows.push(
                div()
                    .id(("team-bot", bot_id as usize))
                    .role(gpui::Role::CheckBox)
                    .aria_label(bot.name.to_string())
                    .aria_selected(selected)
                    .tab_index(tab_index)
                    .focus_visible(move |style| style.border_1().border_color(focus))
                    .h(px(metrics::PX_40))
                    .px(px(metrics::SPACE_2))
                    .flex()
                    .items_center()
                    .gap(px(metrics::SPACE_3))
                    .rounded(px(metrics::RADIUS_CONTROL))
                    .when(selected, |this| this.bg(theme.raised))
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        if let Some(Dialog::Team(form)) = this.dialog.as_mut() {
                            if let Some(pos) = form.members.iter().position(|id| *id == bot_id) {
                                form.members.remove(pos);
                            } else {
                                form.members.push(bot_id);
                            }
                        }
                        cx.notify();
                    }))
                    .child(widgets::mascot(
                        bot.mascot,
                        theme.identity(bot.color),
                        bot.presence,
                        26.0,
                        bot_id as usize * 1000 + 11,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(metrics::PX_1))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(typo::BODY))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.ink)
                                    .child(bot.name.to_string()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(typo::TINY))
                                    .text_color(theme.ink_tertiary)
                                    .child(format!("{} · {}", bot.role, bot.presence.label())),
                            ),
                    )
                    .child(widgets::presence_dot(bot.presence, theme, 6.0))
                    .when(selected, |this| {
                        this.child(widgets::icon("check", 14.0, theme.accent))
                    })
                    .into_any_element(),
            );
        }

        let bots_section = if bot_rows.is_empty() {
            div()
                .py(px(metrics::SPACE_2))
                .text_size(px(typo::META))
                .text_color(theme.ink_tertiary)
                .child("No bots yet — create one first, then come back for the team.")
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(metrics::PX_2))
                .children(bot_rows)
                .into_any_element()
        };

        let body = div()
            .id("dialog-body")
            .flex_1()
            .overflow_y_scroll()
            .px(px(metrics::SPACE_4))
            .pb(px(metrics::SPACE_4))
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_3))
            .child(self.labelled(
                theme,
                "Team name",
                Some("Enter creates · Esc closes"),
                title_control,
            ))
            .child(self.labelled(
                theme,
                "Bots",
                Some("Click a bot to add it, click again to remove"),
                bots_section,
            ))
            .into_any_element();

        let can_submit = form.can_submit();
        let accent = theme.accent;
        let on_accent = theme.on_accent;
        let control = theme.control;
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
                    .child("The team shares one chat; members can be changed later in its panel."),
            )
            .child(widgets::segment(
                ("team-cancel", 0),
                "Cancel",
                false,
                theme,
                cx.listener(|this, _event, window, cx| this.close_dialog(window, cx)),
            ))
            .child(
                div()
                    .id("team-create")
                    .h(px(metrics::PX_26))
                    .px(px(metrics::SPACE_3))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .bg(if can_submit { accent } else { control })
                    .when(can_submit, |this| this.cursor_pointer())
                    .text_size(px(typo::TINY))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if can_submit {
                        on_accent
                    } else {
                        theme.ink_tertiary
                    })
                    .on_click(
                        cx.listener(|this, _event, window, cx| this.commit_team_form(window, cx)),
                    )
                    .child(if form.target.is_some() {
                        "Save changes"
                    } else {
                        "Create team"
                    })
                    .into_any_element(),
            )
            .into_any_element();

        self.dialog_shell(
            theme,
            cx,
            DialogShellPresentation {
                width: 440.0,
                label: if form.target.is_some() {
                    "Edit team"
                } else {
                    "Create team"
                },
            },
            header,
            body,
            footer,
        )
    }
}
