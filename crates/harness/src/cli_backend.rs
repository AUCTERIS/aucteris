//! [`CliBackend`]: a komet [`Harness`] backed by a core [`RuntimeAdapter`].
//!
//! Runtimes without a native komet driver (gemini, mock) still get a
//! session-shaped backend so chats and missions share one API. The
//! capabilities are honestly degraded: single-shot runs, no resume, no
//! steering, no per-tool approvals — the wrapped adapter's own sandbox
//! plan still applies (bubblewrap when available), exactly like the
//! legacy mission path did.

use async_trait::async_trait;
use aucteris_core::runtime::{RunRequest as CoreRunRequest, RuntimeAdapter, RuntimeEvent};
use aucteris_core::sandbox::SandboxPlan;
use futures::{StreamExt, stream::BoxStream};
use komet_harness::{Harness, HarnessError};
use komet_proto::{
    AgentEvent, DoneStatus, ErrorKind, HarnessId, Model, ReasoningLevel, RunPhase, RunRequest,
    SteeringMode,
};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::sync::mpsc;

/// komet `Harness` over a core `RuntimeAdapter`.
pub struct CliBackend {
    adapter: Arc<dyn RuntimeAdapter>,
    runtime: String,
}

impl CliBackend {
    pub fn new(adapter: Arc<dyn RuntimeAdapter>, runtime: impl Into<String>) -> Self {
        Self {
            adapter,
            runtime: runtime.into(),
        }
    }
}

#[async_trait]
impl Harness for CliBackend {
    fn id(&self) -> HarnessId {
        // komet-proto has no variant for CLI-generic runtimes (gemini,
        // mock). `id()` feeds only komet-engine telemetry surfaces we
        // never call — Mock is the least-wrong placeholder, documented
        // here so nobody "fixes" it into a real harness id.
        HarnessId::Mock
    }

    fn display_name(&self) -> &str {
        &self.runtime
    }

    fn supports_steering(&self) -> bool {
        false
    }

    fn steering_mode(&self) -> SteeringMode {
        SteeringMode::TurnBoundary
    }

    fn reasoning_levels(&self) -> &[ReasoningLevel] {
        &[]
    }

    async fn models(&self) -> Result<Vec<Model>, HarnessError> {
        Err(HarnessError::Protocol(format!(
            "{} exposes no live model discovery",
            self.runtime
        )))
    }

    async fn run(
        &self,
        request: RunRequest,
        controls: komet_harness::RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        let adapter = self.adapter.clone();
        let runtime = self.runtime.clone();
        let (tx, rx) = mpsc::channel::<Result<AgentEvent, HarnessError>>(256);

        let workspace = PathBuf::from(&request.cwd);
        let run_dir = std::env::temp_dir().join(format!(
            "aucteris-cli-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::create_dir_all(&run_dir);
        let events_path = run_dir.join("events.jsonl");
        let pid_file = run_dir.join("child.pid");

        // Interrupt → kill the supervised process tree via its pid file.
        // The file appears at spawn time; poll briefly so a cancel that
        // lands mid-spawn still reaches the child.
        let interrupted = Arc::new(AtomicBool::new(false));
        // Signals the interrupt watcher to exit when the run finishes —
        // without it one pending task leaks per chat run.
        let run_done = Arc::new(tokio::sync::Notify::new());
        {
            let flag = interrupted.clone();
            let pid_file = pid_file.clone();
            let token = controls.interrupt.clone();
            let done = run_done.clone();
            tokio::spawn(async move {
                tokio::select! {
                    _ = token.cancelled() => {}
                    _ = done.notified() => return,
                }
                flag.store(true, Ordering::SeqCst);
                for _ in 0..100 {
                    if let Ok(raw) = std::fs::read_to_string(&pid_file)
                        && let Ok(pid) = raw.trim().parse::<u32>()
                        && aucteris_core::infrastructure::process::pid_alive(pid)
                    {
                        aucteris_core::infrastructure::process::kill_process_tree(
                            pid,
                            libc::SIGKILL,
                        );
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            });
        }

        // The core adapter is synchronous: run it on the blocking pool.
        let core_req = CoreRunRequest {
            workspace: workspace.clone(),
            prompt: request.prompt.clone(),
            events_path,
            max_turns: u32::MAX,
            pid_file: Some(pid_file),
            // Same engine sandbox the legacy mission path applied:
            // bubblewrap when available, honest degradation otherwise.
            sandbox: Some(SandboxPlan {
                workspace: workspace.clone(),
                mounts: adapter.sandbox_mounts(),
                allow_network: true,
            }),
            cgroup_name: None,
            egress_filter: None,
            mcp: None,
            resume_session: request.resume.clone(),
            interactions: None,
            steering: None,
            event_sink: Some(runtime_event_sink(tx.clone(), runtime.clone())),
        };

        let flag = interrupted.clone();
        tokio::task::spawn_blocking(move || {
            let _ = tx.blocking_send(Ok(AgentEvent::RunPhase {
                phase: RunPhase::Starting,
            }));
            let outcome = adapter.run(core_req);
            let status = if flag.load(Ordering::SeqCst) {
                DoneStatus::Interrupted
            } else {
                match &outcome {
                    Ok(o) if o.exit_code == 0 => DoneStatus::Completed,
                    _ => DoneStatus::Errored,
                }
            };
            match outcome {
                Ok(o) => {
                    if let Some((input, output)) = o.usage {
                        let _ = tx.blocking_send(Ok(AgentEvent::Usage {
                            input_tokens: input,
                            cached_input_tokens: 0,
                            output_tokens: output,
                            reasoning_tokens: 0,
                            context_limit: None,
                        }));
                    }
                    let _ = tx.blocking_send(Ok(AgentEvent::Done {
                        status,
                        result: o.agent_assertion.clone(),
                        error: (o.exit_code != 0 && status == DoneStatus::Errored)
                            .then(|| format!("runtime exited with code {}", o.exit_code)),
                        session_id: None,
                        reason: None,
                    }));
                }
                Err(e) => {
                    let message = format!("{e:#}");
                    let _ = tx.blocking_send(Ok(AgentEvent::Error {
                        message: message.clone(),
                        kind: ErrorKind::Unknown,
                        detail: None,
                        retryable: false,
                    }));
                    let _ = tx.blocking_send(Ok(AgentEvent::Done {
                        status: DoneStatus::Errored,
                        result: None,
                        error: Some(message),
                        session_id: None,
                        reason: None,
                    }));
                }
            }
            run_done.notify_one();
        });

        let stream = futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        });
        Ok(stream.boxed())
    }
}

/// Bridge core `RuntimeEvent`s into synthesized `AgentEvent`s. The wrapped
/// adapter has no session/approval/steer protocol, so those surfaces stay
/// empty — capabilities are reported honestly instead.
fn runtime_event_sink(
    tx: mpsc::Sender<Result<AgentEvent, HarnessError>>,
    _runtime: String,
) -> Arc<dyn Fn(RuntimeEvent) + Send + Sync> {
    let saw_text = Arc::new(AtomicBool::new(false));
    let tool_seq = Arc::new(AtomicU64::new(1));
    let last_tool = Arc::new(Mutex::new(String::new()));
    Arc::new(move |event| {
        let agent_events: Vec<AgentEvent> = match event {
            RuntimeEvent::SessionStarted { .. } => vec![],
            RuntimeEvent::AssistantMessage { text } => {
                saw_text.store(true, Ordering::SeqCst);
                vec![AgentEvent::TextDelta { text }]
            }
            RuntimeEvent::FinalResult { text } => {
                if saw_text.load(Ordering::SeqCst) || text.trim().is_empty() {
                    vec![]
                } else {
                    vec![AgentEvent::TextDelta { text }]
                }
            }
            RuntimeEvent::ToolCall { name } => {
                let id = format!("tool-{}", tool_seq.fetch_add(1, Ordering::SeqCst));
                *last_tool.lock().unwrap_or_else(|e| e.into_inner()) = id.clone();
                vec![AgentEvent::ToolCall {
                    id,
                    call: komet_proto::ToolCall::Unknown { name, input: None },
                }]
            }
            RuntimeEvent::ToolResult { ok } => {
                let id = last_tool.lock().unwrap_or_else(|e| e.into_inner()).clone();
                vec![AgentEvent::ToolResult {
                    id,
                    is_error: !ok,
                    output: None,
                    diff: None,
                }]
            }
            RuntimeEvent::Usage {
                input_tokens,
                output_tokens,
            } => vec![AgentEvent::Usage {
                input_tokens,
                cached_input_tokens: 0,
                output_tokens,
                reasoning_tokens: 0,
                context_limit: None,
            }],
            RuntimeEvent::Error { message } => vec![AgentEvent::Error {
                message,
                kind: ErrorKind::Unknown,
                detail: None,
                retryable: false,
            }],
            RuntimeEvent::Other => vec![],
        };
        for event in agent_events {
            let _ = tx.blocking_send(Ok(event));
        }
    })
}
