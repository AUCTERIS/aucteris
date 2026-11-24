//! Single entry point for user intents that mutate shell state.
//!
//! Views never touch workspace state directly: a click or key produces a
//! [`UiAction`], `dispatch` routes it to the handler and performs the one
//! controlled disk write for that mutation. Engine-driven mutations (run
//! events, finish/fail) keep their own persistence inside the run pipeline.

use gpui::Context;

use crate::domain::model::{AppearancePreference, Mode};
use crate::presentation::ui::dialog::{BotFormSubmission, PickerHost};
use crate::presentation::ui::shell::{ContextTarget, Shell};
use aucteris_harness::komet_proto::SandboxLevel;

/// Every user intent that changes what the workspace holds or shows.
#[derive(Debug)]
pub enum UiAction {
    SelectChat(usize),
    StopRun,
    TogglePanel,
    ToggleAppearance,
    SetAppearance(AppearancePreference),
    ResetUsage(u32),
    UndoPart {
        chat: usize,
        message: usize,
        part: usize,
    },
    OpenFile {
        chat: usize,
        path: String,
    },
    ToggleFileDiff {
        chat: usize,
        message: usize,
        part: usize,
    },
    ToggleThought {
        chat: usize,
        message: usize,
        part: usize,
    },
    ToggleActivity {
        chat: usize,
        message: usize,
    },
    AnswerApproval {
        chat: usize,
        message: usize,
        part: usize,
        choice: usize,
    },
    AnswerQuestionChoice {
        chat: usize,
        message: usize,
        part: usize,
        option: usize,
    },
    AnswerQuestionMulti {
        chat: usize,
        message: usize,
        part: usize,
    },
    SetMode(Mode),
    ToggleSendOnEnter,
    ToggleRuntimeVisibility(String),
    SetDefaultRuntime(String),
    TogglePinned(ContextTarget),
    DeleteContextTarget(ContextTarget),
    AddAllowedFolder {
        bot: u32,
        path: String,
    },
    RemoveAllowedFolder {
        bot: u32,
        index: usize,
    },
    SetBotSandbox {
        bot: u32,
        sandbox: SandboxLevel,
    },
    SaveBot(BotFormSubmission),
    SaveTeam {
        target: Option<u32>,
        title: String,
        members: Vec<u32>,
    },
    AddBotToRoom {
        room: u32,
        bot: u32,
    },
    RemoveBotFromRoom {
        room: u32,
        bot: u32,
    },
    ApplyProviderChoice {
        host: PickerHost,
        choice: String,
    },
}

impl UiAction {
    /// Whether handling this action changes persisted state. Chrome-only
    /// toggles skip the disk write.
    fn persists(&self) -> bool {
        !matches!(self, Self::TogglePanel | Self::StopRun)
    }
}

impl Shell {
    /// Route one user intent: mutate, then write disk exactly once.
    pub(crate) fn dispatch(&mut self, action: UiAction, cx: &mut Context<Self>) {
        let persists = action.persists();
        match action {
            UiAction::SelectChat(index) => self.select_chat(index, cx),
            UiAction::StopRun => self.stop_run(cx),
            UiAction::TogglePanel => self.toggle_panel(cx),
            UiAction::ToggleAppearance => self.toggle_appearance(cx),
            UiAction::SetAppearance(preference) => self.set_appearance(preference, cx),
            UiAction::ResetUsage(bot) => self.reset_usage(bot, cx),
            UiAction::UndoPart {
                chat,
                message,
                part,
            } => self.undo_part(chat, message, part, cx),
            UiAction::OpenFile { chat, path } => self.open_file(chat, &path, cx),
            UiAction::ToggleFileDiff {
                chat,
                message,
                part,
            } => self.toggle_file_diff(chat, message, part, cx),
            UiAction::ToggleThought {
                chat,
                message,
                part,
            } => self.toggle_thought(chat, message, part, cx),
            UiAction::ToggleActivity { chat, message } => self.toggle_activity(chat, message, cx),
            UiAction::AnswerApproval {
                chat,
                message,
                part,
                choice,
            } => self.answer_approval(chat, message, part, choice, cx),
            UiAction::AnswerQuestionChoice {
                chat,
                message,
                part,
                option,
            } => self.answer_question_choice(chat, message, part, option, cx),
            UiAction::AnswerQuestionMulti {
                chat,
                message,
                part,
            } => self.answer_question_multi(chat, message, part, cx),
            UiAction::SetMode(mode) => self.set_mode(mode, cx),
            UiAction::ToggleSendOnEnter => {
                self.settings.send_on_enter = !self.settings.send_on_enter;
                cx.notify();
            }
            UiAction::ToggleRuntimeVisibility(runtime_id) => {
                let was_enabled = !self
                    .settings
                    .disabled_runtimes
                    .iter()
                    .any(|disabled| disabled == &runtime_id);
                let enabled_count = self
                    .engine
                    .available_runtimes()
                    .iter()
                    .filter(|runtime| {
                        !self
                            .settings
                            .disabled_runtimes
                            .iter()
                            .any(|disabled| disabled == runtime.id)
                    })
                    .count();
                if was_enabled {
                    if enabled_count > 1 {
                        if !self.settings.disabled_runtimes.contains(&runtime_id) {
                            self.settings.disabled_runtimes.push(runtime_id.clone());
                        }
                        if crate::application::engine::Engine::runtime_base(
                            &self.settings.default_runtime,
                        ) == runtime_id
                            && let Some(next) =
                                self.engine.available_runtimes().iter().find(|runtime| {
                                    runtime.id != runtime_id
                                        && !self
                                            .settings
                                            .disabled_runtimes
                                            .iter()
                                            .any(|disabled| disabled == runtime.id)
                                })
                        {
                            self.settings.default_runtime = next.id.to_string();
                        }
                    }
                } else {
                    self.settings
                        .disabled_runtimes
                        .retain(|disabled| disabled != &runtime_id);
                }
                cx.notify();
            }
            UiAction::SetDefaultRuntime(runtime_id) => {
                self.settings.default_runtime = runtime_id;
                cx.notify();
            }
            UiAction::TogglePinned(target) => {
                match target {
                    ContextTarget::Bot(id) => {
                        if let Some(bot) = self.workspace.bot_mut(id) {
                            bot.pinned = !bot.pinned;
                        }
                    }
                    ContextTarget::Team(id) => {
                        if let Some(team) =
                            self.workspace.chats.iter_mut().find(|chat| chat.id == id)
                        {
                            team.pinned = !team.pinned;
                        }
                    }
                }
                cx.notify();
            }
            UiAction::DeleteContextTarget(target) => self.delete_context_target(target, cx),
            UiAction::AddAllowedFolder { bot, path } => {
                if let Some(bot) = self.workspace.bot_mut(bot)
                    && !bot.allowed_folders.contains(&path)
                {
                    bot.allowed_folders.push(path);
                    cx.notify();
                }
            }
            UiAction::RemoveAllowedFolder { bot, index } => {
                if self.run_bot == Some(bot) {
                    self.stop_run_without_persist(cx);
                }
                if let Some(bot) = self.workspace.bot_mut(bot)
                    && index < bot.allowed_folders.len()
                {
                    bot.allowed_folders.remove(index);
                    cx.notify();
                }
            }
            UiAction::SetBotSandbox { bot, sandbox } => {
                if let Some(bot) = self.workspace.bot_mut(bot) {
                    bot.sandbox = sandbox;
                }
                self.permissions_open = false;
                self.permissions_position = None;
                cx.notify();
            }
            UiAction::SaveBot(submission) => self.save_bot(submission, cx),
            UiAction::SaveTeam {
                target,
                title,
                members,
            } => self.save_team(target, title, members, cx),
            UiAction::AddBotToRoom { room, bot } => self.add_bot_to_room(room, bot, cx),
            UiAction::RemoveBotFromRoom { room, bot } => self.remove_bot_from_room(room, bot, cx),
            UiAction::ApplyProviderChoice { host, choice } => {
                self.apply_provider_choice(host, choice, cx)
            }
        }
        if persists {
            self.persist_workspace();
        }
    }
}
