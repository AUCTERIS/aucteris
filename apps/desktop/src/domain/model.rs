//! The workspace: bots, chats and everything the bot panel shows.
//!
//! This is deliberately plain data — no entities, no async. The shell owns one
//! [`Workspace`] is the UI's persisted view of the integrated local engine.

use aucteris_harness::komet_proto::{PermissionChoice, SandboxLevel};
use gpui::SharedString;
use std::time::Instant;

/// Sidebar/message timestamp label. `HH:MM` local time — the sidebar shows
/// the last-activity clock time; there is no date granularity yet.
/// shortcut: day-level grouping ("Yesterday", dates) needs a structured
/// timestamp on Chat/Message, upgrade when the list outgrows a day.
pub fn timestamp_now() -> SharedString {
    chrono::Local::now().format("%H:%M").to_string().into()
}

/// How a sandbox level reads in a row or a status strip.
pub fn sandbox_label(level: SandboxLevel) -> &'static str {
    match level {
        SandboxLevel::ReadOnly => "Read-only",
        SandboxLevel::WorkspaceWrite => "Workspace write",
        SandboxLevel::DangerFullAccess => "Full access",
    }
}

/// Where a bot is right now. Three states, one mark on the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Resting,
    Working,
    Waiting,
}

impl Presence {
    pub fn label(self) -> &'static str {
        match self {
            Self::Resting => "Resting",
            Self::Working => "Working",
            Self::Waiting => "Waiting on you",
        }
    }

    #[allow(dead_code)]
    pub fn is_working(self) -> bool {
        matches!(self, Self::Working)
    }
}

/// The eight Aucteris mascots: basic geometric shapes, one per bot identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MascotShape {
    #[default]
    Circle,
    Triangle,
    Square,
    Diamond,
    Pentagon,
    Hexagon,
    Octagon,
    Star,
}

impl MascotShape {
    pub const ALL: [MascotShape; 8] = [
        MascotShape::Circle,
        MascotShape::Triangle,
        MascotShape::Square,
        MascotShape::Diamond,
        MascotShape::Pentagon,
        MascotShape::Hexagon,
        MascotShape::Octagon,
        MascotShape::Star,
    ];

    pub fn id_str(self) -> &'static str {
        match self {
            Self::Circle => "circle",
            Self::Triangle => "triangle",
            Self::Square => "square",
            Self::Diamond => "diamond",
            Self::Pentagon => "pentagon",
            Self::Hexagon => "hexagon",
            Self::Octagon => "octagon",
            Self::Star => "star",
        }
    }

    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Self::Circle => "Circle",
            Self::Triangle => "Triangle",
            Self::Square => "Square",
            Self::Diamond => "Diamond",
            Self::Pentagon => "Pentagon",
            Self::Hexagon => "Hexagon",
            Self::Octagon => "Octagon",
            Self::Star => "Star",
        }
    }

    #[allow(dead_code)]
    pub fn from_index(index: usize) -> Self {
        Self::ALL[index % Self::ALL.len()]
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone)]
pub struct Bot {
    pub id: u32,
    pub pinned: bool,
    pub name: SharedString,
    /// What this bot is for, in one word: Research, Engineering, Writing…
    pub role: SharedString,
    /// Slot in the identity palette.
    pub color: usize,
    /// The bot's expressive shape from Grok Bot.
    pub mascot: MascotShape,
    /// Stable runtime identifier selected from installed agent CLIs.
    pub runtime: SharedString,
    pub presence: Presence,
    /// One line under the name in the sidebar: what it is doing or did.
    pub activity: SharedString,
    /// Private, automatically assigned workspace for this bot.
    pub folder: SharedString,
    /// Additional user-approved directories available to this bot.
    pub allowed_folders: Vec<String>,
    pub instructions: SharedString,
    pub tokens: u64,
    /// Last context-window limit the runtime reported for this bot's
    /// model. `None` until a harness emits one — the usage meter stays
    /// empty rather than dividing by an invented cap.
    pub context_limit: Option<u64>,
    /// OS-level confinement this bot runs under. Asked at send time.
    pub sandbox: SandboxLevel,
}

/// One piece of a message. A reply is a sequence of these, so tool work reads
/// inline with the words that explain it.
#[derive(Debug, Clone)]
pub enum Part {
    Text(String),
    /// A file selected by the user and attached to their prompt.
    Attachment {
        path: String,
    },
    /// Collapsible reasoning / thought process block (like Grok / Cursor reasoning mode).
    Thought {
        text: String,
        collapsed: bool,
    },
    /// A single step a bot took: a command, a search, a page it opened.
    Tool {
        icon: &'static str,
        label: String,
        detail: Option<String>,
        /// `false` while the step is still running.
        done: bool,
        /// The step settled with an error rather than a result.
        failed: bool,
        /// The result line once the step settled.
        result: Option<String>,
        /// Real elapsed time recorded when the action settles.
        duration_ms: Option<u64>,
        /// Monotonic start instant for a live action (not persisted).
        started_at: Option<Instant>,
    },
    /// A file the bot touched, with its diff size.
    Files {
        path: String,
        added: u32,
        removed: u32,
        #[allow(dead_code)]
        old_text: Option<String>,
        new_text: Option<String>,
        diff_expanded: bool,
        undo_error: Option<String>,
        undone: bool,
    },
    /// Something the bot changed outside the chat, with a way back.
    Note {
        text: String,
        undone: bool,
    },
    /// A local approval card that can be resolved from the conversation.
    Approval {
        kicker: String,
        title: String,
        detail: String,
        /// Pending permission request this card answers, if any.
        request: Option<u64>,
        /// One button per choice: label, and whether it allows the action.
        choices: Vec<(String, PermissionChoice)>,
        /// Index of the additional "allow and add folder" choice, if any.
        grant_folder_choice: Option<usize>,
        resolved: Option<String>,
    },
    /// A question the agent asked and is blocked on. One card per question.
    Question {
        request: u64,
        index: usize,
        header: String,
        question: String,
        options: Vec<String>,
        multi_select: bool,
        allow_free_text: bool,
        /// The answer must not be shown back in the transcript.
        secret: bool,
        selected: Vec<String>,
        resolved: Option<String>,
    },
}

/// Hide host-side lifecycle hooks from the user-facing transcript. Older
/// saved messages used to keep the hook command in a Bash label or detail.
pub fn is_internal_tool(label: &str, detail: Option<&str>) -> bool {
    let label = label.to_ascii_lowercase();
    if label.starts_with("__internal_")
        || label.starts_with("ran hook:")
        || label.starts_with("run hook:")
    {
        return true;
    }
    detail.is_some_and(|detail| {
        let detail = detail.to_ascii_lowercase();
        detail.contains("session_start")
            || detail.contains("prompt-hook")
            || detail.contains(".claude/skills/")
            || detail.contains(".codex/hooks/")
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Author {
    You,
    Bot,
    /// A room event: someone joined, a bot handed work on.
    System,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub author: Author,
    /// Which bot wrote it, for [`Author::Bot`] inside rooms.
    pub bot: Option<u32>,
    pub parts: Vec<Part>,
    pub time: SharedString,
    pub streaming: bool,
    /// Total wall-clock duration of an assistant run, retained in history.
    pub duration_ms: Option<u64>,
    /// Monotonic start instant for an active run (not persisted).
    pub started_at: Option<Instant>,
    /// Whether the chronological reasoning and tool activity is expanded.
    pub activity_expanded: bool,
    /// Set when the person stopped the run mid-answer.
    pub stopped: bool,
}

impl Message {
    pub fn finish_stream(&mut self) {
        self.streaming = false;
        self.duration_ms = self
            .started_at
            .take()
            .map(|start| start.elapsed().as_millis() as u64);
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Text(text) => out.push_str(text),
                Part::Thought { .. }
                | Part::Tool { .. }
                | Part::Files { .. }
                | Part::Attachment { .. } => {}
                Part::Note { text, .. } => {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(text);
                }
                Part::Approval {
                    title,
                    detail,
                    resolved,
                    ..
                } => {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(title);
                    out.push(' ');
                    out.push_str(detail);
                    if let Some(resolved) = resolved {
                        out.push(' ');
                        out.push_str(resolved);
                    }
                }
                Part::Question {
                    question, resolved, ..
                } => {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(question);
                    if let Some(resolved) = resolved {
                        out.push(' ');
                        out.push_str(resolved);
                    }
                }
            }
        }
        out
    }

    pub fn push_text(&mut self, chunk: &str) {
        if let Some(Part::Text(text)) = self.parts.last_mut() {
            text.push_str(chunk);
        } else {
            self.parts.push(Part::Text(chunk.to_string()));
        }
    }

    /// Append reasoning to the open thought block, or start one.
    pub fn push_thought(&mut self, chunk: &str) {
        if let Some(Part::Thought { text, .. }) = self.parts.last_mut() {
            text.push_str(chunk);
        } else {
            self.parts.push(Part::Thought {
                text: chunk.to_string(),
                collapsed: true,
            });
        }
    }

    pub fn push_part(&mut self, part: Part) {
        self.parts.push(part);
    }

    /// Close the running tool step, if there is one.
    pub fn finish_last_tool(&mut self, result: Option<String>) {
        self.settle_last_tool(result, false);
    }

    /// Close the running tool step and mark it as failed.
    pub fn fail_last_tool(&mut self, result: Option<String>) {
        self.settle_last_tool(result, true);
    }

    fn settle_last_tool(&mut self, result: Option<String>, failed: bool) {
        let running = self
            .parts
            .iter_mut()
            .rev()
            .find(|part| matches!(part, Part::Tool { done: false, .. }));
        if let Some(Part::Tool {
            done,
            failed: slot_failed,
            result: slot,
            duration_ms,
            started_at,
            ..
        }) = running
        {
            *done = true;
            *slot_failed = failed;
            *slot = result;
            *duration_ms = started_at.map(|start| start.elapsed().as_millis() as u64);
            *started_at = None;
        }
    }
}

#[derive(Debug, Clone)]
pub struct Chat {
    pub id: u32,
    pub pinned: bool,
    pub title: SharedString,
    /// `Some` for a direct chat, `None` for a room.
    pub bot: Option<u32>,
    /// Everyone in the room. Empty for a direct chat.
    pub members: Vec<u32>,
    pub messages: Vec<Message>,
    /// The one line the sidebar shows under the title.
    pub preview: SharedString,
    pub time: SharedString,
    /// Harness-native session id, resumed by the next prompt in this chat.
    pub session_id: Option<String>,
}

impl Chat {
    pub fn is_room(&self) -> bool {
        self.bot.is_none()
    }
}

/// Everything the app knows. One of these lives in the shell.
pub struct Workspace {
    pub bots: Vec<Bot>,
    pub chats: Vec<Chat>,
    pub selected: usize,
    pub tokens_used: u64,
    pub tokens_cap: u64,
}

impl Workspace {
    pub fn bot(&self, id: u32) -> Option<&Bot> {
        self.bots.iter().find(|bot| bot.id == id)
    }

    pub fn bot_mut(&mut self, id: u32) -> Option<&mut Bot> {
        self.bots.iter_mut().find(|bot| bot.id == id)
    }

    pub fn chat(&self) -> &Chat {
        &self.chats[self.selected]
    }

    pub fn chat_mut(&mut self) -> &mut Chat {
        &mut self.chats[self.selected]
    }

    /// The bot a direct chat belongs to.
    pub fn selected_bot(&self) -> Option<u32> {
        self.chats.get(self.selected)?.bot
    }

    pub fn select(&mut self, index: usize) {
        if index < self.chats.len() {
            self.selected = index;
        }
    }

    /// Bots the composer talks to: one for a chat, everyone for a room.
    pub fn audience(&self) -> Vec<u32> {
        let Some(chat) = self.chats.get(self.selected) else {
            return Vec::new();
        };
        match chat.bot {
            Some(bot) => vec![bot],
            None => chat.members.clone(),
        }
    }

    /// The bot that should answer next: for a room, whoever is free, else the
    /// first member.
    pub fn responder(&self) -> Option<u32> {
        let audience = self.audience();
        audience
            .iter()
            .copied()
            .find(|id| {
                self.bot(*id)
                    .map(|bot| bot.presence != Presence::Working)
                    .unwrap_or(false)
            })
            .or_else(|| audience.first().copied())
    }

    #[allow(dead_code)]
    pub fn total_working(&self) -> usize {
        self.bots
            .iter()
            .filter(|bot| bot.presence.is_working())
            .count()
    }

    pub fn replace_last_preview(&mut self) {
        let chat_id = self.chat().id;
        self.replace_preview_for(chat_id);
    }

    /// Refresh one chat's sidebar preview, whichever chat is selected. A run
    /// keeps writing to its own chat while the person looks elsewhere.
    pub fn replace_preview_for(&mut self, chat_id: u32) {
        let Some(chat) = self.chats.iter_mut().find(|chat| chat.id == chat_id) else {
            return;
        };
        let Some(message) = chat.messages.last() else {
            return;
        };
        let preview = message.text();
        let preview = preview.replace('\n', " ");
        let short: String = preview.chars().take(72).collect();
        chat.preview = SharedString::from(short);
        chat.time = timestamp_now();
    }
}

impl Workspace {
    pub fn empty() -> Self {
        Self {
            bots: Vec::new(),
            chats: Vec::new(),
            selected: 0,
            tokens_used: 0,
            // 0 = unknown until a runtime reports its model's window;
            // Action::Usage/ContextWindow overwrite it with the real cap.
            tokens_cap: 0,
        }
    }
}

/// How much of the app is on show.
///
/// Simple is the default for a new install: tokens, usage meters and the
/// inspector rows belong in Advanced. Advanced adds to the screen, it never
/// rearranges it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Simple,
    Advanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppearancePreference {
    Dark,
    Light,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Simple => "Simple",
            Self::Advanced => "Advanced",
        }
    }

    pub fn is_simple(self) -> bool {
        matches!(self, Self::Simple)
    }
}

/// App-wide preferences.
///
/// A single bot's settings live in its own panel; these are the few things that
/// change the whole app, and every one of them does something the moment it
/// changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub mode: Mode,
    /// `None` follows the operating-system appearance.
    pub appearance: Option<AppearancePreference>,
    /// Installed agent CLIs hidden from model pickers.
    pub disabled_runtimes: Vec<String>,
    /// Stable CLI runtime id assigned to new bots.
    pub default_runtime: String,
    /// Enter sends. Off means Enter is a line break and Ctrl+Enter sends.
    pub send_on_enter: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Simple,
            appearance: None,
            disabled_runtimes: Vec::new(),
            default_runtime: "codex".to_string(),
            send_on_enter: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply_with_running_tool() -> Message {
        Message {
            author: Author::Bot,
            bot: Some(1),
            parts: vec![Part::Tool {
                icon: "terminal",
                label: "cargo test".to_string(),
                detail: None,
                done: false,
                failed: false,
                result: None,
                duration_ms: None,
                started_at: Some(Instant::now()),
            }],
            time: "now".into(),
            streaming: true,
            duration_ms: None,
            started_at: Some(Instant::now()),
            activity_expanded: false,
            stopped: false,
        }
    }

    #[test]
    fn finishing_a_tool_marks_it_done_without_failure() {
        let mut message = reply_with_running_tool();
        message.finish_last_tool(Some("ok".to_string()));
        match &message.parts[0] {
            Part::Tool {
                done,
                failed,
                result,
                ..
            } => {
                assert!(*done);
                assert!(!*failed);
                assert_eq!(result.as_deref(), Some("ok"));
            }
            other => panic!("expected a tool part, got {other:?}"),
        }
    }

    #[test]
    fn failing_a_tool_marks_it_done_and_failed() {
        let mut message = reply_with_running_tool();
        message.fail_last_tool(Some("Run failed".to_string()));
        match &message.parts[0] {
            Part::Tool {
                done,
                failed,
                result,
                ..
            } => {
                assert!(*done);
                assert!(*failed);
                assert_eq!(result.as_deref(), Some("Run failed"));
            }
            other => panic!("expected a tool part, got {other:?}"),
        }
    }
}
