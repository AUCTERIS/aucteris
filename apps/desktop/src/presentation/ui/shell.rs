//! The app shell: sidebar, chat, bot panel — the three places everything lives.
//!
//! The shell owns the workspace, the composer draft and the run that is in
//! flight. Views are plain methods on it (sidebar.rs, chat.rs, composer.rs,
//! bot_panel.rs) so a click can mutate the one source of truth without an
//! event bus.

mod actions;
mod keys;
mod run;

use gpui::{
    App, Context, FocusHandle, Focusable, Pixels, Point, Render, ScrollHandle, SharedString, Task,
    Window, div, prelude::*, px,
};

use crate::application::engine;
use crate::domain::model::{AppearancePreference, Settings, Workspace};
use crate::infrastructure::storage::AppStorage;
use crate::presentation::theme::{Appearance, Theme, metrics, text as typo};
use crate::presentation::ui::dialog::{Dialog, ProviderPicker};
use aucteris_harness::komet_proto::{PermissionChoice, Scope};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub(crate) enum ContextTarget {
    Bot(u32),
    Team(u32),
}

pub struct Shell {
    pub(crate) workspace: Workspace,
    pub(crate) engine: Arc<engine::Engine>,
    storage: AppStorage,
    pub(crate) available_runtimes: Vec<engine::AgentCli>,
    pub(crate) appearance: Appearance,

    /// What the person is typing in the composer.
    pub(crate) draft: String,
    /// Preserved drafts for chats the person is not looking at. Keyed by chat
    /// id, in memory only: a draft is not a document worth syncing to disk.
    pub(crate) drafts: HashMap<u32, String>,
    /// What they are typing in the sidebar search.
    pub(crate) search: String,
    pub(crate) searching: bool,
    pub(crate) panel_open: bool,
    pub(crate) sidebar_collapsed: bool,

    pub(crate) focus: FocusHandle,
    pub(crate) search_focus: FocusHandle,
    pub(crate) dialog_focus: FocusHandle,
    pub(crate) scroll: ScrollHandle,

    /// App-wide preferences. One bot's settings live in its own panel.
    pub(crate) settings: Settings,
    /// The open dialog, if any. Only one can be open at a time.
    pub(crate) dialog: Option<Dialog>,
    /// The bot panel's provider/model picker — same widget as the bot
    /// form's dropdown, with its own search/rail/scroll state.
    pub(crate) panel_picker: ProviderPicker,
    pub(crate) panel_picker_focus: FocusHandle,
    pub(crate) panel_picker_position: Option<Point<Pixels>>,
    pub(crate) permissions_open: bool,
    pub(crate) permissions_position: Option<Point<Pixels>>,
    /// Blink phase of the dialog form caret; any keystroke forces it on.
    pub(crate) caret_visible: bool,
    /// The sidebar's "+" menu: New bot / New team.
    pub(crate) create_menu_open: bool,
    pub(crate) context_menu: Option<(ContextTarget, Point<Pixels>)>,
    pub(crate) provider_expanded: HashSet<String>,
    /// Expanded long code blocks in the transcript, keyed by stable view id.
    pub(crate) expanded_code_blocks: HashSet<usize>,
    /// Files selected for the draft in each conversation.
    pub(crate) draft_attachments: HashMap<u32, Vec<String>>,

    /// The UI task consuming the in-flight run's events. Dropping it cancels
    /// the loop, so a stale run can never write into a newer one — no
    /// generation counter needed.
    pub(crate) run_task: Option<Task<()>>,
    pub(crate) run_bot: Option<u32>,
    /// Chat whose run is in flight. Runs are per chat, not global.
    pub(crate) run_chat: Option<u32>,
    /// The line the status strip shows while a run is live.
    pub(crate) status: Option<SharedString>,
    /// Question waiting for a typed answer from the composer.
    pub(crate) free_text_question: Option<(u64, usize)>,
    focus_pending: bool,
}

impl Shell {
    pub fn new(
        appearance: Appearance,
        engine: Arc<engine::Engine>,
        cx: &mut Context<Self>,
    ) -> Self {
        let storage = AppStorage::new(engine.data_dir());
        let (workspace, settings) = storage.load();
        let appearance = match settings.appearance {
            Some(AppearancePreference::Dark) => Appearance::Dark,
            Some(AppearancePreference::Light) => Appearance::Light,
            None => appearance,
        };
        let current_theme = Theme::of(cx);
        let font_sans = current_theme.font_sans.clone();
        let font_mono = current_theme.font_mono.clone();
        let mut theme = Theme::for_appearance(appearance);
        theme.font_sans = font_sans;
        theme.font_mono = font_mono;
        cx.set_global(theme);
        let available_runtimes = engine.available_runtimes().to_vec();
        // Ask each installed CLI which models it accepts, off the UI thread.
        // Until they answer, the picker shows the built-in catalog.
        let engine_for_models = Arc::clone(&engine);
        cx.spawn(async move |this, cx| {
            let discovered = cx
                .background_executor()
                .spawn(async move { engine_for_models.discover_models() })
                .await;
            if discovered > 0 {
                let _ = this.update(cx, |_, cx| cx.notify());
            }
        })
        .detach();
        // Caret blink: dialog fields and the composer share one phase, so
        // the loop runs for the shell's lifetime — two notifications a
        // second is cheaper than tracking which surface holds the caret.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(530))
                    .await;
                let _ = this.update(cx, |shell, cx| {
                    shell.caret_visible = !shell.caret_visible;
                    cx.notify();
                });
            }
        })
        .detach();
        Self {
            workspace,
            engine,
            storage,
            available_runtimes,
            appearance,
            draft: String::new(),
            drafts: HashMap::new(),
            search: String::new(),
            searching: false,
            panel_open: false,
            sidebar_collapsed: false,
            focus: cx.focus_handle(),
            search_focus: cx.focus_handle(),
            dialog_focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            settings,
            dialog: None,
            panel_picker: ProviderPicker::default(),
            panel_picker_focus: cx.focus_handle(),
            panel_picker_position: None,
            permissions_open: false,
            permissions_position: None,
            caret_visible: true,
            create_menu_open: false,
            context_menu: None,
            provider_expanded: HashSet::new(),
            expanded_code_blocks: HashSet::new(),
            draft_attachments: HashMap::new(),
            run_task: None,
            run_bot: None,
            run_chat: None,
            status: None,
            free_text_question: None,
            focus_pending: true,
        }
    }

    pub(crate) fn persist_workspace(&self) {
        if let Err(error) = self.storage.save(&self.workspace, &self.settings) {
            eprintln!("failed to save Aucteris desktop state: {error:#}");
        }
    }
    pub(crate) fn running(&self) -> bool {
        self.status.is_some() && self.run_bot.is_some()
    }

    /// Whether the chat on screen is the one the live run belongs to. A run
    /// keeps going while the person reads another chat; its Stop control and
    /// status strip only appear where the run actually writes.
    pub(crate) fn running_here(&self) -> bool {
        self.running() && self.run_chat == Some(self.workspace.chat().id)
    }

    /// Whether the composer's Send can steer the run in flight.
    pub(crate) fn can_steer(&self) -> bool {
        self.run_chat
            .filter(|chat| {
                self.workspace
                    .chats
                    .get(self.workspace.selected)
                    .map(|c| c.id)
                    == Some(*chat)
            })
            .map(|chat| self.engine.can_steer(chat))
            .unwrap_or(false)
    }

    // ---- selection -------------------------------------------------------

    pub(crate) fn select_chat(&mut self, index: usize, cx: &mut Context<Self>) {
        self.stash_draft();
        self.workspace.select(index);
        self.searching = false;
        self.search.clear();
        self.panel_picker.open = false;
        self.restore_draft();
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    /// Keep the text with the chat it was written for before switching.
    pub(crate) fn stash_draft(&mut self) {
        let chat_id = self.workspace.chat().id;
        if self.draft.is_empty() {
            self.drafts.remove(&chat_id);
        } else {
            self.drafts.insert(chat_id, self.draft.clone());
        }
    }

    /// Bring back the draft that belongs to the newly selected chat.
    pub(crate) fn restore_draft(&mut self) {
        let chat_id = self.workspace.chat().id;
        self.draft = self.drafts.remove(&chat_id).unwrap_or_default();
    }

    /// Forget the current draft once it has been sent.
    pub(crate) fn clear_draft(&mut self) {
        self.draft.clear();
        let chat_id = self.workspace.chat().id;
        self.drafts.remove(&chat_id);
        self.draft_attachments.remove(&chat_id);
    }

    /// Whether the transcript is close enough to the bottom to keep following
    /// new output. A reader who scrolled up owns the viewport.
    pub(crate) fn follows_bottom(&self) -> bool {
        let offset = self.scroll.offset();
        let max = self.scroll.max_offset();
        (max.y - offset.y) <= px(24.0)
    }

    /// Scroll only when the person was already reading the latest message.
    pub(crate) fn scroll_if_following(&self) {
        if self.follows_bottom() {
            self.scroll.scroll_to_bottom();
        }
    }

    /// The sidebar row for a bot, or a room, by title.
    pub(crate) fn chat_index_for_bot(&self, bot: u32) -> Option<usize> {
        self.workspace
            .chats
            .iter()
            .position(|chat| chat.bot == Some(bot))
    }
}

impl Focusable for Shell {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_pending {
            self.focus_pending = false;
            window.focus(&self.focus, cx);
        }
        let theme = Theme::of(cx).clone();

        let sidebar = self.view_sidebar(&theme, cx);
        let chat = self.view_chat(&theme, window, cx);
        let create_menu = self.view_create_menu(&theme, cx);
        let context_menu = self.view_context_menu(&theme, cx);
        let overlay = self.view_overlay(&theme, cx);
        let panel_popovers = self.view_panel_popovers(&theme, cx);
        let main_pane = div()
            .flex_1()
            .min_w_0()
            .mt(px(metrics::SPACE_2))
            .mb(px(metrics::SPACE_2))
            .ml(px(metrics::SPACE_2))
            .when(!self.panel_open, |this| this.mr(px(metrics::SPACE_2)))
            .flex()
            .overflow_hidden()
            .rounded(px(metrics::RADIUS_CARD))
            .border_1()
            .border_color(theme.edge)
            .bg(theme.bg)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_hidden()
                    .child(chat),
            );

        let right_panel = self.panel_open.then(|| {
            div()
                .w(px(metrics::PANEL_WIDTH))
                .mt(px(metrics::SPACE_2))
                .mr(px(metrics::SPACE_2))
                .mb(px(metrics::SPACE_2))
                .ml(px(metrics::SPACE_2))
                .flex_shrink_0()
                .flex()
                .overflow_hidden()
                .rounded(px(metrics::RADIUS_CARD))
                .border_1()
                .border_color(theme.edge)
                .bg(theme.panel)
                .child(self.view_bot_panel(&theme, cx))
        });

        div()
            .id("root")
            .relative()
            .size_full()
            .flex()
            .overflow_hidden()
            .bg(theme.bg)
            .font_family(theme.font_sans.clone())
            .text_size(px(typo::BODY))
            .text_color(theme.ink)
            .on_key_down(cx.listener(Self::on_root_key))
            .when(!self.sidebar_collapsed, |this| this.child(sidebar))
            .child(main_pane)
            .children(right_panel)
            .children(create_menu)
            .children(context_menu)
            .children(overlay)
            .children(panel_popovers)
            .into_any_element()
    }
}

/// What a permission button says.
fn permission_label(choice: &PermissionChoice) -> String {
    match choice {
        PermissionChoice::Allow => "Allow once".to_string(),
        PermissionChoice::AllowAlways { scope } => match scope {
            Scope::Chat => "Allow in this conversation".to_string(),
            Scope::Repo => "Allow in this project".to_string(),
            Scope::Pattern(pattern) => format!("Always for {pattern}"),
        },
        PermissionChoice::Deny => "Deny".to_string(),
    }
}

fn is_deny(choice: &PermissionChoice) -> bool {
    matches!(choice, PermissionChoice::Deny)
}
