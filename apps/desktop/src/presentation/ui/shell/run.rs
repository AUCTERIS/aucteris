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
            if self.run_chat == Some(chat_id)
                && self.engine.can_steer(chat_id)
                && self.engine.steer(chat_id, prompt.clone())
            {
                self.draft.clear();
                self.workspace.chat_mut().messages.push(Message {
                    author: Author::You,
                    bot: None,
                    parts: vec![Part::Text(prompt)],
                    time: model::timestamp_now(),
                    streaming: false,
                    stopped: false,
                });
                self.workspace.replace_last_preview();
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
                time: model::timestamp_now(),
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
            time: model::timestamp_now(),
            streaming: false,
            stopped: false,
        });
        chat.messages.push(Message {
            author: Author::Bot,
            bot: Some(bot_id),
            parts: Vec::new(),
            time: model::timestamp_now(),
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
        self.workspace.replace_last_preview();
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    fn finish_run(&mut self, cx: &mut Context<Self>) {
        if let Some(message) = self.reply_mut() {
            message.streaming = false;
            message.finish_last_tool(None);
        }
        if let Some(bot_id) = self.run_bot
            && let Some(bot) = self.workspace.bot_mut(bot_id)
        {
            bot.presence = Presence::Resting;
            bot.activity = "Completed".into();
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
        if let Some(bot_id) = self.run_bot.take()
            && let Some(bot) = self.workspace.bot_mut(bot_id)
        {
            bot.presence = Presence::Resting;
            bot.activity = "Run failed".into();
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
        // Cancelling the consumer drops the event receiver, so nothing the
        // engine still emits can reach the workspace.
        self.run_task = None;
        if let Some(chat_id) = self.run_chat {
            self.engine.stop(chat_id);
        }
        if let Some(message) = self.reply_mut()
            && message.streaming
        {
            message.streaming = false;
            message.stopped = true;
            message.finish_last_tool(Some("Stopped".into()));
        }
        if let Some(bot_id) = self.run_bot.take()
            && let Some(bot) = self.workspace.bot_mut(bot_id)
        {
            bot.presence = Presence::Resting;
            bot.activity = "Stopped on request".into();
        }
        self.run_chat = None;
        self.free_text_question = None;
        self.close_cards();
        self.status = None;
        self.workspace.replace_last_preview();
        self.persist_workspace();
        cx.notify();
    }
}
