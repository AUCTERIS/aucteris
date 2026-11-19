//! The app shell: sidebar, chat, bot panel — the three places everything lives.
//!
//! The shell owns the workspace, the composer draft and the run that is in
//! flight. Views are plain methods on it (sidebar.rs, chat.rs, composer.rs,
//! bot_panel.rs) so a click can mutate the one source of truth without an
//! event bus.

use gpui::{
    App, Context, FocusHandle, Focusable, KeyDownEvent, Render, ScrollHandle, SharedString, Window,
    div, prelude::*, px,
};

use crate::application::engine::{self, Action, EngineEvent};
use crate::domain::model::{self, Author, Message, Part, Presence, Settings, Workspace};
use crate::infrastructure::storage::AppStorage;
use crate::presentation::theme::{Appearance, Theme, metrics, text as typo};
use crate::presentation::ui::dialog::{Dialog, ProviderPicker};
use komet_proto::{PermissionChoice, PermissionKind, Scope};
use std::sync::Arc;
use std::time::Duration;

pub struct Shell {
    pub(crate) workspace: Workspace,
    pub(crate) engine: Arc<engine::Engine>,
    storage: AppStorage,
    pub(crate) available_runtimes: Vec<engine::AgentCli>,
    pub(crate) appearance: Appearance,

    /// What the person is typing in the composer.
    pub(crate) draft: String,
    /// What they are typing in the sidebar search.
    pub(crate) search: String,
    pub(crate) searching: bool,
    pub(crate) panel_open: bool,

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
    /// Blink phase of the dialog form caret; any keystroke forces it on.
    pub(crate) caret_visible: bool,
    /// The sidebar's "+" menu: New bot / New team.
    pub(crate) create_menu_open: bool,

    /// Bumped to abandon a run in flight. The loop compares it after every
    /// sleep, so Stop needs no cancellation plumbing.
    generation: u64,
    saw_assistant_text: bool,
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
        let workspace = storage.load();
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
            search: String::new(),
            searching: false,
            panel_open: false,
            focus: cx.focus_handle(),
            search_focus: cx.focus_handle(),
            dialog_focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            settings: Settings::default(),
            dialog: None,
            panel_picker: ProviderPicker::default(),
            panel_picker_focus: cx.focus_handle(),
            caret_visible: true,
            create_menu_open: false,
            generation: 0,
            saw_assistant_text: false,
            run_bot: None,
            run_chat: None,
            status: None,
            free_text_question: None,
            focus_pending: true,
        }
    }

    pub(crate) fn persist_workspace(&self) {
        if let Err(error) = self.storage.save(&self.workspace) {
            eprintln!("failed to save Aucteris desktop state: {error:#}");
        }
    }

    /// Register a finished team: title and members chosen in the dialog.
    pub(crate) fn create_team(&mut self, title: String, members: Vec<u32>, cx: &mut Context<Self>) {
        let id = self
            .workspace
            .chats
            .iter()
            .map(|chat| chat.id)
            .max()
            .unwrap_or(0)
            + 1;
        self.workspace.chats.push(model::Chat {
            id,
            title: title.into(),
            bot: None,
            members: members.clone(),
            messages: Vec::new(),
            preview: if members.is_empty() {
                "Add bots to start this team".into()
            } else {
                format!("{} bots in this team", members.len()).into()
            },
            time: "now".into(),
            session_id: None,
        });
        self.workspace.selected = self.workspace.chats.len() - 1;
        self.panel_open = true;
        self.scroll.scroll_to_bottom();
        self.persist_workspace();
        cx.notify();
    }

    pub(crate) fn add_bot_to_room(&mut self, room_id: u32, bot_id: u32, cx: &mut Context<Self>) {
        let Some(room) = self
            .workspace
            .chats
            .iter_mut()
            .find(|chat| chat.id == room_id && chat.is_room())
        else {
            return;
        };
        if !room.members.contains(&bot_id) {
            room.members.push(bot_id);
            room.preview = format!("{} bots in this team", room.members.len()).into();
            self.persist_workspace();
            cx.notify();
        }
    }

    pub(crate) fn remove_bot_from_room(
        &mut self,
        room_id: u32,
        bot_id: u32,
        cx: &mut Context<Self>,
    ) {
        let Some(room) = self
            .workspace
            .chats
            .iter_mut()
            .find(|chat| chat.id == room_id && chat.is_room())
        else {
            return;
        };
        room.members.retain(|id| *id != bot_id);
        room.preview = if room.members.is_empty() {
            "Add bots to start this team".into()
        } else {
            format!("{} bots in this team", room.members.len()).into()
        };
        self.persist_workspace();
        cx.notify();
    }

    pub(crate) fn running(&self) -> bool {
        self.status.is_some() && self.run_bot.is_some()
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
        if self.running() {
            return;
        }
        self.workspace.select(index);
        self.searching = false;
        self.search.clear();
        self.panel_picker.open = false;
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    /// The sidebar row for a bot, or a room, by title.
    pub(crate) fn chat_index_for_bot(&self, bot: u32) -> Option<usize> {
        self.workspace
            .chats
            .iter()
            .position(|chat| chat.bot == Some(bot))
    }

    // ---- the run ---------------------------------------------------------

    /// Send the composer's contents to whoever should answer.
    pub(crate) fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prompt = self.draft.trim().to_string();
        if prompt.is_empty() {
            return;
        }
        // A question offering free text takes the next line typed here.
        if let Some((request, index)) = self.free_text_question.take() {
            self.draft.clear();
            self.engine
                .answer_question(request, index, Vec::new(), Some(prompt.clone()));
            self.mark_question(request, index, prompt);
            cx.notify();
            return;
        }
        // A live run that supports steering takes the prompt as an injection
        // into the same turn instead of starting a new one.
        if self.running() {
            let chat_id = self.workspace.chat().id;
            if self.run_chat == Some(chat_id) && self.engine.can_steer(chat_id) {
                if self.engine.steer(chat_id, prompt.clone()) {
                    self.draft.clear();
                    self.workspace.chat_mut().messages.push(Message {
                        author: Author::You,
                        bot: None,
                        parts: vec![Part::Text(prompt)],
                        time: "now".into(),
                        streaming: false,
                        stopped: false,
                    });
                    self.workspace.replace_last_preview();
                    cx.notify();
                }
            }
            return;
        }
        let Some(bot_id) = self.workspace.responder() else {
            self.workspace.chat_mut().messages.push(Message {
                author: Author::System,
                bot: None,
                parts: vec![Part::Text(
                    "No bot in this room can answer — add one first.".to_string(),
                )],
                time: "now".into(),
                streaming: false,
                stopped: false,
            });
            self.workspace.replace_last_preview();
            self.persist_workspace();
            cx.notify();
            return;
        };
        let Some(bot) = self.workspace.bot(bot_id) else {
            return;
        };
        let run_config = engine::BotRunConfig {
            id: bot.id,
            name: bot.name.to_string(),
            runtime: bot.runtime.to_string(),
            folder: bot.folder.to_string(),
            instructions: bot.instructions.to_string(),
            resume: self.workspace.chat().session_id.clone(),
            sandbox: bot.sandbox,
        };
        let bot_id = bot.id;
        if !engine::Engine::runtime_usable(&run_config.runtime) {
            let label = self.engine.runtime_label(&run_config.runtime);
            self.workspace.chat_mut().messages.push(Message {
                author: Author::Bot,
                bot: Some(bot_id),
                parts: vec![Part::Text(format!(
                    "Run failed: {label} is not installed or not on PATH."
                ))],
                time: "now".into(),
                streaming: false,
                stopped: false,
            });
            self.workspace.replace_last_preview();
            self.persist_workspace();
            cx.notify();
            return;
        }

        self.draft.clear();
        let chat = self.workspace.chat_mut();
        chat.messages.push(Message {
            author: Author::You,
            bot: None,
            parts: vec![Part::Text(prompt.clone())],
            time: "now".into(),
            streaming: false,
            stopped: false,
        });
        chat.messages.push(Message {
            author: Author::Bot,
            bot: Some(bot_id),
            parts: Vec::new(),
            time: "now".into(),
            streaming: true,
            stopped: false,
        });
        if let Some(bot) = self.workspace.bot_mut(bot_id) {
            bot.presence = Presence::Working;
            bot.activity = "Starting…".into();
        }
        self.workspace.replace_last_preview();
        self.status = Some("Starting…".into());
        self.run_bot = Some(bot_id);
        self.run_chat = Some(self.workspace.chat().id);
        self.generation += 1;
        self.saw_assistant_text = false;
        let generation = self.generation;
        self.scroll.scroll_to_bottom();
        self.persist_workspace();

        let engine = Arc::clone(&self.engine);
        let chat_id = self.workspace.chat().id;
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        std::thread::spawn(move || engine.run_prompt(chat_id, run_config, prompt, sender));

        cx.spawn_in(window, async move |this, cx| {
            while let Some(event) = receiver.recv().await {
                let applied = this.update(cx, |shell, cx| {
                    if shell.generation != generation {
                        return false;
                    }
                    match event {
                        EngineEvent::Action(action) => shell.apply(action, cx),
                        EngineEvent::SessionId(session_id) => {
                            shell.workspace.chat_mut().session_id = Some(session_id);
                            shell.persist_workspace();
                        }
                        EngineEvent::Permission {
                            id,
                            kind,
                            summary,
                            choices,
                            tool_call_id,
                        } => {
                            let mut detail = match &kind {
                                PermissionKind::Tool { name } => format!("tool · {name}"),
                                PermissionKind::Command { cmdline } => cmdline.clone(),
                                PermissionKind::FileWrite { path } => format!("write · {path}"),
                            };
                            if let Some(call) = tool_call_id {
                                detail.push_str(&format!(" · {call}"));
                            }
                            let buttons = choices
                                .iter()
                                .map(|choice| (permission_label(choice), !is_deny(choice)))
                                .collect();
                            shell.waiting(Part::Approval {
                                kicker: "Permission".to_string(),
                                title: summary,
                                detail,
                                request: Some(id),
                                choices: buttons,
                                resolved: None,
                            });
                            shell.set_waiting("Waiting for your approval…", cx);
                        }
                        EngineEvent::Questions { id, questions } => {
                            let count = questions.len();
                            for (index, question) in questions.into_iter().enumerate() {
                                let header = if question.header.trim().is_empty() {
                                    format!("Question {}", index + 1)
                                } else {
                                    question.header
                                };
                                shell.waiting(Part::Question {
                                    request: id,
                                    index,
                                    header,
                                    question: question.question,
                                    options: question.options,
                                    multi_select: question.multi_select,
                                    allow_free_text: question.allow_free_text,
                                    secret: question.secret,
                                    selected: Vec::new(),
                                    resolved: None,
                                });
                            }
                            shell.set_waiting(
                                &format!("Waiting for your answer · {count} question(s)"),
                                cx,
                            );
                        }
                        EngineEvent::Finished(Ok(())) => {
                            shell.finish_run(cx);
                            return false;
                        }
                        EngineEvent::Finished(Err(error)) => {
                            shell.fail_run(error, cx);
                            return false;
                        }
                    }
                    true
                });
                if !matches!(applied, Ok(true)) {
                    return;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// Attach a card (approval, question) to the reply being streamed.
    pub(crate) fn waiting(&mut self, part: Part) {
        if let Some(message) = self.reply_mut() {
            message.push_part(part);
        }
    }

    /// The agent is blocked on the person: say so everywhere it shows.
    pub(crate) fn set_waiting(&mut self, status: &str, cx: &mut Context<Self>) {
        self.status = Some(status.to_string().into());
        if let Some(bot) = self.run_bot.and_then(|id| self.workspace.bot_mut(id)) {
            bot.presence = Presence::Waiting;
            bot.activity = status.to_string().into();
        }
        self.workspace.replace_last_preview();
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    /// The bot reply being streamed. Last message is not necessarily it: a
    /// steer appends the user's own line while the answer is still open.
    fn reply_mut(&mut self) -> Option<&mut Message> {
        self.workspace
            .chat_mut()
            .messages
            .iter_mut()
            .rev()
            .find(|message| message.streaming)
    }

    fn apply(&mut self, action: Action, cx: &mut Context<Self>) {
        let bot_id = self.run_bot;
        match action {
            Action::Intent(activity) => {
                self.status = Some(activity.clone().into());
                if let Some(bot) = bot_id.and_then(|id| self.workspace.bot_mut(id)) {
                    bot.activity = activity.into();
                }
            }
            Action::Say(text) => {
                self.saw_assistant_text = true;
                if let Some(message) = self.reply_mut() {
                    message.push_text(&text);
                }
            }
            Action::Thought(text) => {
                if let Some(message) = self.reply_mut() {
                    message.push_thought(&text);
                }
            }
            Action::Tool {
                icon,
                label,
                activity,
                detail,
            } => {
                self.status = Some(activity.clone().into());
                if let Some(bot) = bot_id.and_then(|id| self.workspace.bot_mut(id)) {
                    bot.activity = activity.into();
                }
                if let Some(message) = self.reply_mut() {
                    message.push_part(Part::Tool {
                        icon,
                        label,
                        detail,
                        done: false,
                        result: None,
                    });
                }
            }
            Action::ToolProgress { text, append } => {
                if let Some(message) = self.reply_mut() {
                    if let Some(Part::Tool { result, .. }) = message
                        .parts
                        .iter_mut()
                        .rev()
                        .find(|part| matches!(part, Part::Tool { done: false, .. }))
                    {
                        match result {
                            Some(slot) if append => slot.push_str(&text),
                            slot => *slot = Some(text),
                        }
                    }
                }
                cx.notify();
                return;
            }
            Action::Phase(phase) => {
                self.status = Some(phase.into());
                if let Some(bot) = bot_id.and_then(|id| self.workspace.bot_mut(id)) {
                    bot.activity = phase.into();
                }
                cx.notify();
                return;
            }
            Action::Files {
                path,
                added,
                removed,
            } => {
                if let Some(message) = self.reply_mut() {
                    message.push_part(Part::Files {
                        path,
                        added,
                        removed,
                        undone: false,
                    });
                }
            }
            Action::Settle(result) => {
                if let Some(message) = self.reply_mut() {
                    message.finish_last_tool(Some(result));
                }
            }
            Action::Error(text) => {
                if let Some(message) = self.reply_mut() {
                    message.push_text(&format!("\n\n{text}"));
                }
            }
            Action::Usage { input, output } => {
                let used = input.saturating_add(output);
                self.workspace.tokens_used = self.workspace.tokens_used.saturating_add(used);
                if let Some(bot) = bot_id.and_then(|id| self.workspace.bot_mut(id)) {
                    bot.tokens = bot.tokens.saturating_add(used);
                }
            }
        }
        self.workspace.replace_last_preview();
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    fn finish_run(&mut self, cx: &mut Context<Self>) {
        if let Some(message) = self.reply_mut() {
            message.streaming = false;
            message.finish_last_tool(None);
        }
        if let Some(bot_id) = self.run_bot {
            if let Some(bot) = self.workspace.bot_mut(bot_id) {
                bot.presence = Presence::Resting;
                bot.activity = "Completed".into();
            }
        }
        self.run_bot = None;
        self.run_chat = None;
        self.free_text_question = None;
        self.close_cards();
        self.status = None;
        self.workspace.replace_last_preview();
        self.persist_workspace();
        cx.notify();
    }

    fn fail_run(&mut self, error: String, cx: &mut Context<Self>) {
        if let Some(message) = self.reply_mut() {
            message.streaming = false;
            if !error.trim().is_empty() {
                if !message.parts.is_empty() {
                    message.push_text("\n\n");
                }
                message.push_text(&format!("Run failed: {error}"));
            }
            message.finish_last_tool(Some("Run failed".into()));
        }
        if let Some(bot_id) = self.run_bot.take() {
            if let Some(bot) = self.workspace.bot_mut(bot_id) {
                bot.presence = Presence::Resting;
                bot.activity = "Run failed".into();
            }
        }
        self.run_chat = None;
        self.free_text_question = None;
        self.close_cards();
        self.status = None;
        self.workspace.replace_last_preview();
        self.persist_workspace();
        cx.notify();
    }

    /// Stop the run in flight. The answer keeps whatever arrived, marked as
    /// stopped, so nothing already useful disappears.
    pub(crate) fn stop_run(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        if let Some(chat_id) = self.run_chat {
            self.engine.stop(chat_id);
        }
        if let Some(message) = self.reply_mut() {
            if message.streaming {
                message.streaming = false;
                message.stopped = true;
                message.finish_last_tool(Some("Stopped".into()));
            }
        }
        if let Some(bot_id) = self.run_bot.take() {
            if let Some(bot) = self.workspace.bot_mut(bot_id) {
                bot.presence = Presence::Resting;
                bot.activity = "Stopped on request".into();
            }
        }
        self.run_chat = None;
        self.free_text_question = None;
        self.close_cards();
        self.status = None;
        self.workspace.replace_last_preview();
        self.persist_workspace();
        cx.notify();
    }

    // ---- bot panel actions ----------------------------------------------

    pub(crate) fn cycle_sandbox(&mut self, bot_id: u32, cx: &mut Context<Self>) {
        if let Some(bot) = self.workspace.bot_mut(bot_id) {
            bot.sandbox = model::next_sandbox(bot.sandbox);
        }
        self.persist_workspace();
        cx.notify();
    }

    pub(crate) fn reset_usage(&mut self, bot_id: u32, cx: &mut Context<Self>) {
        if let Some(bot) = self.workspace.bot_mut(bot_id) {
            bot.tokens = 0;
        }
        self.persist_workspace();
        cx.notify();
    }

    /// Take back a file edit or an out-of-chat change, in place.
    pub(crate) fn undo_part(
        &mut self,
        chat_index: usize,
        message_index: usize,
        part_index: usize,
        cx: &mut Context<Self>,
    ) {
        if let Some(chat) = self.workspace.chats.get_mut(chat_index) {
            if let Some(message) = chat.messages.get_mut(message_index) {
                match message.parts.get_mut(part_index) {
                    Some(Part::Files { undone, .. }) | Some(Part::Note { undone, .. }) => {
                        *undone = true;
                    }
                    _ => {}
                }
            }
        }
        cx.notify();
    }

    // ---- chrome ----------------------------------------------------------

    pub(crate) fn toggle_panel(&mut self, cx: &mut Context<Self>) {
        self.panel_open = !self.panel_open;
        self.panel_picker.open = false;
        cx.notify();
    }

    pub(crate) fn toggle_appearance(&mut self, cx: &mut Context<Self>) {
        self.appearance = self.appearance.toggled();
        Theme::install(self.appearance, cx);
        cx.notify();
    }

    // ---- keys ------------------------------------------------------------

    fn shortcuts(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // A dialog owns the keyboard while it is open.
        if self.dialog.is_some() {
            return;
        }
        let key = event.keystroke.key.as_str();
        match key {
            "j" => self.toggle_appearance(cx),
            "i" => self.toggle_panel(cx),
            "n" => self.open_new_bot(window, cx),
            "f" => {
                self.searching = true;
                window.focus(&self.search_focus, cx);
                cx.notify();
            }
            _ => {}
        }
    }

    /// The composer's key handler: type, delete, send.
    pub(crate) fn on_composer_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        self.caret_visible = true;
        // Ctrl+Enter is the send chord when Enter is a line break.
        if modifiers.control || modifiers.platform {
            if event.keystroke.key == "enter" && !self.settings.send_on_enter {
                self.submit(window, cx);
            }
            return;
        }
        match event.keystroke.key.as_str() {
            // Shift+Enter always keeps the line break.
            "enter" if modifiers.shift => {
                self.draft.push('\n');
                cx.notify();
            }
            "enter" if self.settings.send_on_enter => self.submit(window, cx),
            "enter" => {
                self.draft.push('\n');
                cx.notify();
            }
            "backspace" => {
                self.draft.pop();
                cx.notify();
            }
            "escape" => {
                // An open panel picker eats Escape before the draft does.
                if self.panel_picker.open {
                    self.panel_picker.open = false;
                } else {
                    self.draft.clear();
                }
                cx.notify();
            }
            key => {
                let typed = event
                    .keystroke
                    .key_char
                    .clone()
                    .or_else(|| (key == "space").then(|| " ".to_string()));
                if let Some(typed) = typed.filter(|text| !text.is_empty()) {
                    self.draft.push_str(&typed);
                    cx.notify();
                }
            }
        }
    }

    /// The sidebar search's key handler.
    pub(crate) fn on_search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.control || modifiers.platform {
            return;
        }
        match event.keystroke.key.as_str() {
            "enter" => {
                self.searching = false;
                window.focus(&self.focus, cx);
                cx.notify();
            }
            "escape" => {
                self.search.clear();
                self.searching = false;
                window.focus(&self.focus, cx);
                cx.notify();
            }
            "backspace" => {
                self.search.pop();
                cx.notify();
            }
            key => {
                let typed = event
                    .keystroke
                    .key_char
                    .clone()
                    .or_else(|| (key == "space").then(|| " ".to_string()));
                if let Some(typed) = typed.filter(|text| !text.is_empty()) {
                    self.search.push_str(&typed);
                    cx.notify();
                }
            }
        }
    }

    /// Shortcuts work from anywhere, including an empty window.
    fn on_root_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.create_menu_open && event.keystroke.key == "escape" {
            self.create_menu_open = false;
            cx.notify();
            return;
        }
        if event.keystroke.modifiers.control || event.keystroke.modifiers.platform {
            self.shortcuts(event, window, cx);
        }
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
        let overlay = self.view_overlay(&theme, cx);
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
            .child(sidebar)
            .child(main_pane)
            .children(right_panel)
            .children(create_menu)
            .children(overlay)
            .into_any_element()
    }
}

/// What a permission button says.
fn permission_label(choice: &PermissionChoice) -> String {
    match choice {
        PermissionChoice::Allow => "Allow".to_string(),
        PermissionChoice::AllowAlways { scope } => match scope {
            Scope::Chat => "Always in this chat".to_string(),
            Scope::Repo => "Always in this repo".to_string(),
            Scope::Pattern(pattern) => format!("Always: {pattern}"),
        },
        PermissionChoice::Deny => "Deny".to_string(),
    }
}

fn is_deny(choice: &PermissionChoice) -> bool {
    matches!(choice, PermissionChoice::Deny)
}
