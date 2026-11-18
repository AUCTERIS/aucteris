//! Desktop integration with the embedded Aucteris execution engine.

use aucteris_core::{
    AucterisCore, CoreConfig, ExecutionEvent,
    domain::{MissionSpec, MissionStatus, TaskSpec},
};
use std::{
    path::PathBuf,
    sync::{Mutex, mpsc::Sender},
};

#[derive(Debug, Clone, Copy)]
pub struct AgentCli {
    pub id: &'static str,
    pub label: &'static str,
    executable: &'static str,
}

const SUPPORTED: [AgentCli; 4] = [
    AgentCli {
        id: "claude-code",
        label: "Claude Code",
        executable: "claude",
    },
    AgentCli {
        id: "codex",
        label: "Codex CLI",
        executable: "codex",
    },
    AgentCli {
        id: "opencode",
        label: "OpenCode",
        executable: "opencode",
    },
    AgentCli {
        id: "gemini",
        label: "Gemini CLI",
        executable: "gemini",
    },
];

#[derive(Debug)]
pub enum EngineEvent {
    Runtime(ExecutionEvent),
    Finished(Result<String, String>),
}

#[derive(Debug, Clone)]
pub enum Action {
    Intent(String),
    Say(String),
    Tool { label: String, activity: String },
    Settle(String),
    Error(String),
    Usage { input: u64, output: u64 },
}

pub fn tool_icon(name: &str) -> &'static str {
    let normalized = name.to_ascii_lowercase();
    if normalized.contains("bash")
        || normalized.contains("command")
        || normalized.contains("terminal")
    {
        "terminal"
    } else if normalized.contains("read")
        || normalized.contains("file")
        || normalized.contains("edit")
        || normalized.contains("write")
    {
        "file"
    } else if normalized.contains("search")
        || normalized.contains("grep")
        || normalized.contains("glob")
    {
        "search"
    } else if normalized.contains("web")
        || normalized.contains("fetch")
        || normalized.contains("browser")
    {
        "globe"
    } else {
        "bot"
    }
}

pub struct Engine {
    core: AucterisCore,
    data_dir: PathBuf,
    available: Vec<AgentCli>,
    run_lock: Mutex<()>,
}

pub struct BotRunConfig {
    pub id: u32,
    pub name: String,
    pub runtime: String,
    pub folder: String,
    pub instructions: String,
}

impl Engine {
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
            run_lock: Mutex::new(()),
        })
    }

    pub fn available_runtimes(&self) -> &[AgentCli] {
        &self.available
    }

    pub fn runtime_label(id: &str) -> String {
        SUPPORTED
            .iter()
            .find(|cli| cli.id == id)
            .map(|cli| cli.label.to_string())
            .unwrap_or_else(|| id.to_string())
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    pub fn run_prompt(&self, bot: BotRunConfig, prompt: String, events: Sender<EngineEvent>) {
        let result = self.run_prompt_inner(&bot, &prompt, &events);
        let _ = events.send(EngineEvent::Finished(
            result
                .map(|mission_id| mission_id)
                .map_err(|error| format!("{error:#}")),
        ));
    }

    fn run_prompt_inner(
        &self,
        bot: &BotRunConfig,
        prompt: &str,
        events: &Sender<EngineEvent>,
    ) -> anyhow::Result<String> {
        // Cancellation uses a process-global core signal. Serialize runs so
        // a stopped mission cannot affect the next prompt.
        let _run_guard = self
            .run_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("agent engine run lock was poisoned"))?;
        let full_prompt = if bot.instructions.trim().is_empty() {
            prompt.to_string()
        } else {
            format!("{}\n\nUser request:\n{}", bot.instructions.trim(), prompt)
        };
        let spec = MissionSpec {
            objective: format!("{}: {}", bot.name, prompt),
            constraints: Vec::new(),
            sandbox: true,
            allow_network: true,
            egress_filter: false,
            memory: false,
            memory_namespace: None,
            sync_memory: false,
            tasks: vec![TaskSpec {
                id: format!("bot-{}", bot.id),
                title: bot.name.clone(),
                prompt: full_prompt,
                verify_cmd: "true".to_string(),
                depends_on: Vec::new(),
                runtime: bot.runtime.clone(),
                isolated: false,
            }],
        };
        let workspace = crate::infrastructure::storage::expand_home(&bot.folder);
        let mission = self.core.submit_in_workspace(&spec, workspace)?;
        self.core.run_with_events(&mission.id, {
            let events = events.clone();
            move |event| {
                let _ = events.send(EngineEvent::Runtime(event));
            }
        })?;
        let finished = self.core.mission(&mission.id)?;
        if finished.status != MissionStatus::Completed {
            anyhow::bail!(
                "mission ended with status {:?}; see core evidence for details",
                finished.status
            );
        }
        Ok(mission.id)
    }

    pub fn cancel_active(&self) -> usize {
        self.core.cancel_active()
    }
}

fn executable_on_path(executable: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(executable);
        candidate.is_file()
    })
}
