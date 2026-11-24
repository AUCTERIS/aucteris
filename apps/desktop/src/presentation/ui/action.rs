//! Single entry point for user intents that mutate shell state.
//!
//! Views never touch workspace state directly: a click or key produces a
//! [`UiAction`], `dispatch` routes it to the handler and performs the one
//! controlled disk write for that mutation. Engine-driven mutations (run
//! events, finish/fail) keep their own persistence inside the run pipeline.

use gpui::Context;

use crate::domain::model::{AppearancePreference, Mode};
use crate::presentation::ui::dialog::PickerHost;
use crate::presentation::ui::shell::Shell;

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
    CreateTeam {
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
        !matches!(self, Self::TogglePanel)
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
            UiAction::CreateTeam { title, members } => self.create_team(title, members, cx),
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
