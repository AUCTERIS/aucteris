//! The workspace: bots, chats and everything the bot panel shows.
//!
//! This is deliberately plain data — no entities, no async. The shell owns one
//! [`Workspace`] is the UI's persisted view of the integrated local engine.

use aucteris_harness::komet_proto::SandboxLevel;
use gpui::SharedString;

/// How a sandbox level reads in a row or a status strip.
pub fn sandbox_label(level: SandboxLevel) -> &'static str {
    match level {
        SandboxLevel::ReadOnly => "Read-only",
        SandboxLevel::WorkspaceWrite => "Workspace write",
        SandboxLevel::DangerFullAccess => "Full access",
    }
}

/// Next sandbox level in the cycle a click walks through.
pub fn next_sandbox(level: SandboxLevel) -> SandboxLevel {
    match level {
        SandboxLevel::ReadOnly => SandboxLevel::WorkspaceWrite,
        SandboxLevel::WorkspaceWrite => SandboxLevel::DangerFullAccess,
        SandboxLevel::DangerFullAccess => SandboxLevel::ReadOnly,
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

/// The 26 expressive mascot shapes from Grok Bot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MascotShape {
    Arch,
    Bean,
    Blob,
    Capsule,
    Cloud,
    Clover,
    Crystal,
    Cylinder,
    Dome,
    Egg,
    Flower,
    Gem,
    Heart,
    Hex,
    House,
    Leaf,
    Pebble,
    Pumpkin,
    Shield,
    #[default]
    Sparkle,
    Square,
    Squircle,
    Star,
    Tablet,
    Teardrop,
    Wedge,
}

impl MascotShape {
    pub const ALL: [MascotShape; 26] = [
        MascotShape::Arch,
        MascotShape::Bean,
        MascotShape::Blob,
        MascotShape::Capsule,
        MascotShape::Cloud,
        MascotShape::Clover,
        MascotShape::Crystal,
        MascotShape::Cylinder,
        MascotShape::Dome,
        MascotShape::Egg,
        MascotShape::Flower,
        MascotShape::Gem,
        MascotShape::Heart,
        MascotShape::Hex,
        MascotShape::House,
        MascotShape::Leaf,
        MascotShape::Pebble,
        MascotShape::Pumpkin,
        MascotShape::Shield,
        MascotShape::Sparkle,
        MascotShape::Square,
        MascotShape::Squircle,
        MascotShape::Star,
        MascotShape::Tablet,
        MascotShape::Teardrop,
        MascotShape::Wedge,
    ];

    pub fn id_str(self) -> &'static str {
        match self {
            Self::Arch => "arch",
            Self::Bean => "bean",
            Self::Blob => "blob",
            Self::Capsule => "capsule",
            Self::Cloud => "cloud",
            Self::Clover => "clover",
            Self::Crystal => "crystal",
            Self::Cylinder => "cylinder",
            Self::Dome => "dome",
            Self::Egg => "egg",
            Self::Flower => "flower",
            Self::Gem => "gem",
            Self::Heart => "heart",
            Self::Hex => "hex",
            Self::House => "house",
            Self::Leaf => "leaf",
            Self::Pebble => "pebble",
            Self::Pumpkin => "pumpkin",
            Self::Shield => "shield",
            Self::Sparkle => "sparkle",
            Self::Square => "square",
            Self::Squircle => "squircle",
            Self::Star => "star",
            Self::Tablet => "tablet",
            Self::Teardrop => "teardrop",
            Self::Wedge => "wedge",
        }
    }

    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Self::Arch => "Arch",
            Self::Bean => "Bean",
            Self::Blob => "Blob",
            Self::Capsule => "Capsule",
            Self::Cloud => "Cloud",
            Self::Clover => "Clover",
            Self::Crystal => "Crystal",
            Self::Cylinder => "Cylinder",
            Self::Dome => "Dome",
            Self::Egg => "Egg",
            Self::Flower => "Flower",
            Self::Gem => "Gem",
            Self::Heart => "Heart",
            Self::Hex => "Hexagon",
            Self::House => "House",
            Self::Leaf => "Leaf",
            Self::Pebble => "Pebble",
            Self::Pumpkin => "Pumpkin",
            Self::Shield => "Shield",
            Self::Sparkle => "Sparkle",
            Self::Square => "Square",
            Self::Squircle => "Squircle",
            Self::Star => "Star",
            Self::Tablet => "Tablet",
            Self::Teardrop => "Teardrop",
            Self::Wedge => "Wedge",
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
    pub folder: SharedString,
    pub instructions: SharedString,
    pub tokens: u64,
    /// OS-level confinement this bot runs under. Asked at send time.
    pub sandbox: SandboxLevel,
}

/// One piece of a message. A reply is a sequence of these, so tool work reads
/// inline with the words that explain it.
#[derive(Debug, Clone)]
pub enum Part {
    Text(String),
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
        /// The result line once the step settled.
        result: Option<String>,
    },
    /// A file the bot touched, with its diff size.
    Files {
        path: String,
        added: u32,
        removed: u32,
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
        choices: Vec<(String, bool)>,
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
    /// Set when the person stopped the run mid-answer.
    pub stopped: bool,
}

impl Message {
    pub fn text(&self) -> String {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Text(text) => out.push_str(text),
                Part::Thought { text, .. } => {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(text);
                }
                Part::Tool { label, .. } => {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(label);
                }
                Part::Files { path, .. } => {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(path);
                }
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
        let running = self
            .parts
            .iter_mut()
            .rev()
            .find(|part| matches!(part, Part::Tool { done: false, .. }));
        if let Some(Part::Tool {
            done, result: slot, ..
        }) = running
        {
            *done = true;
            *slot = result;
        }
    }
}

#[derive(Debug, Clone)]
pub struct Chat {
    pub id: u32,
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
        if let Some(message) = self
            .chats
            .get(self.selected)
            .and_then(|chat| chat.messages.last())
        {
            let preview = message.text();
            let preview = preview.replace('\n', " ");
            let short: String = preview.chars().take(72).collect();
            let chat = self.chat_mut();
            chat.preview = SharedString::from(short);
            chat.time = SharedString::from("now");
        }
    }
}

impl Workspace {
    pub fn empty() -> Self {
        Self {
            bots: Vec::new(),
            chats: Vec::new(),
            selected: 0,
            tokens_used: 0,
            tokens_cap: 2_000_000,
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
    /// Stable CLI runtime id assigned to new bots.
    pub default_runtime: String,
    /// Enter sends. Off means Enter is a line break and Ctrl+Enter sends.
    pub send_on_enter: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Simple,
            default_runtime: "codex".to_string(),
            send_on_enter: true,
        }
    }
}
