//! Desktop integration with the embedded Aucteris execution engine.

use crate::application::session::{self, Sessions};
use aucteris_core::{AucterisCore, CoreConfig};
use aucteris_harness::komet_proto::{
    ClaudeSandbox, PermissionChoice, RunRequest, SandboxLevel, SandboxOptions,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;

pub use crate::application::session::{Action, EngineEvent};

#[derive(Debug, Clone, Copy)]
pub struct AgentCli {
    pub id: &'static str,
    pub label: &'static str,
    pub executable: &'static str,
    pub provider: &'static str,
    pub icon: &'static str,
}

pub const SUPPORTED: [AgentCli; 6] = [
    AgentCli {
        id: "claude-code",
        label: "Claude Code",
        executable: "claude",
        provider: "Anthropic",
        icon: "claude-mark",
    },
    AgentCli {
        id: "codex",
        label: "Codex CLI",
        executable: "codex",
        provider: "OpenAI",
        icon: "openai-mark",
    },
    AgentCli {
        id: "opencode",
        label: "OpenCode",
        executable: "opencode",
        provider: "OpenCode",
        icon: "opencode-mark",
    },
    AgentCli {
        id: "gemini",
        label: "Gemini CLI",
        executable: "gemini",
        provider: "Google",
        icon: "gemini-mark",
    },
    AgentCli {
        id: "devin",
        label: "Devin CLI",
        executable: "devin",
        provider: "Devin",
        icon: "devin-mark",
    },
    AgentCli {
        id: "grok",
        label: "Grok CLI",
        executable: "grok",
        provider: "xAI",
        icon: "bot",
    },
];

#[derive(Debug, Clone, Copy)]
pub struct AgentModel {
    pub id: &'static str,
    pub runtime_id: &'static str,
    pub provider: &'static str,
    pub icon: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub badge: Option<&'static str>,
}

pub const SUPPORTED_MODELS: &[AgentModel] = &[
    // OpenCode models
    AgentModel {
        id: "opencode:adaptive",
        runtime_id: "opencode",
        provider: "OpenCode",
        icon: "opencode-mark",
        label: "OpenCode (Adaptive)",
        description: "OpenCode selects the best model per task",
        badge: Some("Auto"),
    },
    AgentModel {
        id: "opencode:claude-3-7-sonnet",
        runtime_id: "opencode",
        provider: "OpenCode",
        icon: "opencode-mark",
        label: "Claude 3.7 Sonnet",
        description: "Hybrid reasoning model via OpenCode",
        badge: Some("*1"),
    },
    AgentModel {
        id: "opencode:claude-3-5-sonnet",
        runtime_id: "opencode",
        provider: "OpenCode",
        icon: "opencode-mark",
        label: "Claude 3.5 Sonnet",
        description: "Industry-leading coding model via OpenCode",
        badge: None,
    },
    AgentModel {
        id: "opencode:gpt-4o",
        runtime_id: "opencode",
        provider: "OpenCode",
        icon: "opencode-mark",
        label: "GPT-4o",
        description: "Multimodal high-speed model via OpenCode",
        badge: None,
    },
    AgentModel {
        id: "opencode:o3-mini",
        runtime_id: "opencode",
        provider: "OpenCode",
        icon: "opencode-mark",
        label: "o3-mini",
        description: "High-reasoning coding model via OpenCode",
        badge: Some("*2"),
    },
    AgentModel {
        id: "opencode:deepseek-r1",
        runtime_id: "opencode",
        provider: "OpenCode",
        icon: "opencode-mark",
        label: "DeepSeek R1",
        description: "Open-weights reasoning model via OpenCode",
        badge: None,
    },
    // Anthropic / Claude Code models
    AgentModel {
        id: "claude-code:claude-3-7-sonnet",
        runtime_id: "claude-code",
        provider: "Anthropic",
        icon: "claude-mark",
        label: "Claude 3.7 Sonnet",
        description: "Most intelligent hybrid reasoning model",
        badge: Some("*1"),
    },
    AgentModel {
        id: "claude-code:claude-3-5-sonnet",
        runtime_id: "claude-code",
        provider: "Anthropic",
        icon: "claude-mark",
        label: "Claude 3.5 Sonnet",
        description: "Industry-leading intelligence and coding performance",
        badge: None,
    },
    AgentModel {
        id: "claude-code:claude-3-5-haiku",
        runtime_id: "claude-code",
        provider: "Anthropic",
        icon: "claude-mark",
        label: "Claude 3.5 Haiku",
        description: "Fastest and most responsive model",
        badge: Some("Fast"),
    },
    AgentModel {
        id: "claude-code:claude-3-opus",
        runtime_id: "claude-code",
        provider: "Anthropic",
        icon: "claude-mark",
        label: "Claude 3 Opus",
        description: "Powerful model for complex analysis",
        badge: None,
    },
    // OpenAI / Codex models
    AgentModel {
        id: "codex:gpt-5.6-terra",
        runtime_id: "codex",
        provider: "OpenAI",
        icon: "openai-mark",
        label: "GPT-5.6-Terra",
        description: "Balanced agentic coding model for everyday work",
        badge: Some("*1"),
    },
    AgentModel {
        id: "codex:gpt-5.6-luna",
        runtime_id: "codex",
        provider: "OpenAI",
        icon: "openai-mark",
        label: "GPT-5.6-Luna",
        description: "Fast and affordable agentic coding model",
        badge: None,
    },
    AgentModel {
        id: "codex:gpt-5.5",
        runtime_id: "codex",
        provider: "OpenAI",
        icon: "openai-mark",
        label: "GPT-5.5",
        description: "Frontier model for complex coding and research",
        badge: None,
    },
    AgentModel {
        id: "codex:o3-mini",
        runtime_id: "codex",
        provider: "OpenAI",
        icon: "openai-mark",
        label: "o3-mini",
        description: "High reasoning STEM and coding model",
        badge: Some("*2"),
    },
    AgentModel {
        id: "codex:o1",
        runtime_id: "codex",
        provider: "OpenAI",
        icon: "openai-mark",
        label: "o1",
        description: "Frontier reasoning model for hard problems",
        badge: None,
    },
    AgentModel {
        id: "codex:gpt-4o",
        runtime_id: "codex",
        provider: "OpenAI",
        icon: "openai-mark",
        label: "GPT-4o",
        description: "Multimodal flagship model",
        badge: None,
    },
    // Google / Gemini models
    AgentModel {
        id: "gemini:gemini-2.5-pro",
        runtime_id: "gemini",
        provider: "Google",
        icon: "gemini-mark",
        label: "Gemini 2.5 Pro",
        description: "Next-gen multimodal coding and reasoning",
        badge: Some("*1"),
    },
    AgentModel {
        id: "gemini:gemini-2.0-flash",
        runtime_id: "gemini",
        provider: "Google",
        icon: "gemini-mark",
        label: "Gemini 2.0 Flash",
        description: "Ultra-fast lightweight coding model",
        badge: Some("Fast"),
    },
    AgentModel {
        id: "gemini:gemini-2.0-flash-thinking",
        runtime_id: "gemini",
        provider: "Google",
        icon: "gemini-mark",
        label: "Gemini 2.0 Flash Thinking",
        description: "Dedicated reasoning mode for complex logic",
        badge: Some("*2"),
    },
    AgentModel {
        id: "gemini:gemini-1.5-pro",
        runtime_id: "gemini",
        provider: "Google",
        icon: "gemini-mark",
        label: "Gemini 1.5 Pro",
        description: "2M context window for massive repos",
        badge: None,
    },
    // Devin models (as in Komet)
    AgentModel {
        id: "devin:swe-2-flash-medium",
        runtime_id: "devin",
        provider: "Devin",
        icon: "devin-mark",
        label: "SWE-2 Flash Medium",
        description: "Devin's fast autonomous coding model",
        badge: Some("*2"),
    },
    AgentModel {
        id: "devin:swe-1-7-medium",
        runtime_id: "devin",
        provider: "Devin",
        icon: "devin-mark",
        label: "SWE-1.7 Medium",
        description: "Devin's deep autonomous coding model",
        badge: Some("*1"),
    },
];

/// One model offered in the picker: the static catalog merged with whatever
/// the installed CLI reported at startup.
#[derive(Debug, Clone)]
pub struct ModelChoice {
    pub id: String,
    pub provider: String,
    pub icon: &'static str,
    pub label: String,
    pub description: String,
    pub badge: Option<String>,
}

pub struct Engine {
    core: AucterisCore,
    data_dir: PathBuf,
    available: Vec<AgentCli>,
    sessions: Sessions,
    /// Models each CLI reported, keyed by runtime base id.
    live_models: Mutex<HashMap<String, Vec<ModelChoice>>>,
}

pub struct BotRunConfig {
    pub bot_id: u32,
    pub runtime: String,
    pub folder: String,
    pub allowed_folders: Vec<String>,
    pub instructions: String,
    /// Harness-native session id from the chat's previous turn.
    pub resume: Option<String>,
    pub sandbox: SandboxLevel,
}

impl Engine {
    /// Persistent scoped directories are supported by Claude's native
    /// additionalDirectories and Gemini's bubblewrap mount plan.
    pub fn supports_scoped_folders(runtime: &str) -> bool {
        matches!(Self::runtime_base(runtime), "claude-code" | "gemini")
    }
    pub fn open() -> anyhow::Result<Self> {
        let data_dir = crate::infrastructure::storage::AppStorage::data_dir();
        let core = AucterisCore::open(CoreConfig::new(&data_dir))?;
        let available = SUPPORTED
            .into_iter()
            .filter(|cli| executable_on_path(cli.executable))
            .collect();
        Ok(Self {
            core,
            data_dir,
            available,
            sessions: Sessions::new()?,
            live_models: Mutex::new(HashMap::new()),
        })
    }

    pub fn available_runtimes(&self) -> &[AgentCli] {
        &self.available
    }

    #[allow(dead_code)]
    pub fn runtime_cli(id: &str) -> Option<&'static AgentCli> {
        SUPPORTED.iter().find(|cli| cli.id == id)
    }

    #[allow(dead_code)]
    pub fn supported_models() -> &'static [AgentModel] {
        SUPPORTED_MODELS
    }

    /// Base runtime id of a stored `runtime` value ("codex:gpt-5.5" → "codex").
    pub fn runtime_base(id: &str) -> &str {
        id.split_once(':').map(|(base, _)| base).unwrap_or(id)
    }

    /// Models one runtime exposes: what its CLI reported first, then the
    /// built-in catalog for whatever it did not.
    pub fn models_for(&self, runtime_id: &str) -> Vec<ModelChoice> {
        let base = Self::runtime_base(runtime_id);
        let mut out = self
            .live_models
            .lock()
            .ok()
            .and_then(|live| live.get(base).cloned())
            .unwrap_or_default();
        for model in SUPPORTED_MODELS.iter().filter(|m| m.runtime_id == base) {
            if out.iter().any(|choice| choice.id == model.id) {
                continue;
            }
            out.push(ModelChoice {
                id: model.id.to_string(),
                provider: model.provider.to_string(),
                icon: model.icon,
                label: model.label.to_string(),
                description: model.description.to_string(),
                badge: model.badge.map(String::from),
            });
        }
        out
    }

    /// Ask every installed CLI which models it accepts. Blocks, so call it
    /// from a background thread. Returns how many models were discovered.
    pub fn discover_models(&self) -> usize {
        let mut discovered = 0;
        for cli in &self.available {
            let Some(harness) = session::komet_backend(cli.id) else {
                continue;
            };
            let Ok(models) = self.sessions.models(harness.as_ref()) else {
                continue;
            };
            let choices = models
                .into_iter()
                .map(|model| {
                    let label = if model.label.trim().is_empty() {
                        model.id.clone()
                    } else {
                        model.label
                    };
                    ModelChoice {
                        id: format!("{}:{}", cli.id, model.id),
                        provider: cli.provider.to_string(),
                        icon: cli.icon,
                        label,
                        description: model.description.unwrap_or_default(),
                        badge: None,
                    }
                })
                .collect::<Vec<_>>();
            discovered += choices.len();
            if let Ok(mut live) = self.live_models.lock() {
                live.insert(cli.id.to_string(), choices);
            }
        }
        discovered
    }

    /// Whether `id` (bare runtime or `runtime:model`) resolves to a supported
    /// CLI whose executable is on PATH. Model ids never add requirements of
    /// their own — the CLI decides which models it accepts.
    pub fn runtime_usable(id: &str) -> bool {
        let base = Self::runtime_base(id);
        SUPPORTED
            .iter()
            .find(|cli| cli.id == base)
            .map(|cli| executable_on_path(cli.executable))
            .unwrap_or(false)
    }

    pub fn runtime_icon(id: &str) -> &'static str {
        if let Some(model) = SUPPORTED_MODELS.iter().find(|m| m.id == id) {
            return model.icon;
        }
        SUPPORTED
            .iter()
            .find(|cli| cli.id == Self::runtime_base(id))
            .map(|cli| cli.icon)
            .unwrap_or("bot")
    }

    pub fn runtime_label(&self, id: &str) -> String {
        if let Some((base, _)) = id.split_once(':')
            && let Ok(live) = self.live_models.lock()
            && let Some(choice) = live
                .get(base)
                .and_then(|list| list.iter().find(|choice| choice.id == id))
        {
            return format!("{} ({})", choice.label, choice.provider);
        }
        if let Some(model) = SUPPORTED_MODELS.iter().find(|m| m.id == id) {
            return format!("{} ({})", model.label, model.provider);
        }
        SUPPORTED
            .iter()
            .find(|cli| cli.id == id)
            .map(|cli| cli.label.to_string())
            .unwrap_or_else(|| id.to_string())
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    pub fn run_prompt(
        &self,
        chat_id: u32,
        bot: BotRunConfig,
        prompt: String,
        events: UnboundedSender<EngineEvent>,
    ) {
        if let Err(error) = self.run_prompt_inner(chat_id, &bot, &prompt, events.clone()) {
            let _ = events.send(EngineEvent::Finished(Err(format!("{error:#}"))));
        }
    }

    fn run_prompt_inner(
        &self,
        chat_id: u32,
        bot: &BotRunConfig,
        prompt: &str,
        events: UnboundedSender<EngineEvent>,
    ) -> anyhow::Result<()> {
        let adapter_name = Self::runtime_base(&bot.runtime);
        if !SUPPORTED.iter().any(|c| c.id == adapter_name) {
            anyhow::bail!(
                "unsupported runtime {:?}; supported: {:?}",
                adapter_name,
                SUPPORTED.iter().map(|c| c.id).collect::<Vec<_>>()
            );
        }
        let full_prompt = compose_prompt(&bot.instructions, prompt);
        let workspace = crate::infrastructure::storage::expand_home(&bot.folder);
        let assigned_workspace = crate::infrastructure::storage::AppStorage::bot_workspace_path(
            &self.data_dir,
            bot.bot_id,
        );
        let is_private_workspace = workspace == assigned_workspace;
        std::fs::create_dir_all(&workspace)?;
        #[cfg(unix)]
        if is_private_workspace {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&workspace, std::fs::Permissions::from_mode(0o700))?;
            if let Some(parent) = workspace.parent() {
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
            }
        }

        // One runtime path for every CLI: komet driver when it exists,
        // otherwise the core adapter wrapped as a session backend (gemini,
        // mock) — same events, same controls, honestly degraded caps.
        let Some(harness) = session::backend(
            &bot.runtime,
            self.core.runtimes(),
            if bot.sandbox == SandboxLevel::WorkspaceWrite {
                bot.allowed_folders.iter().map(PathBuf::from).collect()
            } else {
                Vec::new()
            },
        ) else {
            anyhow::bail!("no runtime backend for {:?}", bot.runtime);
        };
        let mut sandbox_options = SandboxOptions::from_level(bot.sandbox);
        if Self::runtime_base(&bot.runtime) == "claude-code"
            && bot.sandbox == SandboxLevel::WorkspaceWrite
        {
            let claude = sandbox_options
                .claude
                .get_or_insert_with(ClaudeSandbox::default);
            claude.additional_directories = bot.allowed_folders.clone();
        }
        let request = RunRequest {
            prompt: full_prompt,
            harness: None,
            model: bot
                .runtime
                .split_once(':')
                .map(|(_, model)| model.to_string()),
            reasoning: None,
            model_options: Default::default(),
            cwd: workspace.display().to_string(),
            sandbox: bot.sandbox,
            sandbox_options: Some(sandbox_options),
            // Full access means exactly that; every other level keeps the
            // harness in its approval mode so the card can gate the call.
            auto_approve: bot.sandbox == SandboxLevel::DangerFullAccess,
            resume: bot.resume.clone(),
            attachments: Vec::new(),
            worktree: None,
            permission_timeout_ms: None,
            use_mcp: false,
            mcp: None,
            mcp_external: Vec::new(),
        };
        self.sessions.start(chat_id, harness, request, events);
        Ok(())
    }

    /// Interrupt this chat's run: protocol-level interrupt on komet
    /// sessions, process-tree kill on CLI backends.
    pub fn stop(&self, chat_id: u32) {
        self.sessions.stop(chat_id);
    }

    pub fn steer(&self, chat_id: u32, prompt: String) -> bool {
        self.sessions.steer(chat_id, prompt)
    }

    pub fn can_steer(&self, chat_id: u32) -> bool {
        self.sessions.can_steer(chat_id)
    }

    /// Answer a permission card. `false` when the request is already gone
    /// (run ended, or the harness stopped waiting).
    pub fn answer_permission(&self, request: u64, choice: PermissionChoice) -> bool {
        self.sessions.answer_permission(request, choice)
    }

    /// Answer one question of a pending set. The harness gets the whole set
    /// once the last question is answered.
    pub fn answer_question(
        &self,
        request: u64,
        index: usize,
        labels: Vec<String>,
        free_text: Option<String>,
    ) -> bool {
        self.sessions
            .answer_question(request, index, labels, free_text)
    }
}

/// The prompt the agent receives: standing instructions, then the ask.
fn compose_prompt(instructions: &str, prompt: &str) -> String {
    let guidance = "Aucteris is a general-purpose assistant. Answer the user's request directly in the conversation. For comparisons, tables, JSON, and CSV, include the actual structured result in your answer instead of only creating files or reporting their paths. Do not create or edit files or run commands unless the user explicitly requests an artifact or an action that requires it. When returning HTML in a fenced html block, tell the user to use the Preview control provided with the block.";
    let instructions = instructions.trim();
    if instructions.is_empty() {
        format!("{guidance}\n\nUser request:\n{prompt}")
    } else {
        format!("{guidance}\n\nBot instructions:\n{instructions}\n\nUser request:\n{prompt}")
    }
}

pub fn executable_on_path(executable: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(executable);
        candidate.is_file()
    })
}
