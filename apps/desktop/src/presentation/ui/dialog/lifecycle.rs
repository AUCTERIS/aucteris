//! Dialog lifecycle: opening, keyboard handling and committing the bot,
//! team and settings surfaces.

use gpui::{Context, FocusHandle, KeyDownEvent, Window};

use crate::domain::model::Mode;

use super::Shell;
use super::{BotForm, Dialog, Field, SettingsPage, TeamForm, choice_matches, provider_choices};

impl Shell {
    // ---- opening ---------------------------------------------------------

    fn form_focuses(cx: &mut Context<Self>) -> [FocusHandle; 3] {
        [cx.focus_handle(), cx.focus_handle(), cx.focus_handle()]
    }

    pub(crate) fn open_new_bot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = Self::form_focuses(cx);
        let color = self.workspace.bots.len() % 8;
        let enabled = |id: &str| {
            !self
                .settings
                .disabled_runtimes
                .iter()
                .any(|disabled| disabled == id)
        };
        let default_provider =
            crate::application::engine::Engine::runtime_base(&self.settings.default_runtime);
        let runtime = self
            .available_runtimes
            .iter()
            .find(|cli| cli.id == default_provider && enabled(cli.id))
            .map(|_| self.settings.default_runtime.clone())
            .or_else(|| {
                self.available_runtimes
                    .iter()
                    .find(|cli| enabled(cli.id))
                    .map(|cli| cli.id.to_string())
            })
            .unwrap_or_default();
        self.dialog = Some(Dialog::Bot(Box::new(BotForm::new(runtime, color, focus))));
        self.caret_visible = true;
        self.create_menu_open = false;
        self.panel_picker.open = false;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    /// Edit a bot from its panel, through the same form that created it.
    pub(crate) fn open_edit_bot(
        &mut self,
        bot_id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bot) = self.workspace.bot(bot_id).cloned() else {
            return;
        };
        let focus = Self::form_focuses(cx);
        self.dialog = Some(Dialog::Bot(Box::new(BotForm::from_bot(&bot, focus))));
        self.caret_visible = true;
        self.create_menu_open = false;
        self.panel_picker.open = false;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_page(SettingsPage::General, window, cx);
    }

    pub(crate) fn open_settings_page(
        &mut self,
        page: SettingsPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dialog = Some(Dialog::Settings(page));
        self.caret_visible = true;
        self.create_menu_open = false;
        self.panel_picker.open = false;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    /// The team form: a title and the bots that share the new chat.
    pub(crate) fn open_new_team(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.dialog = Some(Dialog::Team(TeamForm::new(focus)));
        self.caret_visible = true;
        self.create_menu_open = false;
        self.panel_picker.open = false;
        cx.notify();
    }

    pub(crate) fn open_edit_team(
        &mut self,
        team_id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(chat) = self
            .workspace
            .chats
            .iter()
            .find(|chat| chat.id == team_id && chat.is_room())
            .cloned()
        else {
            return;
        };
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.dialog = Some(Dialog::Team(TeamForm::from_chat(&chat, focus)));
        self.context_menu = None;
        self.caret_visible = true;
        self.create_menu_open = false;
        cx.notify();
    }

    pub(crate) fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn focus_field(
        &mut self,
        field: Field,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(Dialog::Bot(form)) = self.dialog.as_mut() {
            form.active = field;
            form.provider.open = false;
            self.caret_visible = true;
            let handle = form.focus[field.index()].clone();
            window.focus(&handle, cx);
        }
        cx.notify();
    }

    // ---- keys ------------------------------------------------------------

    pub(crate) fn on_dialog_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        // Typing keeps the caret lit; the blink loop owns it from here.
        self.caret_visible = true;

        if key == "escape" {
            if let Some(Dialog::Bot(form)) = self.dialog.as_mut()
                && form.provider.open
            {
                form.provider.open = false;
                cx.notify();
                return;
            }
            self.close_dialog(window, cx);
            return;
        }
        if key == "tab"
            && matches!(
                self.dialog,
                Some(Dialog::Settings(_) | Dialog::Team(_) | Dialog::Bot(_))
            )
        {
            if let Some(Dialog::Bot(form)) = self.dialog.as_mut()
                && form.provider.open
            {
                form.provider.open = false;
                form.active = form.active.next(modifiers.shift);
                let field = form.active;
                self.focus_field(field, window, cx);
                cx.stop_propagation();
                return;
            }
            if !self.dialog_focus.contains_focused(window, cx) {
                window.focus(&self.dialog_focus, cx);
            }
            if modifiers.shift {
                window.focus_prev(cx);
            } else {
                window.focus_next(cx);
            }
            if !self.dialog_focus.contains_focused(window, cx) {
                window.focus(&self.dialog_focus, cx);
                window.focus_next(cx);
            }
            if let Some(Dialog::Bot(form)) = self.dialog.as_mut() {
                for field in [Field::Name, Field::Role, Field::Instructions] {
                    if form.focus[field.index()].is_focused(window) {
                        form.active = field;
                        break;
                    }
                }
            }
            cx.stop_propagation();
            return;
        }
        // The team form is a single field: type, delete, Enter commits.
        let mut team_submit = false;
        if let Some(Dialog::Team(form)) = self.dialog.as_mut() {
            match key {
                "enter" => team_submit = form.can_submit(),
                "backspace" => {
                    form.title.pop();
                }
                "tab" => {}
                _ => {
                    if modifiers.control || modifiers.platform {
                        cx.notify();
                        return;
                    }
                    let typed = event
                        .keystroke
                        .key_char
                        .clone()
                        .or_else(|| (key == "space").then(|| " ".to_string()))
                        .or_else(|| (key.chars().count() == 1).then(|| key.to_string()));
                    if let Some(typed) = typed.filter(|text| !text.is_empty()) {
                        form.title.push_str(&typed);
                    }
                }
            }
        }
        if team_submit {
            self.commit_team_form(window, cx);
            return;
        }
        if matches!(self.dialog, Some(Dialog::Team(_))) {
            cx.notify();
            return;
        }
        if !matches!(self.dialog, Some(Dialog::Bot(_))) {
            return;
        }

        let mut refocus: Option<Field> = None;

        // When the provider dropdown is open, typing filters the models.
        if let Some(Dialog::Bot(form)) = self.dialog.as_mut()
            && form.provider.open
        {
            match key {
                "tab" => {
                    form.provider.open = false;
                    form.active = form.active.next(modifiers.shift);
                    refocus = Some(form.active);
                }
                "backspace" => {
                    form.provider.search.pop();
                    form.provider.reset_scroll();
                    cx.notify();
                    return;
                }
                "enter" => {
                    let query = form.provider.search.trim().to_lowercase();
                    let rail_filter = form.provider.rail.as_deref();
                    if let Some(matched) = provider_choices(
                        &self.engine,
                        &self.settings.disabled_runtimes,
                        Some(&form.runtime),
                    )
                    .iter()
                    .find(|choice| choice_matches(choice, rail_filter, &query))
                    {
                        form.runtime = matched.id.clone();
                    }
                    form.provider.open = false;
                    cx.notify();
                    return;
                }
                _ => {
                    if modifiers.control || modifiers.platform {
                        return;
                    }
                    let typed = event
                        .keystroke
                        .key_char
                        .clone()
                        .or_else(|| (key == "space").then(|| " ".to_string()))
                        .or_else(|| (key.chars().count() == 1).then(|| key.to_string()));
                    if let Some(typed) = typed.filter(|text| !text.is_empty()) {
                        form.provider.search.push_str(&typed);
                        form.provider.reset_scroll();
                        cx.notify();
                        return;
                    }
                }
            }
            if let Some(field) = refocus {
                self.focus_field(field, window, cx);
            }
            return;
        }

        let mut submit = false;
        if let Some(Dialog::Bot(form)) = self.dialog.as_mut() {
            match key {
                "tab" => {
                    form.active = form.active.next(modifiers.shift);
                    refocus = Some(form.active);
                }
                "enter" => {
                    if modifiers.shift && form.active == Field::Instructions {
                        form.text_mut(form.active).push('\n');
                    } else {
                        submit = form.can_submit();
                    }
                }
                "space" => {
                    form.text_mut(form.active).push(' ');
                }
                "backspace" => {
                    form.text_mut(form.active).pop();
                }
                _ => {
                    if modifiers.control || modifiers.platform {
                        return;
                    }
                    let typed = event
                        .keystroke
                        .key_char
                        .clone()
                        .or_else(|| (key == "space").then(|| " ".to_string()))
                        .or_else(|| (key.chars().count() == 1).then(|| key.to_string()));
                    if let Some(typed) = typed.filter(|text| !text.is_empty()) {
                        form.text_mut(form.active).push_str(&typed);
                    }
                }
            }
        }
        if let Some(field) = refocus {
            self.focus_field(field, window, cx);
        }
        if submit {
            self.commit_bot_form(window, cx);
            return;
        }
        cx.notify();
    }

    // ---- committing ------------------------------------------------------

    /// Create the bot the form describes, or write the edits back to it.
    pub(crate) fn commit_bot_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Bot(form)) = self.dialog.take() else {
            return;
        };
        if !form.can_submit() {
            self.dialog = Some(Dialog::Bot(form));
            return;
        }

        let name = form.name.trim().to_string();
        let role = if form.role.trim().is_empty() {
            "General".to_string()
        } else {
            form.role.trim().to_string()
        };
        let instructions = if form.instructions.trim().is_empty() {
            "Read files as needed. Ask before writing outside the private workspace or approved folders.".to_string()
        } else {
            form.instructions.trim().to_string()
        };
        self.dispatch(
            crate::presentation::ui::action::UiAction::SaveBot(super::BotFormSubmission {
                target: form.target,
                name,
                role,
                instructions,
                runtime: form.runtime,
                color: form.color,
                mascot: form.mascot,
            }),
            cx,
        );
        window.focus(&self.focus, cx);
    }

    /// Create the team the form describes and land on its chat.
    pub(crate) fn commit_team_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Team(form)) = self.dialog.take() else {
            return;
        };
        if !form.can_submit() {
            self.dialog = Some(Dialog::Team(form));
            return;
        }

        let mut members = form.members;
        members.sort_by_key(|id| {
            self.workspace
                .bots
                .iter()
                .position(|bot| bot.id == *id)
                .unwrap_or(usize::MAX)
        });
        members.dedup();
        self.dispatch(
            crate::presentation::ui::action::UiAction::SaveTeam {
                target: form.target,
                title: form.title.trim().to_string(),
                members,
            },
            cx,
        );
        window.focus(&self.focus, cx);
    }

    pub(crate) fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.settings.mode = mode;
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Field, ProviderChoice, SettingsPage, choice_matches};

    #[test]
    fn field_tab_order_cycles_both_ways() {
        assert_eq!(Field::Name.next(false), Field::Role);
        assert_eq!(Field::Instructions.next(false), Field::Name);
        assert_eq!(Field::Name.next(true), Field::Instructions);
        assert_eq!(Field::Role.next(true), Field::Name);
        for field in [Field::Name, Field::Role, Field::Instructions] {
            assert_eq!(field.next(false).next(true), field);
        }
    }

    #[test]
    fn choice_matches_filters_rail_and_query() {
        let choice = ProviderChoice {
            id: "claude:opus".to_string(),
            icon: "claude-mark",
            label: "Claude Opus".to_string(),
            detail: "Anthropic · claude".to_string(),
            badge: None,
            provider: "Anthropic",
        };
        assert!(choice_matches(&choice, None, ""));
        assert!(choice_matches(&choice, Some("Anthropic"), "opus"));
        assert!(!choice_matches(&choice, Some("OpenAI"), ""));
        assert!(!choice_matches(&choice, None, "gemini"));
        // Query matches id and detail too, not only the label.
        assert!(choice_matches(&choice, None, "claude:op"));
    }

    #[test]
    fn settings_pages_have_labels() {
        for page in SettingsPage::ALL {
            assert!(!page.label().is_empty());
        }
    }
}
