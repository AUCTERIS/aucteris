//! Workspace mutations that are not part of a run: team membership,
//! bot panel settings, panel/appearance toggles.

use gpui::Context;

use crate::domain::model::{self, Part};
use crate::presentation::theme::Theme;

use super::Shell;

impl Shell {
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
            time: model::timestamp_now(),
            session_id: None,
        });
        self.workspace.selected = self.workspace.chats.len() - 1;
        self.panel_open = true;
        self.scroll.scroll_to_bottom();
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
        cx.notify();
    }
    // ---- bot panel actions ----------------------------------------------

    pub(crate) fn cycle_sandbox(&mut self, bot_id: u32, cx: &mut Context<Self>) {
        if let Some(bot) = self.workspace.bot_mut(bot_id) {
            bot.sandbox = model::next_sandbox(bot.sandbox);
        }
        cx.notify();
    }

    pub(crate) fn reset_usage(&mut self, bot_id: u32, cx: &mut Context<Self>) {
        if let Some(bot) = self.workspace.bot_mut(bot_id) {
            bot.tokens = 0;
        }
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
        if let Some(chat) = self.workspace.chats.get_mut(chat_index)
            && let Some(message) = chat.messages.get_mut(message_index)
        {
            match message.parts.get_mut(part_index) {
                Some(Part::Files { undone, .. }) | Some(Part::Note { undone, .. }) => {
                    *undone = true;
                }
                _ => {}
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
}
