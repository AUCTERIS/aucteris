mod author;
mod bot;
mod part;

use crate::domain::model::{Author, Message};
use crate::presentation::theme::Theme;
use crate::presentation::ui::shell::Shell;
use gpui::{AnyElement, Context};

impl Shell {
    pub(in crate::presentation::ui::chat) fn message_view(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        chat_index: usize,
        message_index: usize,
        message: &Message,
    ) -> AnyElement {
        match message.author {
            Author::You => self.user_message_view(theme, cx, message_index, message),
            Author::System => self.system_message_view(theme, message),
            Author::Bot => self.bot_message_view(theme, cx, chat_index, message_index, message),
        }
    }
}
