//! Dialogs: creating a bot, and the app-wide settings.
//!
//! A dialog is the only floating surface in the app, so it is the only thing
//! that casts a shadow and the only thing behind a scrim. Both dialogs are
//! keyboard-complete: Tab moves between fields, Enter commits, Escape leaves.

use std::ops::Range;

use gpui::{
    AnyElement, Context, FocusHandle, FontWeight, HighlightStyle, IntoElement, KeyDownEvent,
    ParentElement, PathPromptOptions, ScrollHandle, Styled, StyledText, Window, div, prelude::*,
    px,
};

use crate::domain::model::{Bot, Chat, MascotShape, Mode, Presence};
use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets;
use aucteris_harness::komet_proto::SandboxLevel;

/// The four text fields of the new-bot form, in Tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Role,
    Folder,
    Instructions,
}

impl Field {
    fn index(self) -> usize {
        match self {
            Self::Name => 0,
            Self::Role => 1,
            Self::Folder => 2,
            Self::Instructions => 3,
        }
    }

    fn next(self, backwards: bool) -> Self {
        let order = [Self::Name, Self::Role, Self::Folder, Self::Instructions];
        let current = self.index();
        let next = if backwards {
            (current + order.len() - 1) % order.len()
        } else {
            (current + 1) % order.len()
        };
        order[next]
    }

    fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Role => "Role",
            Self::Folder => "Folder",
            Self::Instructions => "Instructions",
        }
    }
}

/// What the person has typed so far.
///
/// One form, two jobs: with `target` empty it creates a bot, with a bot id it
/// edits that bot in place. Fields, keys and layout are identical either way, so
/// learning one teaches the other.
pub struct BotForm {
    /// `None` creates; `Some(id)` edits that bot.
    pub target: Option<u32>,
    pub name: String,
    pub role: String,
    pub folder: String,
    pub instructions: String,
    pub runtime: String,
    pub color: usize,
    pub mascot: MascotShape,
    pub active: Field,
    pub focus: [FocusHandle; 4],
    pub provider: ProviderPicker,
}

impl BotForm {
    pub fn new(runtime: String, color: usize, focus: [FocusHandle; 4]) -> Self {
        Self {
            target: None,
            name: String::new(),
            role: "General".to_string(),
            folder: "~".to_string(),
            instructions: String::new(),
            runtime,
            color,
            mascot: MascotShape::Sparkle,
            active: Field::Name,
            focus,
            provider: ProviderPicker::default(),
        }
    }

    /// Open the form on an existing bot: every field starts at its current value.
    pub fn from_bot(bot: &Bot, focus: [FocusHandle; 4]) -> Self {
        Self {
            target: Some(bot.id),
            name: bot.name.to_string(),
            role: bot.role.to_string(),
            folder: bot.folder.to_string(),
            instructions: bot.instructions.to_string(),
            runtime: bot.runtime.to_string(),
            color: bot.color,
            mascot: bot.mascot,
            active: Field::Name,
            focus,
            provider: ProviderPicker::default(),
        }
    }

    fn editing(&self) -> bool {
        self.target.is_some()
    }

    fn text(&self, field: Field) -> &str {
        match field {
            Field::Name => &self.name,
            Field::Role => &self.role,
            Field::Folder => &self.folder,
            Field::Instructions => &self.instructions,
        }
    }

    fn text_mut(&mut self, field: Field) -> &mut String {
        match field {
            Field::Name => &mut self.name,
            Field::Role => &mut self.role,
            Field::Folder => &mut self.folder,
            Field::Instructions => &mut self.instructions,
        }
    }

    fn can_submit(&self) -> bool {
        !self.name.trim().is_empty() && !self.runtime.is_empty()
    }
}

/// What the person has typed so far for a new team: a title and the bots
/// that will share its chat.
pub struct TeamForm {
    pub title: String,
    /// Selected bot ids, in click order.
    pub members: Vec<u32>,
    pub focus: FocusHandle,
}

impl TeamForm {
    pub fn new(focus: FocusHandle) -> Self {
        Self {
            title: String::new(),
            members: Vec::new(),
            focus,
        }
    }

    fn can_submit(&self) -> bool {
        !self.title.trim().is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPage {
    General,
    Behavior,
    Shortcuts,
    About,
}

impl SettingsPage {
    const ALL: [SettingsPage; 4] = [
        SettingsPage::General,
        SettingsPage::Behavior,
        SettingsPage::Shortcuts,
        SettingsPage::About,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Behavior => "Behavior",
            Self::Shortcuts => "Shortcuts",
            Self::About => "About",
        }
    }
}

/// One pickable entry in the provider dropdown: a CLI default or one of the
/// models that CLI exposes (like Komet's per-agent model list).
struct ProviderChoice {
    /// Value stored in `BotForm::runtime` — bare cli id or `runtime:model`.
    id: String,
    icon: &'static str,
    label: String,
    detail: String,
    badge: Option<String>,
    /// Provider rail bucket (same list as `AgentCli::provider`).
    provider: &'static str,
}

/// Selectable runtime/model pairs — only for CLIs actually installed on
/// PATH. A provider that isn't present never appears in the picker.
fn provider_choices(engine: &crate::application::engine::Engine) -> Vec<ProviderChoice> {
    use crate::application::engine::Engine;
    let mut choices = Vec::new();
    for cli in engine.available_runtimes() {
        if !Engine::runtime_usable(cli.id) {
            continue;
        }
        choices.push(ProviderChoice {
            id: cli.id.to_string(),
            icon: cli.icon,
            label: format!("{} (default)", cli.label),
            detail: format!("{} · {}", cli.provider, cli.executable),
            badge: Some("Default".to_string()),
            provider: cli.provider,
        });
        for model in engine.models_for(cli.id) {
            choices.push(ProviderChoice {
                id: model.id,
                icon: model.icon,
                label: model.label,
                detail: model.description,
                badge: model.badge,
                provider: cli.provider,
            });
        }
    }
    choices
}

fn choice_matches(choice: &ProviderChoice, rail: Option<&str>, query: &str) -> bool {
    if let Some(rail) = rail
        && choice.provider != rail
    {
        return false;
    }
    if query.is_empty() {
        return true;
    }
    choice.label.to_lowercase().contains(query)
        || choice.detail.to_lowercase().contains(query)
        || choice.id.to_lowercase().contains(query)
        || choice.provider.to_lowercase().contains(query)
}

/// Search, rail filter and scroll of a provider list. Shared by the bot
/// form's dropdown and the bot panel's chip so both behave identically.
#[derive(Default)]
pub struct ProviderPicker {
    pub open: bool,
    pub search: String,
    pub rail: Option<String>,
    /// Must outlive re-renders or the offset resets on every keystroke.
    pub scroll: ScrollHandle,
}

impl ProviderPicker {
    /// Back to the top after the filter changes — otherwise a stale offset
    /// can land past the end of a shorter list.
    fn reset_scroll(&mut self) {
        self.scroll.set_offset(gpui::point(px(0.0), px(0.0)));
    }
}

/// Which copy of the picker a click mutates: the open bot form, or the
/// bot panel's chip.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PickerHost {
    Form,
    Panel,
}

pub enum Dialog {
    Bot(Box<BotForm>),
    Team(TeamForm),
    Settings(SettingsPage),
}

impl Shell {
    // ---- opening ---------------------------------------------------------

    fn form_focuses(cx: &mut Context<Self>) -> [FocusHandle; 4] {
        [
            cx.focus_handle(),
            cx.focus_handle(),
            cx.focus_handle(),
            cx.focus_handle(),
        ]
    }

    pub(crate) fn open_new_bot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = Self::form_focuses(cx);
        let color = self.workspace.bots.len() % 8;
        let runtime = self
            .available_runtimes
            .iter()
            .find(|cli| cli.id == self.settings.default_runtime)
            .or_else(|| self.available_runtimes.first())
            .map(|cli| cli.id.to_string())
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
        self.dialog = Some(Dialog::Settings(SettingsPage::General));
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

    pub(crate) fn pick_working_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Select working folder".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = prompt.await
                && let Some(path) = paths.pop()
            {
                let path_str = path.to_string_lossy().to_string();
                let _ = this.update(cx, |this, cx| {
                    if let Some(Dialog::Bot(form)) = this.dialog.as_mut() {
                        form.folder = path_str;
                        cx.notify();
                    }
                });
            }
        })
        .detach();
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
                    if let Some(matched) = provider_choices(&self.engine)
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
        let mut trigger_folder_picker = false;
        if let Some(Dialog::Bot(form)) = self.dialog.as_mut() {
            match key {
                "tab" => {
                    form.active = form.active.next(modifiers.shift);
                    refocus = Some(form.active);
                }
                "enter" => {
                    if form.active == Field::Folder {
                        trigger_folder_picker = true;
                    } else if modifiers.shift && form.active == Field::Instructions {
                        form.text_mut(form.active).push('\n');
                    } else {
                        submit = form.can_submit();
                    }
                }
                "space" => {
                    if form.active == Field::Folder {
                        trigger_folder_picker = true;
                    } else {
                        form.text_mut(form.active).push(' ');
                    }
                }
                "backspace" => {
                    if form.active != Field::Folder {
                        form.text_mut(form.active).pop();
                    }
                }
                _ => {
                    if modifiers.control || modifiers.platform {
                        return;
                    }
                    if form.active != Field::Folder {
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
        }
        if trigger_folder_picker {
            self.pick_working_folder(window, cx);
            return;
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
        let folder = if form.folder.trim().is_empty() {
            "~".to_string()
        } else {
            form.folder.trim().to_string()
        };
        let instructions = if form.instructions.trim().is_empty() {
            "Ask before touching anything outside the working folder.".to_string()
        } else {
            form.instructions.trim().to_string()
        };
        let runtime = form.runtime.clone();

        // Editing: everything the form owns changes in place, in the same order
        // the panel shows it. The chat title follows the name.
        if let Some(id) = form.target {
            if let Some(bot) = self.workspace.bot_mut(id) {
                bot.name = name.clone().into();
                bot.role = role.clone().into();
                bot.runtime = runtime.clone().into();
                bot.folder = folder.clone().into();
                bot.instructions = instructions.into();
                bot.color = form.color;
                bot.mascot = form.mascot;
            }
            if let Some(chat) = self
                .workspace
                .chats
                .iter_mut()
                .find(|chat| chat.bot == Some(id))
            {
                chat.title = name.into();
            }
            window.focus(&self.focus, cx);
            self.persist_workspace();
            cx.notify();
            return;
        }

        let id = self
            .workspace
            .bots
            .iter()
            .map(|bot| bot.id)
            .max()
            .unwrap_or(0)
            + 1;
        let chat_id = self
            .workspace
            .chats
            .iter()
            .map(|chat| chat.id)
            .max()
            .unwrap_or(0)
            + 1;

        self.workspace.bots.push(Bot {
            id,
            name: name.clone().into(),
            role: role.clone().into(),
            color: form.color,
            mascot: form.mascot,
            runtime: runtime.clone().into(),
            presence: Presence::Resting,
            activity: "Ready".into(),
            folder: folder.clone().into(),
            instructions: instructions.into(),
            tokens: 0,
            sandbox: SandboxLevel::WorkspaceWrite,
        });
        self.workspace.chats.push(Chat {
            id: chat_id,
            title: name.clone().into(),
            bot: Some(id),
            members: Vec::new(),
            messages: Vec::new(),
            preview: "Ready for your first task".into(),
            time: "now".into(),
            session_id: None,
        });
        self.workspace.selected = self.workspace.chats.len() - 1;
        self.draft.clear();
        self.scroll.scroll_to_bottom();
        window.focus(&self.focus, cx);
        self.persist_workspace();
        cx.notify();
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
        let title = form.title.trim().to_string();
        // Members answer in workspace order, not click order.
        let mut members = form.members.clone();
        members.sort_by_key(|id| {
            self.workspace
                .bots
                .iter()
                .position(|bot| bot.id == *id)
                .unwrap_or(usize::MAX)
        });
        members.dedup();
        self.create_team(title, members, cx);
        window.focus(&self.focus, cx);
    }

    // ---- rendering -------------------------------------------------------

    pub(crate) fn view_overlay(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let panel = match self.dialog.as_ref()? {
            Dialog::Bot(form) => self.bot_form_dialog(theme, cx, form),
            Dialog::Team(form) => self.team_dialog(theme, cx, form),
            Dialog::Settings(page) => self.settings_dialog(theme, cx, *page),
        };
        Some(
            div()
                .id("dialog-scrim")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                // Modal: nothing behind the scrim — chat, sidebar, panels —
                // may keep scrolling or reacting to the pointer.
                .occlude()
                .bg(theme.scrim)
                .child(panel)
                .into_any_element(),
        )
    }

    fn dialog_shell(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        width: f32,
        header: AnyElement,
        body: AnyElement,
        footer: AnyElement,
    ) -> AnyElement {
        div()
            .id("dialog")
            .w(px(width))
            .max_h(px(600.0))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(metrics::RADIUS_DIALOG))
            .bg(theme.dialog)
            .border_1()
            .border_color(theme.edge)
            .shadow_xl()
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::on_dialog_key))
            .child(header)
            .child(body)
            .child(footer)
            .into_any_element()
    }

    fn dialog_header(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        title: &'static str,
        subtitle: Option<String>,
    ) -> AnyElement {
        div()
            .flex_shrink_0()
            .px(px(metrics::SPACE_4))
            .pt(px(metrics::SPACE_4))
            .pb(px(metrics::SPACE_3))
            .flex()
            .items_start()
            .gap(px(metrics::SPACE_3))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(typo::SHEET_TITLE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.ink)
                            .child(title),
                    )
                    .when_some(subtitle, |this, subtitle| {
                        this.child(
                            div()
                                .text_size(px(typo::META))
                                .line_height(px(typo::META_LINE))
                                .text_color(theme.ink_tertiary)
                                .child(subtitle),
                        )
                    }),
            )
            .child(widgets::icon_button(
                ("dialog-close", 0),
                "x",
                theme,
                cx.listener(|this, _event, window, cx| this.close_dialog(window, cx)),
            ))
            .into_any_element()
    }

    /// One text field: a plate and the value, with a blinking caret after the
    /// last character while the field is active.
    fn form_field(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        form: &BotForm,
        field: Field,
        mono: bool,
    ) -> AnyElement {
        let active = form.active == field;
        let value = form.text(field).to_string();
        let empty = value.is_empty();
        let tall = field == Field::Instructions;
        let placeholder = match field {
            Field::Name => "Ada",
            Field::Role => "Research",
            Field::Folder => "~/code/project",
            Field::Instructions => "How this bot should work",
        };
        let text_color = if empty { theme.ink_tertiary } else { theme.ink };
        let font = if mono {
            theme.font_mono.clone()
        } else {
            theme.font_sans.clone()
        };

        // The caret is a `|` glyph inside the text run, highlighted in the
        // theme's secondary ink, so it lands after the last character — or
        // before the placeholder when the field is empty — on every line of
        // a multiline field alike.
        let (display, caret_ranges) = caret_spans(&value, placeholder, active, self.caret_visible);
        let content = StyledText::new(display).with_highlights(
            caret_ranges
                .into_iter()
                .map(|range| (range, HighlightStyle::color(theme.ink_secondary))),
        );

        div()
            .id(("field", field.index()))
            .h(px(if tall {
                metrics::CONTROL_LG * 2.4
            } else {
                metrics::CONTROL_LG
            }))
            .px(px(10.0))
            .flex()
            .items_center()
            .when(tall, |this| this.items_start().pt(px(9.0)))
            .gap(px(2.0))
            .overflow_hidden()
            .rounded(px(metrics::RADIUS_CONTROL))
            .bg(theme.inset)
            .border_1()
            .border_color(if active { theme.edge } else { theme.hairline })
            .cursor_text()
            .track_focus(&form.focus[field.index()])
            .on_click(
                cx.listener(move |this, _event, window, cx| this.focus_field(field, window, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .when(!tall, |this| this.truncate())
                    .font_family(font)
                    .text_size(px(typo::BODY))
                    .line_height(px(19.0))
                    .text_color(text_color)
                    .child(content),
            )
            .into_any_element()
    }

    fn labelled(
        &self,
        theme: &Theme,
        label: &'static str,
        hint: Option<&'static str>,
        control: AnyElement,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_1))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(metrics::SPACE_1))
                    .child(
                        div()
                            .text_size(px(typo::META))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.ink_secondary)
                            .child(label),
                    )
                    .when_some(hint, |this, hint| {
                        this.child(
                            div()
                                .text_size(px(typo::TINY))
                                .text_color(theme.ink_tertiary)
                                .child(hint),
                        )
                    }),
            )
            .child(control)
            .into_any_element()
    }

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
    fn apply_provider_choice(&mut self, host: PickerHost, choice: String, cx: &mut Context<Self>) {
        match host {
            PickerHost::Form => {
                if let Some(Dialog::Bot(form)) = self.dialog.as_mut() {
                    form.runtime = choice;
                    form.provider.open = false;
                }
            }
            PickerHost::Panel => {
                if let Some(bot_id) = self.workspace.selected_bot() {
                    if let Some(bot) = self.workspace.bot_mut(bot_id) {
                        bot.runtime = choice.into();
                    }
                    self.persist_workspace();
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
                        this.apply_provider_choice(host, choice_id.clone(), cx);
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

    fn bot_form_dialog(&self, theme: &Theme, cx: &mut Context<Self>, form: &BotForm) -> AnyElement {
        let editing = form.editing();
        let header = self.dialog_header(
            theme,
            cx,
            if editing { "Edit bot" } else { "New bot" },
            Some(if editing {
                "The chat, the sidebar and the panel follow these fields the moment you save."
                    .to_string()
            } else {
                "It answers in this chat. You can change any of this later in its panel."
                    .to_string()
            }),
        );

        let name = self.labelled(
            theme,
            Field::Name.label(),
            None,
            self.form_field(theme, cx, form, Field::Name, false),
        );
        let role = self.labelled(
            theme,
            Field::Role.label(),
            None,
            self.form_field(theme, cx, form, Field::Role, false),
        );

        let hover = theme.hover_wash();
        let folder_active = form.active == Field::Folder;
        let folder_text = form.folder.trim().to_string();
        let folder_empty = folder_text.is_empty();

        let folder_control = div()
            .id("form-folder-picker")
            .h(px(metrics::CONTROL_LG))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .rounded(px(metrics::RADIUS_CONTROL))
            .bg(theme.inset)
            .border_1()
            .border_color(if folder_active {
                theme.edge
            } else {
                theme.hairline
            })
            .cursor_pointer()
            .track_focus(&form.focus[Field::Folder.index()])
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(|this, _event, window, cx| {
                this.focus_field(Field::Folder, window, cx);
                this.pick_working_folder(window, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .min_w_0()
                    .flex_1()
                    .child(widgets::icon("folder", 16.0, theme.ink_secondary))
                    .child(
                        div()
                            .truncate()
                            .font_family(theme.font_mono.clone())
                            .text_size(px(typo::BODY))
                            .text_color(if folder_empty {
                                theme.ink_tertiary
                            } else {
                                theme.ink
                            })
                            .child(if folder_empty {
                                "~/my-project".to_string()
                            } else {
                                folder_text
                            }),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(8.0))
                    .py(px(2.0))
                    .rounded_full()
                    .bg(theme.control)
                    .text_size(px(typo::TINY))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.ink_secondary)
                    .child("Browse..."),
            )
            .into_any_element();

        let folder = self.labelled(theme, "Working folder", None, folder_control);
        let instructions = self.labelled(
            theme,
            Field::Instructions.label(),
            Some("Optional · Shift+Enter for a new line"),
            self.form_field(theme, cx, form, Field::Instructions, false),
        );

        let runtime_name = self.engine.runtime_label(&form.runtime);
        let current_icon = crate::application::engine::Engine::runtime_icon(&form.runtime);
        let is_provider_open = form.provider.open;

        let provider_trigger = div()
            .id("form-provider-trigger")
            .h(px(metrics::CONTROL_LG))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(metrics::RADIUS_CONTROL))
            .bg(theme.inset)
            .border_1()
            .border_color(if is_provider_open {
                theme.edge
            } else {
                theme.hairline
            })
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |this, _event, _window, cx| {
                if let Some(Dialog::Bot(form)) = this.dialog.as_mut() {
                    form.provider.open = !form.provider.open;
                }
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(widgets::icon(current_icon, 16.0, theme.ink))
                    .child(
                        div()
                            .text_size(px(typo::BODY))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.ink)
                            .child(if runtime_name.is_empty() {
                                "Select Provider".to_string()
                            } else {
                                runtime_name
                            }),
                    ),
            )
            .child(widgets::icon(
                if is_provider_open {
                    "x"
                } else {
                    "chevron-down"
                },
                12.0,
                theme.ink_tertiary,
            ));

        let provider_section = div()
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_2))
            .child(provider_trigger)
            .when(is_provider_open, |this| {
                this.child(self.provider_picker_view(
                    theme,
                    cx,
                    &form.provider,
                    &form.runtime,
                    PickerHost::Form,
                ))
            })
            .into_any_element();

        let mut swatches: Vec<AnyElement> = Vec::new();
        for slot in 0..8 {
            let color = theme.identity(slot);
            let selected = form.color == slot;
            swatches.push(
                div()
                    .id(("swatch", slot))
                    .p(px(2.0))
                    .rounded_full()
                    .border_1()
                    .border_color(if selected { theme.ink } else { theme.hairline })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        if let Some(Dialog::Bot(form)) = this.dialog.as_mut() {
                            form.color = slot;
                        }
                        cx.notify();
                    }))
                    .child(widgets::mascot(
                        form.mascot,
                        color,
                        Presence::Resting,
                        22.0,
                        100 + slot,
                    ))
                    .into_any_element(),
            );
        }

        let mut mascot_buttons: Vec<AnyElement> = Vec::new();
        for shape in MascotShape::ALL {
            let selected = form.mascot == shape;
            let shape_index = shape.index();
            mascot_buttons.push(
                div()
                    .id(("mascot-pick", shape_index))
                    .p(px(2.0))
                    .rounded(px(metrics::RADIUS_CONTROL))
                    .border_1()
                    .border_color(if selected { theme.ink } else { theme.hairline })
                    .bg(if selected {
                        theme.hover_wash()
                    } else {
                        gpui::transparent_black()
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        if let Some(Dialog::Bot(form)) = this.dialog.as_mut() {
                            form.mascot = shape;
                        }
                        cx.notify();
                    }))
                    .child(widgets::mascot(
                        shape,
                        theme.identity(form.color),
                        if selected {
                            Presence::Working
                        } else {
                            Presence::Resting
                        },
                        26.0,
                        200 + shape_index,
                    ))
                    .into_any_element(),
            );
        }

        // Text fields come first: name, role, folder and instructions stay in
        // the first viewport, above the mascot/color pickers.
        let identity_fields = div()
            .flex()
            .gap(px(metrics::SPACE_3))
            .child(div().flex_1().child(name))
            .child(div().w(px(150.0)).child(role))
            .into_any_element();

        let body = div()
            .id("dialog-body")
            .flex_1()
            // While the provider popover is open it lives inside this scroll
            // area; freezing the scroll keeps it attached to its trigger.
            .when(!form.provider.open, |this| this.overflow_y_scroll())
            .px(px(metrics::SPACE_4))
            .pb(px(metrics::SPACE_4))
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_3))
            .child(self.labelled(theme, "Provider", None, provider_section))
            .child(identity_fields)
            .child(folder)
            .child(instructions)
            .child(
                self.labelled(
                    theme,
                    "Mascot Shape",
                    Some("Choose from 26 expressive agent silhouettes"),
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(metrics::SPACE_1))
                        .children(mascot_buttons)
                        .into_any_element(),
                ),
            )
            .child(
                self.labelled(
                    theme,
                    "Color",
                    Some("Its mark, and its name in the transcript"),
                    div()
                        .flex()
                        .items_center()
                        .gap(px(metrics::SPACE_2))
                        // Live preview: the pick and the mood read at a glance.
                        .child(widgets::mascot(
                            form.mascot,
                            theme.identity(form.color),
                            Presence::Working,
                            40.0,
                            999,
                        ))
                        .child(div().flex().gap(px(metrics::SPACE_1)).children(swatches))
                        .into_any_element(),
                ),
            )
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
                    .child(if editing {
                        "Enter saves · Tab moves · Esc cancels"
                    } else {
                        "Enter creates · Tab moves · Esc closes"
                    }),
            )
            .child(widgets::segment(
                ("form-cancel", 0),
                "Cancel",
                false,
                theme,
                cx.listener(|this, _event, window, cx| this.close_dialog(window, cx)),
            ))
            .child(
                div()
                    .id("form-create")
                    .h(px(26.0))
                    .px(px(12.0))
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
                        cx.listener(|this, _event, window, cx| this.commit_bot_form(window, cx)),
                    )
                    .child(if editing {
                        "Save changes"
                    } else {
                        "Create bot"
                    })
                    .into_any_element(),
            )
            .into_any_element();

        self.dialog_shell(theme, cx, 516.0, header, body, footer)
    }

    /// The team form: a title field with the same caret as the bot form,
    /// then one toggle row per bot.
    fn team_dialog(&self, theme: &Theme, cx: &mut Context<Self>, form: &TeamForm) -> AnyElement {
        let header = self.dialog_header(
            theme,
            cx,
            "New team",
            Some("A title, then the bots that share this chat.".to_string()),
        );

        let empty = form.title.is_empty();
        let (display, caret_ranges) =
            caret_spans(&form.title, "Launch crew", true, self.caret_visible);
        let content = StyledText::new(display).with_highlights(
            caret_ranges
                .into_iter()
                .map(|range| (range, HighlightStyle::color(theme.ink_secondary))),
        );
        let title_control = div()
            .id("team-title-field")
            .h(px(metrics::CONTROL_LG))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(2.0))
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
                    .line_height(px(19.0))
                    .text_color(if empty { theme.ink_tertiary } else { theme.ink })
                    .child(content),
            )
            .into_any_element();

        let mut bot_rows: Vec<AnyElement> = Vec::new();
        for bot in &self.workspace.bots {
            let bot_id = bot.id;
            let selected = form.members.contains(&bot_id);
            let hover = theme.hover_wash();
            bot_rows.push(
                div()
                    .id(("team-bot", bot_id as usize))
                    .h(px(40.0))
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
                            .gap(px(1.0))
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
                .gap(px(2.0))
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
                    .h(px(26.0))
                    .px(px(12.0))
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
                    .child("Create team")
                    .into_any_element(),
            )
            .into_any_element();

        self.dialog_shell(theme, cx, 440.0, header, body, footer)
    }

    fn settings_dialog(
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
                        this.toggle_appearance(cx);
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
                        this.toggle_appearance(cx);
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
                cx.listener(|this, _event, _window, cx| this.set_mode(Mode::Simple, cx)),
            ))
            .child(widgets::segment(
                ("settings-mode", 1),
                Mode::Advanced.label(),
                self.settings.mode == Mode::Advanced,
                theme,
                cx.listener(|this, _event, _window, cx| this.set_mode(Mode::Advanced, cx)),
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

    // ---- settings that do something -------------------------------------

    pub(crate) fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.settings.mode = mode;
        cx.notify();
    }
}

/// Compose the text a form field shows, plus the byte range of its caret.
///
/// The caret is always at the end of the value — insertion is append-only —
/// rendered as a `|` glyph the caller highlights. An empty field shows the
/// caret *before* the placeholder instead. Inactive fields and the blink's
/// off phase return the plain text with no caret range.
pub(crate) fn caret_spans(
    value: &str,
    placeholder: &str,
    active: bool,
    caret_visible: bool,
) -> (String, Vec<Range<usize>>) {
    let shown = if value.is_empty() { placeholder } else { value };
    if !active || !caret_visible {
        return (shown.to_string(), Vec::new());
    }
    let caret_at = if value.is_empty() { 0 } else { shown.len() };
    let mut text = String::with_capacity(shown.len() + 1);
    if value.is_empty() {
        text.push('|');
    }
    text.push_str(shown);
    if !value.is_empty() {
        text.push('|');
    }
    (
        text,
        std::iter::once(caret_at..caret_at + 1).collect::<Vec<Range<usize>>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::caret_spans;

    #[test]
    fn caret_follows_the_last_character() {
        let (text, ranges) = caret_spans("Ada", "Name…", true, true);
        assert_eq!(text, "Ada|");
        assert_eq!(ranges, vec![3..4]);
    }

    #[test]
    fn caret_leads_the_placeholder_when_empty() {
        let (text, ranges) = caret_spans("", "Ada", true, true);
        assert_eq!(text, "|Ada");
        assert_eq!(ranges, vec![0..1]);
    }

    #[test]
    fn inactive_field_shows_no_caret() {
        let (text, ranges) = caret_spans("Ada", "Name…", false, true);
        assert_eq!(text, "Ada");
        assert!(ranges.is_empty());
    }

    #[test]
    fn blink_off_hides_the_caret() {
        let (text, ranges) = caret_spans("Ada", "Name…", true, false);
        assert_eq!(text, "Ada");
        assert!(ranges.is_empty());
        let (text, ranges) = caret_spans("", "Ada", true, false);
        assert_eq!(text, "Ada");
        assert!(ranges.is_empty());
    }

    #[test]
    fn caret_survives_multiline_values() {
        let (text, ranges) = caret_spans("one\ntwo", "…", true, true);
        assert_eq!(text, "one\ntwo|");
        assert_eq!(ranges, vec![7..8]);
    }
}
