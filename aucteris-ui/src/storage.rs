//! Local UI persistence for bot profiles and plain chat history.

use crate::model::{Author, Bot, Chat, MascotShape, Message, Part, Presence, Workspace};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
struct PersistedWorkspace {
    bots: Vec<PersistedBot>,
    chats: Vec<PersistedChat>,
    tokens_used: u64,
    tokens_cap: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedBot {
    id: u32,
    name: String,
    role: String,
    color: usize,
    mascot: String,
    runtime: String,
    folder: String,
    instructions: String,
    tokens: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedChat {
    id: u32,
    title: String,
    bot: Option<u32>,
    members: Vec<u32>,
    messages: Vec<PersistedMessage>,
    preview: String,
    time: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedMessage {
    author: String,
    bot: Option<u32>,
    text: String,
    time: String,
    stopped: bool,
}

pub struct AppStorage {
    path: PathBuf,
}

impl AppStorage {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            path: data_dir.into().join("desktop-state.json"),
        }
    }

    pub fn data_dir() -> PathBuf {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .unwrap_or_else(|| PathBuf::from("."))
            .join("aucteris")
    }

    pub fn load(&self) -> Workspace {
        let Ok(raw) = std::fs::read_to_string(&self.path) else {
            return Workspace::empty();
        };
        let Ok(state) = serde_json::from_str::<PersistedWorkspace>(&raw) else {
            return Workspace::empty();
        };
        state.into()
    }

    pub fn save(&self, workspace: &Workspace) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let state = PersistedWorkspace::from_workspace(workspace);
        let bytes = serde_json::to_vec_pretty(&state)?;
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, bytes)?;
        std::fs::rename(temp, &self.path)?;
        Ok(())
    }
}

impl PersistedWorkspace {
    fn from_workspace(workspace: &Workspace) -> Self {
        Self {
            bots: workspace.bots.iter().map(PersistedBot::from).collect(),
            chats: workspace.chats.iter().map(PersistedChat::from).collect(),
            tokens_used: workspace.tokens_used,
            tokens_cap: workspace.tokens_cap,
        }
    }
}

impl From<PersistedWorkspace> for Workspace {
    fn from(value: PersistedWorkspace) -> Self {
        let bots = value
            .bots
            .into_iter()
            .map(|bot| Bot {
                id: bot.id,
                name: bot.name.into(),
                role: bot.role.into(),
                color: bot.color,
                mascot: mascot(&bot.mascot),
                runtime: bot.runtime.into(),
                presence: Presence::Resting,
                activity: "Ready".into(),
                folder: bot.folder.into(),
                instructions: bot.instructions.into(),
                tokens: bot.tokens,
            })
            .collect();
        let chats = value.chats.into_iter().map(Chat::from).collect::<Vec<_>>();
        let selected = chats.len().saturating_sub(1);
        Self {
            bots,
            chats,
            selected,
            tokens_used: value.tokens_used,
            tokens_cap: value.tokens_cap.max(1),
        }
    }
}

impl From<&Bot> for PersistedBot {
    fn from(bot: &Bot) -> Self {
        Self {
            id: bot.id,
            name: bot.name.to_string(),
            role: bot.role.to_string(),
            color: bot.color,
            mascot: bot.mascot.id_str().to_string(),
            runtime: bot.runtime.to_string(),
            folder: bot.folder.to_string(),
            instructions: bot.instructions.to_string(),
            tokens: bot.tokens,
        }
    }
}

impl From<&Chat> for PersistedChat {
    fn from(chat: &Chat) -> Self {
        Self {
            id: chat.id,
            title: chat.title.to_string(),
            bot: chat.bot,
            members: chat.members.clone(),
            messages: chat.messages.iter().map(PersistedMessage::from).collect(),
            preview: chat.preview.to_string(),
            time: chat.time.to_string(),
        }
    }
}

impl From<PersistedChat> for Chat {
    fn from(chat: PersistedChat) -> Self {
        Self {
            id: chat.id,
            title: chat.title.into(),
            bot: chat.bot,
            members: chat.members,
            messages: chat.messages.into_iter().map(Message::from).collect(),
            preview: chat.preview.into(),
            time: chat.time.into(),
        }
    }
}

impl From<&Message> for PersistedMessage {
    fn from(message: &Message) -> Self {
        let author = match message.author {
            Author::You => "you",
            Author::Bot => "bot",
            Author::System => "system",
        };
        Self {
            author: author.to_string(),
            bot: message.bot,
            text: message
                .parts
                .iter()
                .filter_map(|part| match part {
                    Part::Text(text) => Some(text.as_str()),
                    Part::Thought { text, .. } => Some(text.as_str()),
                    Part::Tool { label, .. } => Some(label.as_str()),
                    Part::Files { path, .. } => Some(path.as_str()),
                    Part::Note { text, .. } => Some(text.as_str()),
                    Part::Approval { title, .. } => Some(title.as_str()),
                })
                .collect::<Vec<_>>()
                .join("\n"),
            time: message.time.to_string(),
            stopped: message.stopped,
        }
    }
}

impl From<PersistedMessage> for Message {
    fn from(message: PersistedMessage) -> Self {
        let author = match message.author.as_str() {
            "you" => Author::You,
            "system" => Author::System,
            _ => Author::Bot,
        };
        Self {
            author,
            bot: message.bot,
            parts: if message.text.is_empty() {
                Vec::new()
            } else {
                vec![Part::Text(message.text)]
            },
            time: message.time.into(),
            streaming: false,
            stopped: message.stopped,
        }
    }
}

fn mascot(id: &str) -> MascotShape {
    MascotShape::ALL
        .into_iter()
        .find(|shape| shape.id_str() == id)
        .unwrap_or_default()
}

pub fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(path).to_path_buf());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(rest))
            .unwrap_or_else(|| PathBuf::from(path));
    }
    PathBuf::from(path)
}
