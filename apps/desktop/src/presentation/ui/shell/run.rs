//! The run lifecycle: submit spawns the engine thread and consumes its
//! events on the UI task; stop/fail/finish settle the workspace state.

use std::sync::Arc;

use gpui::{Context, Window};

use aucteris_harness::komet_proto::PermissionKind;

use super::Shell;
use super::{is_deny, permission_label};
use crate::application::engine::{self, Action, EngineEvent};
use crate::domain::model::{self, Author, Message, Part, Presence};

impl Shell {
    // ---- the run ---------------------------------------------------------

    /// Send the composer's contents to whoever should answer.
    pub(crate) fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft_text = self.draft.trim().to_string();
        let attachments = self
            .draft_attachments
            .get(&self.workspace.chat().id)
            .cloned()
            .unwrap_or_default();
        if draft_text.is_empty() && attachments.is_empty() {
            return;
        }
        let user_text = if draft_text.is_empty() {
            "Please review the attached files.".to_string()
        } else {
            draft_text
        };
        let prompt = if attachments.is_empty() {
            user_text.clone()
        } else {
            format!(
                "{user_text}\n\nAttached files (open these original files):\n{}",
                attachments
                    .iter()
                    .map(|path| format!("- {path}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        let mut user_parts = vec![Part::Text(user_text)];
        user_parts.extend(
            attachments
                .iter()
                .cloned()
                .map(|path| Part::Attachment { path }),
        );
        // A question offering free text takes the next line typed here.
        if let Some((request, index)) = self.free_text_question.take() {
            self.clear_draft();
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
            if self.run_chat == Some(chat_id) {
                if self.engine.can_steer(chat_id) && self.engine.steer(chat_id, prompt.clone()) {
                    self.clear_draft();
                    self.workspace.chat_mut().messages.push(Message {
                        author: Author::You,
                        bot: None,
                        parts: user_parts.clone(),
                        time: model::timestamp_now(),
                        streaming: false,
                        duration_ms: None,
                        started_at: None,
                        activity_expanded: false,
                        stopped: false,
                    });
                    self.workspace.replace_last_preview();
                    cx.notify();
                }
            } else {
                // One run at a time: say why this chat cannot start another,
                // and keep the draft so it can be sent once the run stops.
                self.workspace.chat_mut().messages.push(Message {
                    author: Author::System,
                    bot: None,
                    parts: vec![Part::Text(
                        "Another chat is still running. Stop it before sending here.".to_string(),
                    )],
                    time: model::timestamp_now(),
                    streaming: false,
                    duration_ms: None,
                    started_at: None,
                    activity_expanded: false,
                    stopped: false,
                });
                self.workspace.replace_last_preview();
                self.persist_workspace();
                self.scroll.scroll_to_bottom();
                cx.notify();
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
                time: model::timestamp_now(),
                streaming: false,
                duration_ms: None,
                started_at: None,
                activity_expanded: false,
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
            bot_id: bot.id,
            runtime: bot.runtime.to_string(),
            folder: bot.folder.to_string(),
            allowed_folders: bot.allowed_folders.clone(),
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
                time: model::timestamp_now(),
                streaming: false,
                duration_ms: None,
                started_at: None,
                activity_expanded: false,
                stopped: false,
            });
            self.workspace.replace_last_preview();
            self.persist_workspace();
            cx.notify();
            return;
        }

        self.clear_draft();
        let chat = self.workspace.chat_mut();
        chat.messages.push(Message {
            author: Author::You,
            bot: None,
            parts: user_parts,
            time: model::timestamp_now(),
            streaming: false,
            duration_ms: None,
            started_at: None,
            activity_expanded: false,
            stopped: false,
        });
        chat.messages.push(Message {
            author: Author::Bot,
            bot: Some(bot_id),
            parts: Vec::new(),
            time: model::timestamp_now(),
            streaming: true,
            duration_ms: None,
            started_at: Some(std::time::Instant::now()),
            activity_expanded: false,
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
        // Any previous run's consumer is cancelled by dropping its task
        // handle; only one run is ever live, so this is just belt-and-braces.
        self.run_task = None;
        self.scroll.scroll_to_bottom();
        self.persist_workspace();

        let engine = Arc::clone(&self.engine);
        let chat_id = self.workspace.chat().id;
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        std::thread::spawn(move || engine.run_prompt(chat_id, run_config, prompt, sender));

        self.run_task = Some(cx.spawn_in(window, async move |this, cx| {
            while let Some(event) = receiver.recv().await {
                let applied = this.update(cx, |shell, cx| {
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
                            tool_call_id: _,
                        } => {
                            let (title, mut detail) = permission_copy(&kind, &summary);
                            let grant_folder = folder_to_grant(&shell.workspace, bot_id, &kind);
                            let mut buttons: Vec<_> = choices
                                .iter()
                                .map(|choice| (permission_label(choice), choice.clone()))
                                .collect();
                            if !choices.iter().any(is_deny) {
                                buttons.push((
                                    permission_label(
                                        &aucteris_harness::komet_proto::PermissionChoice::Deny,
                                    ),
                                    aucteris_harness::komet_proto::PermissionChoice::Deny,
                                ));
                            }
                            let grant_folder_choice = grant_folder.as_ref().map(|folder| {
                                detail.push_str(&format!(
                                    "\nApprove this folder for future runs: {folder}"
                                ));
                                let index = buttons.len();
                                buttons.push((
                                    "Allow and add folder".to_string(),
                                    aucteris_harness::komet_proto::PermissionChoice::Allow,
                                ));
                                index
                            });
                            shell.waiting(Part::Approval {
                                kicker: "Permission".to_string(),
                                title,
                                detail,
                                request: Some(id),
                                choices: buttons,
                                grant_folder_choice,
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
        }));
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
        self.refresh_run_preview();
        if self.running_here() {
            self.scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    /// Refresh the run chat's sidebar preview even when another chat is open.
    fn refresh_run_preview(&mut self) {
        if let Some(chat_id) = self.run_chat {
            self.workspace.replace_preview_for(chat_id);
        }
    }

    /// The bot reply being streamed. Last message is not necessarily it: a
    /// steer appends the user's own line while the answer is still open.
    /// The run's chat is addressed by id, not by selection.
    fn reply_mut(&mut self) -> Option<&mut Message> {
        let chat_id = self.run_chat?;
        self.workspace
            .chats
            .iter_mut()
            .find(|chat| chat.id == chat_id)?
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
                        failed: false,
                        result: None,
                        duration_ms: None,
                        started_at: Some(std::time::Instant::now()),
                    });
                }
            }
            Action::ToolProgress { text, append } => {
                if let Some(message) = self.reply_mut()
                    && let Some(Part::Tool { result, .. }) = message
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
                old_text,
                new_text,
            } => {
                let snapshot_fits = old_text.as_ref().is_none_or(|text| text.len() <= 1_000_000)
                    && new_text.len() <= 1_000_000;
                if let Some(message) = self.reply_mut() {
                    message.push_part(Part::Files {
                        path,
                        added,
                        removed,
                        old_text: snapshot_fits.then_some(old_text).flatten(),
                        new_text: snapshot_fits.then_some(new_text),
                        diff_expanded: false,
                        undo_error: None,
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
            Action::Usage {
                input,
                output,
                context_limit,
            } => {
                let used = input.saturating_add(output);
                self.workspace.tokens_used = self.workspace.tokens_used.saturating_add(used);
                if let Some(limit) = context_limit {
                    self.workspace.tokens_cap = limit;
                }
                if let Some(bot) = bot_id.and_then(|id| self.workspace.bot_mut(id)) {
                    bot.tokens = bot.tokens.saturating_add(used);
                    if let Some(limit) = context_limit {
                        bot.context_limit = Some(limit);
                    }
                }
            }
            Action::ContextWindow { limit } => {
                self.workspace.tokens_cap = limit;
                if let Some(bot) = bot_id.and_then(|id| self.workspace.bot_mut(id)) {
                    bot.context_limit = Some(limit);
                }
            }
        }
        self.refresh_run_preview();
        if self.running_here() {
            self.scroll_if_following();
        }
        cx.notify();
    }

    fn finish_run(&mut self, cx: &mut Context<Self>) {
        if let Some(message) = self.reply_mut() {
            message.finish_stream();
            message.finish_last_tool(None);
        }
        if let Some(bot_id) = self.run_bot
            && let Some(bot) = self.workspace.bot_mut(bot_id)
        {
            bot.presence = Presence::Resting;
            bot.activity = "Completed".into();
        }
        self.refresh_run_preview();
        self.run_bot = None;
        self.run_chat = None;
        self.free_text_question = None;
        self.close_cards();
        self.status = None;
        self.persist_workspace();
        cx.notify();
    }

    fn fail_run(&mut self, error: String, cx: &mut Context<Self>) {
        if let Some(message) = self.reply_mut() {
            message.finish_stream();
            if !error.trim().is_empty() {
                if !message.parts.is_empty() {
                    message.push_text("\n\n");
                }
                message.push_text(&format!("Run failed: {error}"));
            }
            message.fail_last_tool(Some("Run failed".into()));
        }
        if let Some(bot_id) = self.run_bot.take()
            && let Some(bot) = self.workspace.bot_mut(bot_id)
        {
            bot.presence = Presence::Resting;
            bot.activity = "Run failed".into();
        }
        self.refresh_run_preview();
        self.run_chat = None;
        self.free_text_question = None;
        self.close_cards();
        self.status = None;
        self.persist_workspace();
        cx.notify();
    }

    /// Stop the run in flight. The answer keeps whatever arrived, marked as
    /// stopped, so nothing already useful disappears.
    pub(crate) fn stop_run(&mut self, cx: &mut Context<Self>) {
        self.stop_run_without_persist(cx);
        self.persist_workspace();
    }

    pub(crate) fn stop_run_without_persist(&mut self, cx: &mut Context<Self>) {
        // Cancelling the consumer drops the event receiver, so nothing the
        // engine still emits can reach the workspace.
        self.run_task = None;
        if let Some(chat_id) = self.run_chat {
            self.engine.stop(chat_id);
        }
        if let Some(message) = self.reply_mut()
            && message.streaming
        {
            message.finish_stream();
            message.stopped = true;
            message.finish_last_tool(Some("Stopped".into()));
        }
        if let Some(bot_id) = self.run_bot.take()
            && let Some(bot) = self.workspace.bot_mut(bot_id)
        {
            bot.presence = Presence::Resting;
            bot.activity = "Stopped on request".into();
        }
        self.refresh_run_preview();
        self.run_chat = None;
        self.free_text_question = None;
        self.close_cards();
        self.status = None;
        cx.notify();
    }
}

fn permission_copy(kind: &PermissionKind, summary: &str) -> (String, String) {
    let opaque = |value: &str| {
        value.split_whitespace().any(|word| {
            let token =
                word.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_');
            token.starts_with("call-")
                || token.starts_with("toolu_")
                || (token.len() >= 32 && token.chars().filter(|ch| *ch == '-').count() >= 4)
        })
    };
    match kind {
        PermissionKind::Tool { name } => {
            let friendly = match name.to_ascii_lowercase().as_str() {
                "read" | "read_file" | "readfile" => Some("Read a file"),
                "write" | "write_file" | "edit" | "apply_patch" => Some("Update a file"),
                "bash" | "exec" | "shell" => Some("Run a command"),
                "web_search" | "websearch" | "search_web" => Some("Search the web"),
                _ => None,
            };
            let title = if !opaque(summary) && !summary.trim().is_empty() {
                summary.trim().to_string()
            } else if let Some(label) = friendly {
                label.to_string()
            } else if !opaque(name) && !name.trim().is_empty() {
                format!("Use {}", name.trim())
            } else {
                "Use a connected tool".to_string()
            };
            let detail = if !opaque(name) && !name.trim().is_empty() && friendly.is_none() {
                format!("{} needs permission to continue.", name.trim())
            } else {
                "A connected tool is requesting permission.".to_string()
            };
            (title, detail)
        }
        PermissionKind::Command { cmdline } => (
            "Run a command".to_string(),
            if cmdline.trim().is_empty() {
                "The agent wants to run a command.".to_string()
            } else {
                cmdline.clone()
            },
        ),
        PermissionKind::FileWrite { path } => {
            let name = std::path::Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .unwrap_or("file");
            ("Update a file".to_string(), format!("File: {name}"))
        }
    }
}

fn folder_to_grant(
    workspace: &model::Workspace,
    bot_id: u32,
    kind: &PermissionKind,
) -> Option<String> {
    let bot = workspace.bot(bot_id)?;
    if !engine::Engine::supports_scoped_folders(&bot.runtime)
        || bot.sandbox != aucteris_harness::komet_proto::SandboxLevel::WorkspaceWrite
    {
        return None;
    }
    let PermissionKind::FileWrite { path } = kind else {
        return None;
    };
    let target = std::path::Path::new(path);
    let parent = target.parent()?.canonicalize().ok()?;
    let private = crate::infrastructure::storage::expand_home(&bot.folder)
        .canonicalize()
        .unwrap_or_else(|_| crate::infrastructure::storage::expand_home(&bot.folder));
    if parent.starts_with(private) {
        return None;
    }
    let value = parent.to_string_lossy().to_string();
    if bot.allowed_folders.iter().any(|root| root == &value) {
        return None;
    }
    Some(value)
}
