//! The provider/model picker shared by the bot form dropdown and the bot
//! panel chip: filtering, rail tabs, scrolling and applying a choice.

use gpui::{AnyElement, Context, FontWeight, KeyDownEvent, Styled, Window, div, prelude::*, px};

use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets;

use super::{Dialog, PickerHost, ProviderPicker, choice_matches, provider_choices};

impl Shell {
    /// The picker copy a click mutates: the open bot form's, or the panel's.
    pub(crate) fn picker_mut(&mut self, host: PickerHost) -> Option<&mut ProviderPicker> {
        match host {
            PickerHost::Form => match self.dialog.as_mut() {
                Some(Dialog::Bot(form)) => Some(&mut form.provider),
                _ => None,
            },
            PickerHost::Panel => Some(&mut self.panel_picker),
        }
    }

    /// A picker row was clicked: write the choice where its host keeps it.
    /// The form edits `runtime` until save; the panel writes the bot live.
    pub(crate) fn apply_provider_choice(
        &mut self,
        host: PickerHost,
        choice: String,
        cx: &mut Context<Self>,
    ) {
        match host {
            PickerHost::Form => {
                if let Some(Dialog::Bot(form)) = self.dialog.as_mut() {
                    form.runtime = choice;
                    form.provider.open = false;
                }
            }
            PickerHost::Panel => {
                if let Some(bot_id) = self.workspace.selected_bot()
                    && let Some(bot) = self.workspace.bot_mut(bot_id)
                {
                    bot.runtime = choice.into();
                }
                self.panel_picker.open = false;
                self.panel_picker.search.clear();
                self.panel_picker.rail = None;
                self.panel_picker.reset_scroll();
            }
        }
        cx.notify();
    }

    /// Keys typed inside the panel's picker filter its search, like the
    /// dialog's `on_dialog_key` branch does for the form's.
    pub(crate) fn on_panel_picker_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.panel_picker.open {
            return;
        }
        self.caret_visible = true;
        let modifiers = event.keystroke.modifiers;
        if modifiers.control || modifiers.platform {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.panel_picker.open = false;
                window.focus(&self.focus, cx);
            }
            "enter" => {
                // Same as the form's picker: Enter takes the first match.
                let query = self.panel_picker.search.trim().to_lowercase();
                let rail = self.panel_picker.rail.clone();
                if let Some(matched) = provider_choices(&self.engine)
                    .iter()
                    .find(|choice| choice_matches(choice, rail.as_deref(), &query))
                {
                    let choice = matched.id.clone();
                    self.apply_provider_choice(PickerHost::Panel, choice, cx);
                    return;
                }
                self.panel_picker.open = false;
            }
            "backspace" => {
                self.panel_picker.search.pop();
                self.panel_picker.reset_scroll();
            }
            key => {
                let typed = event
                    .keystroke
                    .key_char
                    .clone()
                    .or_else(|| (key == "space").then(|| " ".to_string()))
                    .or_else(|| (key.chars().count() == 1).then(|| key.to_string()));
                if let Some(typed) = typed.filter(|text| !text.is_empty()) {
                    self.panel_picker.search.push_str(&typed);
                    self.panel_picker.reset_scroll();
                }
            }
        }
        cx.notify();
    }

    /// The provider/model list — one view for the bot form's dropdown and the
    /// panel's chip, so both filter, scroll and pick identically.
    pub(crate) fn provider_picker_view(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        picker: &ProviderPicker,
        current: &str,
        host: PickerHost,
    ) -> AnyElement {
        let hover = theme.hover_wash();
        let purple = gpui::hsla(0.72, 0.75, 0.65, 1.0);
        let current_rail = picker.rail.clone();
        let search_query = picker.search.clone();
        let query = search_query.trim().to_lowercase();
        let search_empty = search_query.is_empty();

        // 1. Search Bar at top
        let search_bar = div()
            .h(px(38.0))
            .px(px(10.0))
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.hairline)
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(widgets::icon("search", 13.0, theme.ink_tertiary))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(typo::META))
                    .text_color(if search_empty {
                        theme.ink_tertiary
                    } else {
                        theme.ink
                    })
                    .child(if search_empty {
                        "Search providers...".to_string()
                    } else {
                        search_query.clone()
                    }),
            )
            .when(!search_empty, |this| {
                this.child(
                    div()
                        .id("provider-search-clear")
                        .p(px(2.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            if let Some(picker) = this.picker_mut(host) {
                                picker.search.clear();
                                picker.reset_scroll();
                            }
                            cx.notify();
                        }))
                        .child(widgets::icon("x", 12.0, theme.ink_tertiary)),
                )
            });

        // 2. Left Rail with provider tabs, derived from the catalog so a new
        // provider appears without touching this view.
        let all_selected = current_rail.is_none();
        let mut rail_col = div()
            .w(px(42.0))
            .flex_shrink_0()
            .p(px(4.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .border_r_1()
            .border_color(theme.hairline)
            // Star / All tab
            .child(
                div()
                    .id("provider-rail-all")
                    .relative()
                    .w(px(34.0))
                    .h(px(34.0))
                    .rounded(px(6.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .when(!all_selected, |el| el.hover(move |s| s.bg(hover)))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        if let Some(picker) = this.picker_mut(host) {
                            picker.rail = None;
                            picker.reset_scroll();
                        }
                        cx.notify();
                    }))
                    .child(widgets::icon(
                        "star",
                        15.0,
                        if all_selected {
                            theme.ink
                        } else {
                            theme.ink_tertiary
                        },
                    ))
                    .when(all_selected, |el| {
                        el.child(
                            div()
                                .absolute()
                                .right(px(-4.0))
                                .w(px(3.0))
                                .h(px(18.0))
                                .rounded_full()
                                .bg(purple),
                        )
                    }),
            )
            // Divider
            .child(div().h(px(1.0)).mx(px(-4.0)).my(px(2.0)).bg(theme.hairline));

        // Unique providers in catalog order, each with its brand icon.
        let mut providers: Vec<(&'static str, &'static str)> = Vec::new();
        for choice in provider_choices(&self.engine) {
            if !providers.iter().any(|(name, _)| *name == choice.provider) {
                providers.push((choice.provider, choice.icon));
            }
        }

        for (ix, (provider_name, provider_icon)) in providers.iter().enumerate() {
            let is_provider_selected = current_rail.as_deref() == Some(*provider_name);
            let provider_str = provider_name.to_string();
            rail_col = rail_col.child(
                div()
                    .id(("provider-rail-brand", ix))
                    .relative()
                    .w(px(34.0))
                    .h(px(34.0))
                    .rounded(px(6.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .when(!is_provider_selected, |el| el.hover(move |s| s.bg(hover)))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        if let Some(picker) = this.picker_mut(host) {
                            picker.rail = Some(provider_str.clone());
                            picker.reset_scroll();
                        }
                        cx.notify();
                    }))
                    .child(widgets::icon(
                        provider_icon,
                        16.0,
                        if is_provider_selected {
                            theme.ink
                        } else {
                            theme.ink_tertiary
                        },
                    ))
                    .when(is_provider_selected, |el| {
                        el.child(
                            div()
                                .absolute()
                                .right(px(-4.0))
                                .w(px(3.0))
                                .h(px(18.0))
                                .rounded_full()
                                .bg(purple),
                        )
                    }),
            );
        }

        // 3. Right List Area: provider defaults first, then the models each
        // agent exposes — the same shape Komet uses for agent model pickers.
        let choices = provider_choices(&self.engine);
        let mut filtered_items: Vec<AnyElement> = Vec::new();

        for choice in choices
            .iter()
            .filter(|choice| choice_matches(choice, current_rail.as_deref(), &query))
        {
            let is_selected = choice.id == current;
            let choice_id = choice.id.clone();
            let item_id = format!("provider-item-{}", choice.id);

            filtered_items.push(
                div()
                    .id(item_id)
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .bg(if is_selected {
                        theme.raised_hover
                    } else {
                        gpui::transparent_black()
                    })
                    .border_1()
                    .border_color(if is_selected {
                        theme.edge
                    } else {
                        gpui::transparent_black()
                    })
                    .hover(move |s| s.bg(hover))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.dispatch(
                            crate::presentation::ui::action::UiAction::ApplyProviderChoice {
                                host,
                                choice: choice_id.clone(),
                            },
                            cx,
                        );
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .min_w_0()
                            .flex_1()
                            .child(widgets::icon(choice.icon, 18.0, theme.ink))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(1.0))
                                    .min_w_0()
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(12.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.ink)
                                            .child(choice.label.clone()),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(10.5))
                                            .text_color(theme.ink_tertiary)
                                            .child(choice.detail.clone()),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .when_some(choice.badge.clone(), |this, badge| {
                                this.child(
                                    div()
                                        .px(px(6.0))
                                        .py(px(1.5))
                                        .rounded_full()
                                        .bg(theme.inset)
                                        .text_size(px(typo::TINY))
                                        .text_color(theme.ink_tertiary)
                                        .child(badge),
                                )
                            })
                            .when(is_selected, |this| {
                                this.child(widgets::icon("check", 13.0, theme.accent))
                            }),
                    )
                    .into_any_element(),
            );
        }

        let section_header = div()
            .px(px(8.0))
            .pt(px(6.0))
            .pb(px(2.0))
            .text_size(px(typo::TINY))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.ink_tertiary)
            .child(match current_rail.as_deref() {
                Some(b) => b.to_uppercase(),
                None => "PROVIDERS & MODELS".to_string(),
            });

        let list_area = div()
            .id("provider-list-area")
            .flex_1()
            .min_w_0()
            .overflow_y_scroll()
            .track_scroll(&picker.scroll)
            .p(px(4.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(section_header)
            .when(filtered_items.is_empty(), |this| {
                this.child(
                    div()
                        .p(px(12.0))
                        .text_size(px(typo::META))
                        .text_color(theme.ink_tertiary)
                        .child("No matching providers or models found"),
                )
            })
            .children(filtered_items);

        div()
            .id("provider-dropdown-popover")
            // The list owns the wheel while the pointer is over it; the
            // dialog body painted underneath must not scroll along.
            .occlude()
            // The panel's copy has no dialog key handler: it takes focus so
            // typing filters the search and Escape closes it.
            .when(host == PickerHost::Panel, |this| {
                this.track_focus(&self.panel_picker_focus)
                    .on_key_down(cx.listener(Self::on_panel_picker_key))
                    .on_click(cx.listener(|this, _event, window, cx| {
                        window.focus(&this.panel_picker_focus.clone(), cx);
                    }))
            })
            .h(px(250.0))
            .rounded(px(metrics::RADIUS_CONTROL))
            .bg(theme.panel)
            .border_1()
            .border_color(theme.edge)
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(search_bar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(rail_col)
                    .child(list_area),
            )
            .into_any_element()
    }
}
