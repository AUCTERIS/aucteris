//! [`HarnessMissionAdapter`]: a core [`RuntimeAdapter`] backed by a komet
//! [`Harness`] — the mission side of the unified runtime layer (ADR-0001).
//!
//! Missions executed through this adapter get what raw process adapters
//! cannot offer: harness-native session ids (persisted on the attempt and
//! resumed by the next attempt of the same task), per-tool approvals via
//! `MissionSpec::tool_approval`, and operator steering via
//! `AucterisCore::steer`.
//!
//! Trade-off (documented in the ADR): komet-harness owns the child
//! process, so the engine's bwrap/cgroup/egress-proxy/pid-file
//! supervision does NOT apply. The komet sandbox level is the boundary;
//! every limitation is recorded on the attempt, never assumed.

use anyhow::Result;
use aucteris_core::runtime::{
    AdapterCapabilities, IsolationLevel, RunOutcome, RunRequest, RuntimeAdapter, RuntimeEvent,
};
use aucteris_core::sandbox::SandboxMounts;
use futures::StreamExt;
use komet_harness::{CancellationToken, Harness, SteerMessage};
use komet_proto::{
    AgentEvent, DoneStatus, PermissionChoice, RunRequest as KometRunRequest, SandboxLevel, Scope,
};
use std::io::Write;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// Mission runtime name → komet harness, for the runtimes core missions
/// already know. Anything else stays on the supervised CLI adapters.
const SESSION_RUNTIMES: &[&str] = &["claude-code", "codex", "opencode", "devin"];

/// Register harness-backed adapters for every runtime komet can drive.
/// This REPLACES the supervised CLI adapters of the same name in the
/// registry — opt-in only: callers accept the supervision trade-off
/// documented in ADR-0001 (no engine bwrap/cgroup/egress for these
/// attempts; komet sandbox options are the boundary).
pub fn register_session_adapters(registry: &mut aucteris_core::RuntimeRegistry) {
    for runtime in SESSION_RUNTIMES {
        if let Some(harness) = crate::session::komet_backend(runtime) {
            registry.register_shared(Arc::new(HarnessMissionAdapter::new(harness, runtime)));
        }
    }
}

/// `RuntimeAdapter` over one komet `Harness`.
pub struct HarnessMissionAdapter {
    harness: Arc<dyn Harness>,
    runtime: &'static str,
}

impl HarnessMissionAdapter {
    pub fn new(harness: Arc<dyn Harness>, runtime: &'static str) -> Self {
        Self { harness, runtime }
    }
}

impl RuntimeAdapter for HarnessMissionAdapter {
    fn name(&self) -> &'static str {
        self.runtime
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            // Tool calls CAN be mediated — but only when the mission
            // opted into `tool_approval`; otherwise auto-approve applies.
            mediated_tools: true,
            cancellable: true,
            streaming_events: true,
            // komet sandbox options constrain the CLI's own permissions;
            // there is no engine-level fs boundary on this path.
            workspace_isolation: IsolationLevel::RuntimeAssisted,
            resource_accounting: true,
            limitations: vec![
                "engine sandbox (bwrap/cgroup/egress proxy/pid-file) does not supervise harness sessions — komet sandbox options are the boundary".into(),
                "agent questions are auto-answered empty (missions have no interactive channel)".into(),
                "per-tool approvals require mission tool_approval: true; otherwise tools auto-approve".into(),
            ],
        }
    }

    fn sandbox_mounts(&self) -> SandboxMounts {
        // Engine sandbox is not applied on this path (see limitations);
        // mounts are declared for completeness only.
        SandboxMounts::default()
    }

    fn run(&self, req: RunRequest) -> Result<RunOutcome> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(self.run_async(req))
    }
}

impl HarnessMissionAdapter {
    async fn run_async(&self, req: RunRequest) -> Result<RunOutcome> {
        let events_path = req.events_path.clone();
        if let Some(parent) = events_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut writer = std::io::BufWriter::new(std::fs::File::create(&events_path)?);

        // Steering: drain the engine inbox on a dedicated thread into the
        // komet steering mailbox (consumed at step/turn boundaries).
        let (steer_tx, steer_rx) = mpsc::channel::<SteerMessage>(64);
        if let Some(inbox) = req.steering.clone() {
            std::thread::spawn(move || {
                loop {
                    match inbox.recv_timeout(Duration::from_millis(200)) {
                        Ok(text) => {
                            if steer_tx
                                .blocking_send(SteerMessage {
                                    prompt: text,
                                    message_id: None,
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            if steer_tx.is_closed() {
                                break;
                            }
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            });
        }

        // Cancellation: the engine's global cancel signal (Ctrl+C path)
        // maps onto the harness interrupt token. `done` lets the poller
        // exit when the run ends — without it one thread leaks per attempt.
        let interrupt = CancellationToken::new();
        let cancel_done = Arc::new(AtomicBool::new(false));
        // Drop guard: every exit path (including `?` on stream/write errors)
        // releases the poller thread.
        struct DoneOnDrop(Arc<AtomicBool>);
        impl Drop for DoneOnDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _cancel_guard = DoneOnDrop(cancel_done.clone());
        {
            let token = interrupt.clone();
            let done = cancel_done.clone();
            std::thread::spawn(move || {
                while !done.load(Ordering::SeqCst) {
                    if aucteris_core::infrastructure::process::CANCEL_REQUESTED
                        .load(Ordering::SeqCst)
                    {
                        token.cancel();
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            });
        }

        let permission_seq = Arc::new(AtomicU64::new(1));
        let controls = komet_harness::RunControls {
            // Missions have no question channel: answer empty rather than
            // hang the agent (recorded as a capability limitation).
            request_input: Box::new(|_questions, _source| {
                let (tx, rx) = oneshot::channel();
                let _ = tx.send(Vec::<komet_proto::UserInputAnswer>::new());
                rx
            }),
            request_permission: permission_bridge(req.interactions.clone(), permission_seq),
            steering: steer_rx,
            interrupt,
            env: Vec::new(),
        };

        let komet_req = KometRunRequest {
            prompt: req.prompt.clone(),
            harness: None,
            model: None,
            reasoning: None,
            model_options: Default::default(),
            cwd: req.workspace.display().to_string(),
            // Mission workspaces are the confinement intent; komet's own
            // sandbox level is the enforceable boundary on this path.
            sandbox: SandboxLevel::WorkspaceWrite,
            sandbox_options: None,
            // No gate wired → the mission was pre-authorized as a batch
            // job; with a gate, every tool call asks first.
            auto_approve: req.interactions.is_none(),
            resume: req.resume_session.clone(),
            attachments: Vec::new(),
            worktree: None,
            permission_timeout_ms: None,
            use_mcp: false,
            mcp: None,
            mcp_external: Vec::new(),
        };

        let mut stream = self
            .harness
            .run(komet_req, controls)
            .await
            .map_err(|e| anyhow::anyhow!("harness run: {e}"))?;

        let mut tool_calls = 0u32;
        let mut usage: Option<(u64, u64)> = None;
        let mut session_id: Option<String> = None;
        let mut assertion: Option<String> = None;
        let mut status = DoneStatus::Errored;
        let mut error: Option<String> = None;
        // Assistant text is buffered and flushed as one RuntimeEvent —
        // deltas are a UI concern, missions want the message.
        let mut text_buf = String::new();

        macro_rules! flush_text {
            () => {
                if !text_buf.is_empty() {
                    let _ = req.event_sink.as_ref().map(|sink| {
                        sink(RuntimeEvent::AssistantMessage {
                            text: std::mem::take(&mut text_buf),
                        })
                    });
                }
            };
        }

        while let Some(event) = stream.next().await {
            let event = match event {
                Ok(event) => event,
                Err(e) => {
                    error = Some(e.to_string());
                    break;
                }
            };
            // Raw AgentEvent JSONL is the evidence record.
            serde_json::to_writer(&mut writer, &event)?;
            writeln!(writer)?;
            writer.flush()?;

            match &event {
                AgentEvent::TextDelta { text } | AgentEvent::ReasoningDelta { text } => {
                    text_buf.push_str(text);
                }
                AgentEvent::SessionStarted {
                    model,
                    session_id: sid,
                    ..
                } => {
                    session_id = Some(sid.clone());
                    if let Some(sink) = &req.event_sink {
                        sink(RuntimeEvent::SessionStarted {
                            model: Some(model.clone()),
                        });
                    }
                }
                AgentEvent::ToolCall { call, .. } => {
                    tool_calls += 1;
                    flush_text!();
                    if let Some(sink) = &req.event_sink {
                        sink(RuntimeEvent::ToolCall {
                            name: tool_name(call),
                        });
                    }
                }
                AgentEvent::ToolResult { is_error, .. } => {
                    flush_text!();
                    if let Some(sink) = &req.event_sink {
                        sink(RuntimeEvent::ToolResult { ok: !is_error });
                    }
                }
                AgentEvent::Usage {
                    input_tokens,
                    output_tokens,
                    ..
                } => {
                    usage = Some((*input_tokens, *output_tokens));
                    if let Some(sink) = &req.event_sink {
                        sink(RuntimeEvent::Usage {
                            input_tokens: *input_tokens,
                            output_tokens: *output_tokens,
                        });
                    }
                }
                AgentEvent::Error { message, .. } => {
                    flush_text!();
                    if let Some(sink) = &req.event_sink {
                        sink(RuntimeEvent::Error {
                            message: message.clone(),
                        });
                    }
                }
                AgentEvent::Done {
                    status: done_status,
                    result,
                    error: done_error,
                    session_id: done_session,
                    ..
                } => {
                    status = *done_status;
                    assertion = result.clone();
                    if let Some(sid) = done_session {
                        session_id = Some(sid.clone());
                    }
                    if let Some(e) = done_error {
                        error = Some(e.clone());
                    }
                }
                _ => {}
            }
        }
        flush_text!();
        if let Some(sink) = &req.event_sink
            && let Some(text) = assertion.clone().filter(|t| !t.trim().is_empty())
        {
            sink(RuntimeEvent::FinalResult { text });
        }

        let exit_code = match status {
            DoneStatus::Completed => 0,
            DoneStatus::Interrupted => 130,
            DoneStatus::Errored => 1,
        };
        if status == DoneStatus::Errored && error.is_none() {
            error = Some("agent run failed".into());
        }
        if let Some(e) = &error
            && let Some(sink) = &req.event_sink
        {
            sink(RuntimeEvent::Error { message: e.clone() });
        }

        Ok(RunOutcome {
            exit_code,
            tool_calls,
            agent_assertion: assertion,
            usage,
            pid: None,
            cgroup_limited: false,
            egress: None,
            session_id,
        })
    }
}

/// Display name for a komet `ToolCall` (mission evidence stays coarse —
/// the raw JSONL carries the full call).
fn tool_name(call: &komet_proto::ToolCall) -> String {
    match call {
        komet_proto::ToolCall::Exec { .. } => "Bash".into(),
        komet_proto::ToolCall::ReadFile { .. } => "Read".into(),
        komet_proto::ToolCall::WriteFile { .. } => "Write".into(),
        komet_proto::ToolCall::EditFile { .. } => "Edit".into(),
        komet_proto::ToolCall::ApplyPatch { .. } => "Patch".into(),
        komet_proto::ToolCall::Search { .. } => "Grep".into(),
        komet_proto::ToolCall::Glob { .. } => "Glob".into(),
        komet_proto::ToolCall::WebFetch { .. } => "Fetch".into(),
        komet_proto::ToolCall::WebSearch { .. } => "Search".into(),
        komet_proto::ToolCall::Todo { .. } => "Plan".into(),
        komet_proto::ToolCall::Mcp { tool, .. } => tool.clone(),
        komet_proto::ToolCall::Unknown { name, .. } => name.clone(),
    }
}

/// Bridge the engine approval gate into komet's permission callback.
/// `AllowAlways` maps to chat scope — the narrowest honest interpretation
/// for a batch mission (no repo-wide grants are invented).
fn permission_bridge(
    interactions: Option<aucteris_core::runtime::ApprovalHandler>,
    seq: Arc<AtomicU64>,
) -> komet_harness::RequestPermissionFn {
    Box::new(move |kind, summary, choices, tool_call_id| {
        let (tx, rx) = oneshot::channel();
        let Some(handler) = interactions.clone() else {
            let _ = tx.send(PermissionChoice::Deny);
            return rx;
        };
        let request = aucteris_core::runtime::ApprovalRequest {
            request_id: format!("perm-{}", seq.fetch_add(1, Ordering::SeqCst)),
            kind: permission_kind_label(&kind),
            summary,
            choices: choices.iter().map(choice_label).collect(),
            tool_call_id,
        };
        tokio::spawn(async move {
            // Dropped/errored handler → deny: approvals default closed.
            let decision = handler(request)
                .await
                .unwrap_or(aucteris_core::runtime::ApprovalDecision::Deny);
            let _ = tx.send(match decision {
                aucteris_core::runtime::ApprovalDecision::Allow => PermissionChoice::Allow,
                aucteris_core::runtime::ApprovalDecision::AllowAlways => {
                    PermissionChoice::AllowAlways { scope: Scope::Chat }
                }
                aucteris_core::runtime::ApprovalDecision::Deny => PermissionChoice::Deny,
            });
        });
        rx
    })
}

fn permission_kind_label(kind: &komet_proto::PermissionKind) -> String {
    match kind {
        komet_proto::PermissionKind::Tool { name } => format!("tool:{name}"),
        komet_proto::PermissionKind::Command { .. } => "command".into(),
        komet_proto::PermissionKind::FileWrite { .. } => "file_write".into(),
    }
}

fn choice_label(choice: &PermissionChoice) -> String {
    match choice {
        PermissionChoice::Allow => "allow".into(),
        PermissionChoice::AllowAlways { .. } => "allow_always".into(),
        PermissionChoice::Deny => "deny".into(),
    }
}
