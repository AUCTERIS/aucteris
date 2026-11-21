//! Runtime port: engine-owned contracts implemented by built-in or external harnesses.

use crate::infrastructure::sandbox::{SandboxMounts, SandboxPlan};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

/// What an adapter can actually enforce/expose. Reported honestly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterCapabilities {
    /// Can Aucteris intercept/mediate every tool call? (level 3, doc 04 §4)
    pub mediated_tools: bool,
    /// Can the engine cancel a running session and verify termination?
    pub cancellable: bool,
    /// Does the runtime expose a structured event stream?
    pub streaming_events: bool,
    /// Can the runtime be confined to a workspace by the adapter itself?
    pub workspace_isolation: IsolationLevel,
    /// Can the runtime report resource usage?
    pub resource_accounting: bool,
    /// Known limitations, surfaced in reports.
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationLevel {
    /// Relies on OS-level confinement outside the runtime (our problem).
    ExternalOnly,
    /// Runtime offers its own permission/sandbox system (best-effort).
    RuntimeAssisted,
    /// Runtime enforces isolation itself (rare).
    RuntimeEnforced,
}

/// Normalized view of one runtime event. Raw JSONL is kept as evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeEvent {
    SessionStarted {
        model: Option<String>,
    },
    AssistantMessage {
        text: String,
    },
    ToolCall {
        name: String,
    },
    ToolResult {
        ok: bool,
    },
    FinalResult {
        text: String,
    },
    Error {
        message: String,
    },
    Usage {
        input_tokens: u64,
        output_tokens: u64,
    },
    Other,
}

/// Everything an adapter needs to run one attempt.
#[derive(Clone)]
pub struct RunRequest {
    pub workspace: PathBuf,
    pub prompt: String,
    pub events_path: PathBuf,
    pub max_turns: u32,
    /// If set, the engine writes the child PID here immediately after
    /// spawn — survives an engine kill mid-run (orphan detection).
    pub pid_file: Option<PathBuf>,
    /// Engine-level sandbox plan. When bubblewrap is available the
    /// runtime is spawned inside it (fs confinement); otherwise the
    /// engine degrades to runtime-only isolation and records it.
    pub sandbox: Option<SandboxPlan>,
    /// Attempt id used for the per-attempt cgroup (resource limits +
    /// authoritative subtree kill). None disables cgroup supervision.
    pub cgroup_name: Option<String>,
    /// When set, the engine spawns a local CONNECT proxy restricted to
    /// these domains and injects *_PROXY env vars into the child.
    /// Best-effort mediation — runtimes ignoring proxy env bypass it.
    pub egress_filter: Option<Vec<String>>,
    /// When set, the engine injects an MCP server (mission `memory`).
    /// Only passed to adapters declaring `supports_mcp()`; each adapter
    /// materializes its native config mechanism. The canonical
    /// `config_path` uses the claude `{"mcpServers": {...}}` format as
    /// the interchange document.
    pub mcp: Option<McpPlan>,
    /// Optional live normalized event sink. CLI adapters call it as events
    /// arrive; custom adapters may emit their own normalized events here.
    pub event_sink: Option<Arc<dyn Fn(RuntimeEvent) + Send + Sync>>,
    /// Harness-native session id to resume (persistent agent sessions).
    /// `None` starts a fresh session. Adapters without session support
    /// ignore it — recorded, never silently dropped.
    pub resume_session: Option<String>,
    /// Per-tool approval bridge. When set, the adapter MUST route every
    /// tool-permission request through it and honor the decision; when
    /// `None` the adapter applies its own policy (usually auto-approve)
    /// and records that in `RunOutcome`/limitations.
    pub interactions: Option<ApprovalHandler>,
    /// Operator steer prompts for this attempt, drained by the adapter at
    /// its steering boundary. `None` = steering unsupported/unwired.
    pub steering: Option<SteerInbox>,
}

/// One tool-permission request the engine needs a human/operator decision on.
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    /// Engine-generated id; the resolver echoes it back.
    pub request_id: String,
    /// Runtime-reported permission kind (e.g. "bash", "edit", "fetch").
    pub kind: String,
    /// Human-readable summary of what is being approved.
    pub summary: String,
    /// Offered choices, in order (e.g. ["allow", "allow_always", "deny"]).
    pub choices: Vec<String>,
    /// Runtime tool-call id the approval gates, when known.
    pub tool_call_id: Option<String>,
}

/// The operator's answer to an [`ApprovalRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Allow,
    AllowAlways,
    Deny,
}

impl ApprovalDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::AllowAlways => "allow_always",
            Self::Deny => "deny",
        }
    }
}

/// Bridge the adapter calls (possibly from an async context) to whatever
/// resolves approvals — a file gate, a durable workflow, a UI channel.
/// The returned receiver yields exactly one decision; a dropped sender
/// means "unanswered" and the adapter must degrade to Deny, never hang.
pub type ApprovalHandler =
    Arc<dyn Fn(ApprovalRequest) -> tokio::sync::oneshot::Receiver<ApprovalDecision> + Send + Sync>;

/// Cloneable inbox of steer prompts for one attempt. The sender side is
/// held by the engine (`AucterisCore::steer`); the receiver is drained by
/// the adapter's steering thread.
#[derive(Clone)]
pub struct SteerInbox {
    rx: Arc<Mutex<std::sync::mpsc::Receiver<String>>>,
}

impl SteerInbox {
    /// Create an inbox plus the sender the engine keeps.
    pub fn pair() -> (Self, std::sync::mpsc::Sender<String>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (
            Self {
                rx: Arc::new(Mutex::new(rx)),
            },
            tx,
        )
    }

    /// Next queued steer prompt, or `None` when the channel is empty or
    /// the sender was dropped. Non-blocking by contract: adapters poll it
    /// at their steering boundary.
    pub fn try_recv(&self) -> Option<String> {
        self.rx
            .lock()
            .ok()?
            .try_recv()
            .ok()
            .filter(|text| !text.trim().is_empty())
    }

    /// Block up to `dur` for the next steer prompt. Intended for a
    /// dedicated drain thread inside an adapter — never call from an
    /// async runtime worker. Distinguishes timeout from sender drop so
    /// the drain loop can exit instead of spinning forever.
    pub fn recv_timeout(
        &self,
        dur: std::time::Duration,
    ) -> Result<String, std::sync::mpsc::RecvTimeoutError> {
        let rx = self
            .rx
            .lock()
            .map_err(|_| std::sync::mpsc::RecvTimeoutError::Disconnected)?;
        match rx.recv_timeout(dur) {
            Ok(text) if !text.trim().is_empty() => Ok(text),
            Ok(_) => Err(std::sync::mpsc::RecvTimeoutError::Timeout),
            Err(e) => Err(e),
        }
    }
}

/// Engine-provided MCP server injection for one attempt.
#[derive(Debug, Clone)]
pub struct McpPlan {
    /// Canonical `{"mcpServers": {...}}` JSON written inside the task
    /// workspace — also the file claude consumes via `--mcp-config`.
    pub config_path: PathBuf,
    /// Server name inside the config (e.g. "aucteris-memory").
    pub name: String,
    /// Resolved absolute server command (bound into the sandbox).
    pub command: String,
    /// Env vars for the server process.
    pub env: Vec<(String, String)>,
}

/// What the engine observed — never what the agent claims to have done.
#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub exit_code: i32,
    pub tool_calls: u32,
    pub agent_assertion: Option<String>,
    pub usage: Option<(u64, u64)>,
    /// PID of the supervised process (== pgid on Unix).
    pub pid: Option<u32>,
    /// True when the attempt ran inside a delegated cgroup v2 with
    /// memory/CPU/pids limits — recorded, never assumed.
    pub cgroup_limited: bool,
    /// (allowed, denied) proxy connections when egress filtering was
    /// active for this attempt.
    pub egress: Option<(u64, u64)>,
    /// Harness-native session id the runtime reported (persistent
    /// sessions). `None` for runtimes without session support.
    pub session_id: Option<String>,
}

/// The adapter contract (doc 04 §3). Implementations must not claim
/// capabilities the underlying runtime cannot enforce.
pub trait RuntimeAdapter: Send + Sync {
    /// Stable identifier used in mission specs and attempt records.
    fn name(&self) -> &'static str;

    /// Declared capabilities — read by the engine, shown in reports.
    fn capabilities(&self) -> AdapterCapabilities;

    /// Run one attempt: spawn/supervise the runtime, stream raw events
    /// to `req.events_path`, return observed outcome.
    fn run(&self, req: RunRequest) -> Result<RunOutcome>;

    /// Host mounts the runtime needs inside the engine sandbox: install
    /// dirs, config/credential dirs, protected files. Everything else
    /// under $HOME stays invisible. Default: none.
    fn sandbox_mounts(&self) -> SandboxMounts {
        SandboxMounts::default()
    }

    /// Domains this runtime legitimately needs when mission egress
    /// filtering is on (`egress_filter: true`). Suffix-matched by the
    /// engine proxy. Empty = the runtime declares no network needs —
    /// filtering then denies everything it attempts through the proxy.
    /// This is a declaration, not enforcement: runtimes ignoring proxy
    /// env vars bypass it (see src/egress.rs limitations).
    fn egress_domains(&self) -> Vec<String> {
        vec![]
    }

    /// Can the adapter consume an engine-provided MCP servers config
    /// (`RunRequest::mcp_config_path`)? Default false — adapters that
    /// cannot inject MCP servers must not silently ignore the request;
    /// the executor records the limitation instead.
    fn supports_mcp(&self) -> bool {
        false
    }
}

/// Runtime adapter collection. Applications can add their own harness
/// adapters while keeping Aucteris' execution, isolation, and evidence path.
#[derive(Clone)]
pub struct RuntimeRegistry {
    adapters: HashMap<String, Arc<dyn RuntimeAdapter>>,
}

impl RuntimeRegistry {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    /// Register (or replace) an adapter under its stable runtime identifier.
    pub fn register<A: RuntimeAdapter + 'static>(&mut self, adapter: A) {
        self.register_shared(Arc::new(adapter));
    }

    pub fn register_shared(&mut self, adapter: Arc<dyn RuntimeAdapter>) {
        self.adapters.insert(adapter.name().to_string(), adapter);
    }

    /// Unknown names are explicit errors; there is no silent fallback.
    pub fn get(&self, name: &str) -> Result<Arc<dyn RuntimeAdapter>> {
        self.adapters.get(name).cloned().ok_or_else(|| {
            anyhow::anyhow!(
                "unknown runtime adapter {:?}; registered: {:?}",
                name,
                self.registered()
            )
        })
    }

    pub fn registered(&self) -> Vec<String> {
        let mut names: Vec<_> = self.adapters.keys().cloned().collect();
        names.sort();
        names
    }
}
