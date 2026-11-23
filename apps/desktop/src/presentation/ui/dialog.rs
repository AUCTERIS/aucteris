//! Dialogs: creating a bot, and the app-wide settings.
//!
//! A dialog is the only floating surface in the app, so it is the only thing
//! that casts a shadow and the only thing behind a scrim. Both dialogs are
//! keyboard-complete: Tab moves between fields, Enter commits, Escape leaves.

mod bot_form;
mod lifecycle;
mod provider_picker;
mod settings;
mod team;

use std::ops::Range;

use gpui::{
    AnyElement, Context, FocusHandle, FontWeight, HighlightStyle, IntoElement, ParentElement,
    ScrollHandle, Styled, StyledText, div, prelude::*, px,
};

use crate::domain::model::{Bot, MascotShape};
use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
