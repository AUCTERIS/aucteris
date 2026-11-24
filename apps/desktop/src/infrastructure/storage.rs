//! Local UI persistence for bot profiles and plain chat history.

use crate::domain::model::{
    AppearancePreference, Author, Bot, Chat, MascotShape, Message, Mode, Part, Presence, Settings,
    Workspace,
};
use aucteris_harness::komet_proto::{PermissionChoice, SandboxLevel};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
struct PersistedWorkspace {
    bots: Vec<PersistedBot>,
    chats: Vec<PersistedChat>,
    tokens_used: u64,
    tokens_cap: u64,
    /// App-wide preferences. Absent in files written before settings were
    /// persisted: those load as `Settings::default()`.
    #[serde(default)]
    settings: PersistedSettings,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedSettings {
    #[serde(default)]
    mode: PersistedMode,
    #[serde(default)]
    appearance: Option<PersistedAppearance>,
    #[serde(default)]
    disabled_runtimes: Vec<String>,
    #[serde(default = "default_runtime")]
    default_runtime: String,
    #[serde(default = "default_send_on_enter")]
    send_on_enter: bool,
}

// Derived Default would bypass the serde field defaults and yield an
// empty runtime / send_on_enter=false for files with no settings at all.
impl Default for PersistedSettings {
    fn default() -> Self {
        Self {
            mode: PersistedMode::default(),
            appearance: None,
            disabled_runtimes: Vec::new(),
            default_runtime: default_runtime(),
            send_on_enter: default_send_on_enter(),
        }
    }
}

fn default_runtime() -> String {
    "codex".to_string()
}

fn default_send_on_enter() -> bool {
    true
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PersistedMode {
    #[default]
    Simple,
    Advanced,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PersistedAppearance {
    Dark,
    Light,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedBot {
    id: u32,
    #[serde(default)]
    pinned: bool,
    name: String,
    role: String,
    color: usize,
    mascot: String,
    runtime: String,
    folder: String,
    #[serde(default)]
    allowed_folders: Vec<String>,
    instructions: String,
    tokens: u64,
    /// Absent in files written before context limits were tracked.
    #[serde(default)]
    context_limit: Option<u64>,
    /// Absent in files written before sandbox levels existed: those bots ran
    /// with everything allowed, so that is what they keep.
    #[serde(default = "full_access")]
    sandbox: SandboxLevel,
}

fn full_access() -> SandboxLevel {
    SandboxLevel::DangerFullAccess
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedChat {
    id: u32,
    #[serde(default)]
    pinned: bool,
    title: String,
    bot: Option<u32>,
    members: Vec<u32>,
    messages: Vec<PersistedMessage>,
    preview: String,
    time: String,
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedMessage {
    author: String,
    bot: Option<u32>,
    /// Flattened fallback kept for readers of the pre-parts format. New files
    /// carry `parts`; this field stays populated so an older build can still
    /// show something meaningful.
    text: String,
    /// Rich message parts. Absent in files written before rich parts were
    /// persisted: those load as a single `Part::Text`.
    #[serde(default)]
    parts: Option<Vec<PersistedPart>>,
    time: String,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    activity_expanded: bool,
    stopped: bool,
}

/// Serializable mirror of [`Part`]. `Part::Tool::icon` is a `&'static str`
/// drawn from a fixed set of bundled glyphs, so it persists as a plain string
/// and is mapped back through [`icon_id`].
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PersistedPart {
    Text {
        text: String,
    },
    Attachment {
        path: String,
    },
    Thought {
        text: String,
        collapsed: bool,
    },
    Tool {
        icon: String,
        label: String,
        detail: Option<String>,
        done: bool,
        /// Absent in files written before tool failures were recorded.
        #[serde(default)]
        failed: bool,
        result: Option<String>,
        /// Absent in existing saved chats.
        #[serde(default)]
        duration_ms: Option<u64>,
    },
    Files {
        path: String,
        added: u32,
        removed: u32,
        #[serde(default)]
        old_text: Option<String>,
        #[serde(default)]
        new_text: Option<String>,
        #[serde(default)]
        diff_expanded: bool,
        #[serde(default)]
        undo_error: Option<String>,
        undone: bool,
    },
    Note {
        text: String,
        undone: bool,
    },
    Approval {
        kicker: String,
        title: String,
        detail: String,
        request: Option<u64>,
        choices: Vec<(String, PersistedApprovalChoice)>,
        #[serde(default)]
        grant_folder_choice: Option<usize>,
        resolved: Option<String>,
    },
    Question {
        request: u64,
        index: usize,
        header: String,
        question: String,
        options: Vec<String>,
        multi_select: bool,
        allow_free_text: bool,
        secret: bool,
        selected: Vec<String>,
        resolved: Option<String>,
    },
}

/// Read the original boolean approval choices as well as the newer, exact
/// permission decision (including scope) without invalidating saved chats.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum PersistedApprovalChoice {
    Legacy(bool),
    Exact(PermissionChoice),
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

    pub fn bot_workspace_path(data_dir: &Path, bot_id: u32) -> PathBuf {
        data_dir.join("bot-workspaces").join(bot_id.to_string())
    }

    /// Load the persisted workspace and app settings. A missing or
    /// unreadable file yields empty defaults for both.
    pub fn load(&self) -> (Workspace, Settings) {
        let Ok(raw) = std::fs::read_to_string(&self.path) else {
            return (Workspace::empty(), Settings::default());
        };
        let Ok(state) = serde_json::from_str::<PersistedWorkspace>(&raw) else {
            return (Workspace::empty(), Settings::default());
        };
        let settings = Settings::from(&state.settings);
        (Workspace::from(state), settings)
    }

    pub fn save(&self, workspace: &Workspace, settings: &Settings) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let state = PersistedWorkspace::from_workspace(workspace, settings);
        let bytes = serde_json::to_vec_pretty(&state)?;
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, bytes)?;
        std::fs::rename(temp, &self.path)?;
        Ok(())
    }
}

impl PersistedWorkspace {
    fn from_workspace(workspace: &Workspace, settings: &Settings) -> Self {
        Self {
            bots: workspace.bots.iter().map(PersistedBot::from).collect(),
            chats: workspace.chats.iter().map(PersistedChat::from).collect(),
            tokens_used: workspace.tokens_used,
            tokens_cap: workspace.tokens_cap,
            settings: PersistedSettings::from(settings),
        }
    }
}

impl From<&Settings> for PersistedSettings {
    fn from(settings: &Settings) -> Self {
        Self {
            mode: match settings.mode {
                Mode::Simple => PersistedMode::Simple,
                Mode::Advanced => PersistedMode::Advanced,
            },
            appearance: settings.appearance.map(|appearance| match appearance {
                AppearancePreference::Dark => PersistedAppearance::Dark,
                AppearancePreference::Light => PersistedAppearance::Light,
            }),
            disabled_runtimes: settings.disabled_runtimes.clone(),
            default_runtime: settings.default_runtime.clone(),
            send_on_enter: settings.send_on_enter,
        }
    }
}

impl From<&PersistedSettings> for Settings {
    fn from(value: &PersistedSettings) -> Self {
        Self {
            mode: match value.mode {
                PersistedMode::Simple => Mode::Simple,
                PersistedMode::Advanced => Mode::Advanced,
            },
            appearance: value.appearance.map(|appearance| match appearance {
                PersistedAppearance::Dark => AppearancePreference::Dark,
                PersistedAppearance::Light => AppearancePreference::Light,
            }),
            disabled_runtimes: value.disabled_runtimes.clone(),
            default_runtime: value.default_runtime.clone(),
            send_on_enter: value.send_on_enter,
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
                pinned: bot.pinned,
                name: bot.name.into(),
                role: bot.role.into(),
                color: bot.color,
                mascot: mascot(&bot.mascot),
                runtime: bot.runtime.into(),
                presence: Presence::Resting,
                activity: "Ready".into(),
                folder: bot.folder.into(),
                allowed_folders: bot.allowed_folders,
                instructions: bot.instructions.into(),
                tokens: bot.tokens,
                context_limit: bot.context_limit,
                sandbox: bot.sandbox,
            })
            .collect();
        let chats = value.chats.into_iter().map(Chat::from).collect::<Vec<_>>();
        let selected = chats.len().saturating_sub(1);
        Self {
            bots,
            chats,
            selected,
            tokens_used: value.tokens_used,
            tokens_cap: value.tokens_cap,
        }
    }
}

impl From<&Bot> for PersistedBot {
    fn from(bot: &Bot) -> Self {
        Self {
            id: bot.id,
            pinned: bot.pinned,
            name: bot.name.to_string(),
            role: bot.role.to_string(),
            color: bot.color,
            mascot: bot.mascot.id_str().to_string(),
            runtime: bot.runtime.to_string(),
            folder: bot.folder.to_string(),
            allowed_folders: bot.allowed_folders.clone(),
            instructions: bot.instructions.to_string(),
            tokens: bot.tokens,
            context_limit: bot.context_limit,
            sandbox: bot.sandbox,
        }
    }
}

impl From<&Chat> for PersistedChat {
    fn from(chat: &Chat) -> Self {
        Self {
            id: chat.id,
            pinned: chat.pinned,
            title: chat.title.to_string(),
            bot: chat.bot,
            members: chat.members.clone(),
            messages: chat.messages.iter().map(PersistedMessage::from).collect(),
            preview: chat.preview.to_string(),
            time: chat.time.to_string(),
            session_id: chat.session_id.clone(),
        }
    }
}

impl From<PersistedChat> for Chat {
    fn from(chat: PersistedChat) -> Self {
        Self {
            id: chat.id,
            pinned: chat.pinned,
            title: chat.title.into(),
            bot: chat.bot,
            members: chat.members,
            messages: chat.messages.into_iter().map(Message::from).collect(),
            preview: chat.preview.into(),
            time: chat.time.into(),
            session_id: chat.session_id,
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
                .map(|part| match part {
                    Part::Text(text) => text.as_str(),
                    Part::Attachment { path } => path.as_str(),
                    Part::Thought { text, .. } => text.as_str(),
                    Part::Tool { label, .. } => label.as_str(),
                    Part::Files { path, .. } => path.as_str(),
                    Part::Note { text, .. } => text.as_str(),
                    Part::Approval { title, .. } => title.as_str(),
                    Part::Question { question, .. } => question.as_str(),
                })
                .collect::<Vec<_>>()
                .join("\n"),
            parts: Some(message.parts.iter().map(PersistedPart::from).collect()),
            time: message.time.to_string(),
            duration_ms: message.duration_ms,
            activity_expanded: message.activity_expanded,
            stopped: message.stopped,
        }
    }
}

impl From<&Part> for PersistedPart {
    fn from(part: &Part) -> Self {
        match part {
            Part::Text(text) => Self::Text { text: text.clone() },
            Part::Attachment { path } => Self::Attachment { path: path.clone() },
            Part::Thought { text, collapsed } => Self::Thought {
                text: text.clone(),
                collapsed: *collapsed,
            },
            Part::Tool {
                icon,
                label,
                detail,
                done,
                failed,
                result,
                duration_ms,
                ..
            } => Self::Tool {
                icon: icon.to_string(),
                label: label.clone(),
                detail: detail.clone(),
                done: *done,
                failed: *failed,
                result: result.clone(),
                duration_ms: *duration_ms,
            },
            Part::Files {
                path,
                added,
                removed,
                old_text,
                new_text,
                diff_expanded,
                undo_error,
                undone,
            } => Self::Files {
                path: path.clone(),
                added: *added,
                removed: *removed,
                old_text: old_text.clone(),
                new_text: new_text.clone(),
                diff_expanded: *diff_expanded,
                undo_error: undo_error.clone(),
                undone: *undone,
            },
            Part::Note { text, undone } => Self::Note {
                text: text.clone(),
                undone: *undone,
            },
            Part::Approval {
                kicker,
                title,
                detail,
                request,
                choices,
                grant_folder_choice,
                resolved,
            } => Self::Approval {
                kicker: kicker.clone(),
                title: title.clone(),
                detail: detail.clone(),
                request: *request,
                choices: choices
                    .iter()
                    .map(|(label, choice)| {
                        (
                            label.clone(),
                            PersistedApprovalChoice::Exact(choice.clone()),
                        )
                    })
                    .collect(),
                grant_folder_choice: *grant_folder_choice,
                resolved: resolved.clone(),
            },
            Part::Question {
                request,
                index,
                header,
                question,
                options,
                multi_select,
                allow_free_text,
                secret,
                selected,
                resolved,
            } => Self::Question {
                request: *request,
                index: *index,
                header: header.clone(),
                question: question.clone(),
                options: options.clone(),
                multi_select: *multi_select,
                allow_free_text: *allow_free_text,
                secret: *secret,
                selected: selected.clone(),
                resolved: resolved.clone(),
            },
        }
    }
}

impl From<PersistedPart> for Part {
    fn from(part: PersistedPart) -> Self {
        match part {
            PersistedPart::Text { text } => Self::Text(text),
            PersistedPart::Attachment { path } => Self::Attachment { path },
            PersistedPart::Thought { text, collapsed } => Self::Thought { text, collapsed },
            PersistedPart::Tool {
                icon,
                label,
                detail,
                done,
                failed,
                result,
                duration_ms,
            } => Self::Tool {
                icon: icon_id(&icon),
                label,
                detail,
                done,
                failed,
                result,
                duration_ms,
                started_at: None,
            },
            PersistedPart::Files {
                path,
                added,
                removed,
                old_text,
                new_text,
                diff_expanded,
                undo_error,
                undone,
            } => Self::Files {
                path,
                added,
                removed,
                old_text,
                new_text,
                diff_expanded,
                undo_error,
                undone,
            },
            PersistedPart::Note { text, undone } => Self::Note { text, undone },
            PersistedPart::Approval {
                kicker,
                title,
                detail,
                request,
                choices,
                grant_folder_choice,
                resolved,
            } => {
                let choices = choices
                    .into_iter()
                    .map(|(label, choice)| {
                        let choice = match choice {
                            PersistedApprovalChoice::Legacy(true) => PermissionChoice::Allow,
                            PersistedApprovalChoice::Legacy(false) => PermissionChoice::Deny,
                            PersistedApprovalChoice::Exact(choice) => choice,
                        };
                        (label, choice)
                    })
                    .collect();
                Self::Approval {
                    kicker,
                    title,
                    detail,
                    request,
                    choices,
                    grant_folder_choice,
                    resolved,
                }
            }
            PersistedPart::Question {
                request,
                index,
                header,
                question,
                options,
                multi_select,
                allow_free_text,
                secret,
                selected,
                resolved,
            } => Self::Question {
                request,
                index,
                header,
                question,
                options,
                multi_select,
                allow_free_text,
                secret,
                selected,
                resolved,
            },
        }
    }
}

/// Map a persisted icon name back to one of the bundled glyph ids. Unknown
/// names fall back to the generic bot glyph rather than failing the load.
fn icon_id(name: &str) -> &'static str {
    match name {
        "terminal" => "terminal",
        "file" => "file",
        "search" => "search",
        "globe" => "globe",
        _ => "bot",
    }
}

impl From<PersistedMessage> for Message {
    fn from(message: PersistedMessage) -> Self {
        let author = match message.author.as_str() {
            "you" => Author::You,
            "system" => Author::System,
            _ => Author::Bot,
        };
        let parts = match message.parts {
            Some(parts) => parts.into_iter().map(Part::from).collect(),
            None if message.text.is_empty() => Vec::new(),
            None => vec![Part::Text(message.text)],
        };
        Self {
            author,
            bot: message.bot,
            parts,
            time: message.time.into(),
            streaming: false,
            duration_ms: message.duration_ms,
            started_at: None,
            activity_expanded: message.activity_expanded,
            stopped: message.stopped,
        }
    }
}

/// Mascot ids written before the geometric set. They still load, each
/// folded onto one of the current shapes so old bots keep a stable identity.
const LEGACY_MASCOTS: [&str; 26] = [
    "arch", "bean", "blob", "capsule", "cloud", "clover", "crystal", "cylinder", "dome", "egg",
    "flower", "gem", "heart", "hex", "house", "leaf", "pebble", "pumpkin", "shield", "sparkle",
    "square", "squircle", "star", "tablet", "teardrop", "wedge",
];

fn mascot(id: &str) -> MascotShape {
    if let Some(shape) = MascotShape::ALL
        .into_iter()
        .find(|shape| shape.id_str() == id)
    {
        return shape;
    }
    LEGACY_MASCOTS
        .iter()
        .position(|legacy| *legacy == id)
        .map(MascotShape::from_index)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_with_parts(parts: Vec<Part>) -> Workspace {
        let mut workspace = Workspace::empty();
        workspace.bots.push(Bot {
            id: 1,
            pinned: false,
            name: "Scout".into(),
            role: "Research".into(),
            color: 0,
            mascot: MascotShape::Circle,
            runtime: "codex".into(),
            presence: Presence::Resting,
            activity: "Ready".into(),
            folder: "/tmp".into(),
            allowed_folders: Vec::new(),
            instructions: String::new().into(),
            tokens: 0,
            context_limit: None,
            sandbox: SandboxLevel::DangerFullAccess,
        });
        workspace.chats.push(Chat {
            id: 1,
            pinned: false,
            title: "Chat".into(),
            bot: Some(1),
            members: Vec::new(),
            messages: vec![Message {
                author: Author::Bot,
                bot: Some(1),
                parts,
                time: "now".into(),
                streaming: false,
                duration_ms: None,
                started_at: None,
                activity_expanded: false,
                stopped: false,
            }],
            preview: "preview".into(),
            time: "now".into(),
            session_id: Some("sess-1".into()),
        });
        workspace.selected = 0;
        workspace
    }

    fn all_parts() -> Vec<Part> {
        vec![
            Part::Text("hello".to_string()),
            Part::Thought {
                text: "thinking".to_string(),
                collapsed: true,
            },
            Part::Tool {
                icon: "terminal",
                label: "cargo test".to_string(),
                detail: Some("workspace".to_string()),
                done: true,
                failed: false,
                result: Some("ok".to_string()),
                duration_ms: Some(1500),
                started_at: None,
            },
            Part::Files {
                path: "src/main.rs".to_string(),
                added: 10,
                removed: 2,
                old_text: None,
                new_text: None,
                diff_expanded: false,
                undo_error: None,
                undone: false,
            },
            Part::Note {
                text: "renamed file".to_string(),
                undone: true,
            },
            Part::Approval {
                kicker: "Permission".to_string(),
                title: "Run command".to_string(),
                detail: "cargo publish".to_string(),
                request: Some(42),
                choices: vec![
                    ("Allow once".to_string(), PermissionChoice::Allow),
                    ("Deny".to_string(), PermissionChoice::Deny),
                ],
                grant_folder_choice: None,
                resolved: Some("Allowed".to_string()),
            },
            Part::Question {
                request: 7,
                index: 0,
                header: "Deploy".to_string(),
                question: "Which env?".to_string(),
                options: vec!["staging".to_string(), "prod".to_string()],
                multi_select: false,
                allow_free_text: true,
                secret: false,
                selected: vec!["staging".to_string()],
                resolved: Some("staging".to_string()),
            },
        ]
    }

    #[test]
    fn rich_parts_survive_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let storage = AppStorage::new(dir.path());
        let workspace = workspace_with_parts(all_parts());
        let expected = format!("{:?}", workspace.chats[0].messages[0].parts);

        storage.save(&workspace, &Settings::default()).unwrap();
        let (loaded, _) = storage.load();

        assert_eq!(format!("{:?}", loaded.chats[0].messages[0].parts), expected);
        assert_eq!(loaded.chats[0].session_id.as_deref(), Some("sess-1"));
    }

    #[test]
    fn tool_failure_survives_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let storage = AppStorage::new(dir.path());
        let workspace = workspace_with_parts(vec![Part::Tool {
            icon: "terminal",
            label: "cargo test".to_string(),
            detail: None,
            done: true,
            failed: true,
            result: Some("Run failed".to_string()),
            duration_ms: None,
            started_at: None,
        }]);

        storage.save(&workspace, &Settings::default()).unwrap();
        let (loaded, _) = storage.load();

        match &loaded.chats[0].messages[0].parts[0] {
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

    #[test]
    fn legacy_tool_part_without_failed_loads_as_success() {
        let dir = tempfile::tempdir().unwrap();
        let storage = AppStorage::new(dir.path());
        std::fs::create_dir_all(dir.path()).unwrap();
        std::fs::write(
            storage.path.clone(),
            r#"{
                "bots": [],
                "chats": [{
                    "id": 1,
                    "title": "Old",
                    "bot": null,
                    "members": [],
                    "messages": [{
                        "author": "bot",
                        "bot": null,
                        "text": "cargo test",
                        "parts": [{
                            "kind": "tool",
                            "icon": "terminal",
                            "label": "cargo test",
                            "detail": null,
                            "done": true,
                            "result": "ok"
                        }],
                        "time": "yesterday",
                        "stopped": false
                    }],
                    "preview": "cargo test",
                    "time": "yesterday"
                }],
                "tokens_used": 0,
                "tokens_cap": 100
            }"#,
        )
        .unwrap();

        let (loaded, _) = storage.load();

        match &loaded.chats[0].messages[0].parts[0] {
            Part::Tool { done, failed, .. } => {
                assert!(*done);
                assert!(!*failed);
            }
            other => panic!("expected a tool part, got {other:?}"),
        }
    }

    #[test]
    fn settings_survive_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let storage = AppStorage::new(dir.path());
        let settings = Settings {
            mode: Mode::Advanced,
            appearance: Some(crate::domain::model::AppearancePreference::Dark),
            disabled_runtimes: Vec::new(),
            default_runtime: "grok".to_string(),
            send_on_enter: false,
        };

        storage.save(&Workspace::empty(), &settings).unwrap();
        let (_, loaded) = storage.load();

        assert_eq!(loaded.mode, Mode::Advanced);
        assert_eq!(loaded.default_runtime, "grok");
        assert!(!loaded.send_on_enter);
    }

    #[test]
    fn legacy_file_without_settings_loads_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let storage = AppStorage::new(dir.path());
        std::fs::create_dir_all(dir.path()).unwrap();
        std::fs::write(
            storage.path.clone(),
            r#"{"bots": [], "chats": [], "tokens_used": 0, "tokens_cap": 100}"#,
        )
        .unwrap();

        let (_, settings) = storage.load();
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn unknown_tool_icon_falls_back_to_bot() {
        assert_eq!(icon_id("terminal"), "terminal");
        assert_eq!(icon_id("made-up"), "bot");
    }

    #[test]
    fn legacy_text_only_message_loads_as_single_text_part() {
        let dir = tempfile::tempdir().unwrap();
        let storage = AppStorage::new(dir.path());
        std::fs::create_dir_all(dir.path()).unwrap();
        std::fs::write(
            storage.path.clone(),
            r#"{
                "bots": [],
                "chats": [{
                    "id": 1,
                    "title": "Old",
                    "bot": null,
                    "members": [],
                    "messages": [{
                        "author": "bot",
                        "bot": null,
                        "text": "legacy line",
                        "time": "yesterday",
                        "stopped": false
                    }],
                    "preview": "legacy line",
                    "time": "yesterday"
                }],
                "tokens_used": 0,
                "tokens_cap": 100
            }"#,
        )
        .unwrap();

        let (workspace, _) = storage.load();
        let parts = &workspace.chats[0].messages[0].parts;
        assert_eq!(parts.len(), 1);
        assert!(matches!(parts[0], Part::Text(ref text) if text == "legacy line"));
    }

    #[test]
    fn empty_legacy_message_loads_without_parts() {
        let message = PersistedMessage {
            author: "you".to_string(),
            bot: None,
            text: String::new(),
            parts: None,
            time: "now".to_string(),
            duration_ms: None,
            activity_expanded: false,
            stopped: false,
        };
        let message = Message::from(message);
        assert!(message.parts.is_empty());
        assert_eq!(message.author, Author::You);
    }
}
