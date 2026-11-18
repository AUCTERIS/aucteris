//! Domain model — Vertical Slice 01 subset.
//!
//! Implements only the entities the slice touches (see
//! docs/architecture/VERTICAL-SLICE-01.md §7): Mission, Task, AgentProfile
//! (minimal), AgentInstance, RuntimeSession, Operation, Attempt, Artifact,
//! Evidence. Team, Capability tokens, and PolicyDecision are stubbed.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------- Lifecycle enums (kept separate per doc 02 §5) ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionStatus {
    Pending,
    Running,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Blocked,
    /// Engine restarted while work was in flight; effect state unknown.
    Unknown,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Running,
    Succeeded,
    Failed,
    /// Attempt was interrupted (engine crash/kill); outcome not established.
    Unknown,
    Cancelled,
}

// ---------- Mission input (what a client submits) ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionSpec {
    pub objective: String,
    #[serde(default)]
    pub constraints: Vec<String>,
    /// Engine-level sandbox (bubblewrap fs confinement) for all tasks.
    /// Default ON: agents are untrusted by principle (P1). When bwrap is
    /// unavailable the engine degrades and records it — never silently.
    #[serde(default = "default_sandbox")]
    pub sandbox: bool,
    /// Engine-level network egress for attempts (bubblewrap
    /// `--unshare-net` when false). Only enforced inside the sandbox —
    /// if bwrap is unavailable the runtime keeps its own network and
    /// the degradation is recorded. Default ON: agent CLIs need API
    /// egress; set false for missions that must not touch the network.
    #[serde(default = "default_allow_network")]
    pub allow_network: bool,
    /// Route attempt traffic through the engine's local allowlist proxy
    /// (domains declared by the runtime adapter). Best-effort mediation,
    /// NOT enforcement — runtimes ignoring *_PROXY env vars bypass it.
    /// Only meaningful with allow_network: true. Default OFF.
    #[serde(default)]
    pub egress_filter: bool,
    /// Give every task's agent persistent memory tools via an MCP
    /// server (aucteris-memory). Opt-in per mission: memory is shared
    /// state, so it is never silently enabled. Requires an adapter
    /// that supports MCP injection and the `aucteris-memory` binary
    /// (AUCTERIS_MEMORY_BIN or PATH). Default OFF.
    #[serde(default)]
    pub memory: bool,
    /// Namespace agents should use for mission memories (prompt hint).
    /// Defaults to the mission id. Set this to match a `[[targets]]`
    /// namespace in the aucteris-memory sync config when using
    /// `sync_memory` — dynamic mission ids cannot be pre-registered.
    #[serde(default)]
    pub memory_namespace: Option<String>,
    /// After the mission ends, push `<mission>/memory` to Memexa via
    /// `aucteris-memory sync push`. Requires `memory: true`, the
    /// aucteris-memory binary, and `AUCTERIS_MEMORY_SYNC_CONFIG`
    /// pointing at a sync.toml whose [[targets]] cover
    /// `memory_namespace`. Failures are recorded as evidence, never
    /// fail the mission. Default OFF.
    #[serde(default)]
    pub sync_memory: bool,
    pub tasks: Vec<TaskSpec>,
}

fn default_sandbox() -> bool {
    true
}

fn default_allow_network() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    pub id: String,
    pub title: String,
    /// Prompt handed to the agent runtime.
    pub prompt: String,
    /// Command run by Aucteris itself (not the agent) to verify the task.
    /// Runs via `sh -c` inside the mission workspace.
    pub verify_cmd: String,
    /// Task ids that must complete first (ordered execution for slice 01).
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Runtime adapter name (see runtime::registered()).
    #[serde(default = "default_runtime")]
    pub runtime: String,
    /// Run this task in an isolated sub-workspace
    /// (`<mission>/iso/<task-id>/`): the sandbox binds only that dir,
    /// so the agent cannot see sibling task files. Shared tasks still
    /// see the whole tree including `iso/` — isolation is
    /// one-directional (restricted view, not hidden outputs).
    #[serde(default)]
    pub isolated: bool,
}

fn default_runtime() -> String {
    "claude-code".into()
}

// ---------- Persisted state ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mission {
    pub id: String,
    pub objective: String,
    pub constraints: Vec<String>,
    pub tasks: Vec<TaskSpec>,
    pub status: MissionStatus,
    pub created_at: String,
    pub workspace: PathBuf,
    /// Whether attempts run inside the engine sandbox (bubblewrap).
    /// Recorded at creation; old missions default to false.
    #[serde(default)]
    pub sandbox: bool,
    /// Whether attempts get network egress inside the sandbox.
    /// Recorded at creation; old missions default to true.
    #[serde(default = "default_allow_network")]
    pub allow_network: bool,
    /// Whether attempts route through the engine egress allowlist proxy.
    #[serde(default)]
    pub egress_filter: bool,
    /// Whether attempts get persistent memory tools (see MissionSpec).
    /// Recorded at creation; old missions default to false.
    #[serde(default)]
    pub memory: bool,
    /// Memory namespace hint + post-mission Memexa sync (see MissionSpec).
    #[serde(default)]
    pub memory_namespace: Option<String>,
    #[serde(default)]
    pub sync_memory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRun {
    pub task_id: String,
    pub status: TaskStatus,
    pub attempts: Vec<String>, // attempt ids, oldest first
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attempt {
    pub id: String,
    pub task_id: String,
    pub runtime: String, // e.g. "claude-code"
    pub status: AttemptStatus,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub exit_code: Option<i32>,
    /// Path to JSONL stream of raw runtime events (assertions live here).
    pub events_path: PathBuf,
    /// What the agent *claims* (unverified).
    pub agent_assertion: Option<String>,
    /// Count of tool calls observed in the event stream.
    pub tool_calls: u32,
    /// Isolation level declared by the adapter at attempt time.
    #[serde(default)]
    pub isolation: Option<String>,
    /// Limitations declared by the adapter (shown in reports).
    #[serde(default)]
    pub limitations: Vec<String>,
    /// PID of the spawned runtime process (== its process-group id on
    /// Unix, since we spawn with process_group(0)). Used to detect and
    /// kill orphans after an engine restart.
    #[serde(default)]
    pub pid: Option<u32>,
}

/// Evidence categories per doc 09 §2 — the distinction is the product.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Evidence {
    /// A claim made by the agent; never trusted as fact.
    AgentAssertion { attempt_id: String, text: String },
    /// What the runtime/process observably did (exit code, events, liveness).
    RuntimeObservation { attempt_id: String, detail: String },
    /// A check executed by Aucteris itself (verify_cmd, file inspection).
    VerificationResult {
        task_id: String,
        verify_cmd: String,
        exit_code: i32,
        output_tail: String,
    },
    /// Engine lifecycle note (recovery, cancellation, unknown marking).
    SystemObservation { detail: String },
}
