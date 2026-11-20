//! Persistent agent sessions: one harness run per chat.
//!
//! Each chat owns a harness-native session id, so a follow-up prompt resumes
//! the conversation instead of starting a fresh CLI process with no memory.
//! Runs are independent: Stop interrupts one chat's run, not every agent.
//!
//! Runtimes without a komet driver (gemini, mock) are served by
//! [`CliBackend`](crate::cli_backend::CliBackend) through the same API —
//! with honestly degraded capabilities (no resume, no steering, no
//! per-tool approvals).

use crate::cli_backend::CliBackend;
use aucteris_core::runtime::RuntimeRegistry;
use futures::StreamExt;
use komet_harness::{
    AcpHarness, CancellationToken, ClaudeHarness, CodexHarness, Harness, RunControls, SteerMessage,
};
use komet_proto::{
    AgentEvent, DoneStatus, Model, PermissionChoice, RunRequest, ToolCall, ToolDiff,
    UserInputAnswer, UserInputQuestion,
};
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::{mpsc, oneshot};

/// Events a session run emits to its host (chat UI, mission bridge, tests).
#[derive(Debug)]
pub enum EngineEvent {
    Action(Action),
    /// Harness-native session id: the next prompt in this chat resumes it.
    SessionId(String),
    /// The agent is blocked on an approval. Answer with
    /// [`Sessions::answer_permission`].
    Permission {
        id: u64,
        kind: komet_proto::PermissionKind,
        summary: String,
        choices: Vec<PermissionChoice>,
        tool_call_id: Option<String>,
    },
    /// The agent is blocked on questions. Answer with
    /// [`Sessions::answer_question`], one question at a time.
    Questions {
        id: u64,
        questions: Vec<UserInputQuestion>,
    },
    Finished(Result<(), String>),
}

#[derive(Debug, Clone)]
pub enum Action {
    Intent(String),
    Say(String),
    Thought(String),
    Tool {
        icon: &'static str,
        label: String,
        activity: String,
        detail: Option<String>,
    },
    /// A tool's live output: replaces its preview, or extends it when the
    /// harness streams deltas.
    ToolProgress {
        text: String,
        append: bool,
    },
    /// Coarse run phase, for the status line only.
    Phase(&'static str),
    Files {
        path: String,
        added: u32,
        removed: u32,
    },
    Settle(String),
    Error(String),
    Usage {
        input: u64,
        output: u64,
    },
}

/// The komet driver behind a stored runtime id, or `None` when the CLI has
/// no native driver — callers fall back to [`backend`] for the CLI path.
pub fn komet_backend(runtime: &str) -> Option<Arc<dyn Harness>> {
    match runtime.split(':').next().unwrap_or(runtime) {
        "claude-code" => Some(Arc::new(ClaudeHarness::new())),
        "codex" => Some(Arc::new(CodexHarness::new())),
        "opencode" => Some(Arc::new(AcpHarness::opencode())),
        "devin" => Some(Arc::new(AcpHarness::devin())),
        "grok" => Some(Arc::new(AcpHarness::grok())),
        _ => None,
    }
}

/// The single backend resolution for every runtime id: komet driver first,
/// then a core `RuntimeAdapter` wrapped as a session backend. `None` only
/// when neither exists — never a silent fallback to a different runtime.
pub fn backend(runtime: &str, registry: &RuntimeRegistry) -> Option<Arc<dyn Harness>> {
    let base = runtime.split(':').next().unwrap_or(runtime);
    komet_backend(base).or_else(|| {
        registry
            .get(base)
            .ok()
            .map(|adapter| Arc::new(CliBackend::new(adapter, base.to_string())) as Arc<dyn Harness>)
    })
}

struct Live {
    steer: mpsc::Sender<SteerMessage>,
    interrupt: CancellationToken,
    steerable: bool,
}

/// A question set the agent is blocked on, with the answers gathered so far.
struct PendingInput {
    chat: u32,
    questions: Vec<UserInputQuestion>,
    answers: Vec<Option<(Vec<String>, Option<String>)>>,
    tx: oneshot::Sender<Vec<UserInputAnswer>>,
}

/// Permission requests parked until the UI answers; keyed by request id.
type PendingPermissions = Arc<Mutex<HashMap<u64, (u32, oneshot::Sender<PermissionChoice>)>>>;

pub struct Sessions {
    runtime: tokio::runtime::Runtime,
    live: Arc<Mutex<HashMap<u32, Live>>>,
    /// Permission requests parked until the UI answers; keyed by request id.
    permissions: PendingPermissions,
    inputs: Arc<Mutex<HashMap<u64, PendingInput>>>,
    next_request: Arc<AtomicU64>,
}

impl Sessions {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?,
            live: Arc::default(),
            permissions: Arc::default(),
            inputs: Arc::default(),
            next_request: Arc::new(AtomicU64::new(1)),
        })
    }

    /// Start a run for `chat_id`, forwarding translated actions to `out`.
    pub fn start(
        &self,
        chat_id: u32,
        harness: Arc<dyn Harness>,
        request: RunRequest,
        out: mpsc::UnboundedSender<EngineEvent>,
    ) {
        let (steer, steering) = mpsc::channel(64);
        let interrupt = CancellationToken::new();
        let live = self.live.clone();
        let permissions = self.permissions.clone();
        let inputs = self.inputs.clone();
        let next_request = self.next_request.clone();
        if let Some(previous) = live.lock().expect("session map poisoned").insert(
            chat_id,
            Live {
                steer,
                interrupt: interrupt.clone(),
                steerable: harness.supports_steering(),
            },
        ) {
            previous.interrupt.cancel();
        }

        self.runtime.spawn(async move {
            let questions_out = out.clone();
            let permission_out = out.clone();
            let input_queue = inputs.clone();
            let permission_queue = permissions.clone();
            let input_ids = next_request.clone();
            let permission_ids = next_request;
            let controls = RunControls {
                // Ask the UI; a dismissed card leaves the agent blocked only
                // until the run dies, which drops the parked sender.
                request_input: Box::new(move |questions, _source| {
                    let id = input_ids.fetch_add(1, Ordering::Relaxed);
                    let (tx, rx) = oneshot::channel();
                    input_queue.lock().expect("input map poisoned").insert(
                        id,
                        PendingInput {
                            chat: chat_id,
                            answers: vec![None; questions.len()],
                            questions: questions.clone(),
                            tx,
                        },
                    );
                    let _ = questions_out.send(EngineEvent::Questions { id, questions });
                    rx
                }),
                request_permission: Box::new(move |kind, summary, choices, tool_call_id| {
                    let id = permission_ids.fetch_add(1, Ordering::Relaxed);
                    let (tx, rx) = oneshot::channel();
                    permission_queue
                        .lock()
                        .expect("permission map poisoned")
                        .insert(id, (chat_id, tx));
                    let _ = permission_out.send(EngineEvent::Permission {
                        id,
                        kind,
                        summary,
                        choices,
                        tool_call_id,
                    });
                    rx
                }),
                steering,
                interrupt,
                env: Vec::new(),
            };

            let result = match harness.run(request, controls).await {
                Ok(mut stream) => {
                    let mut end = Ok(());
                    while let Some(event) = stream.next().await {
                        match event {
                            Ok(event) => {
                                if let Some(done) = emit(event, &out) {
                                    end = done;
                                    break;
                                }
                            }
                            Err(error) => {
                                end = Err(error.to_string());
                                break;
                            }
                        }
                    }
                    end
                }
                Err(error) => Err(error.to_string()),
            };

            live.lock().expect("session map poisoned").remove(&chat_id);
            // An unanswered card must not outlive its run: dropping the sender
            // makes the harness fall back to Deny instead of hanging.
            permissions
                .lock()
                .expect("permission map poisoned")
                .retain(|_, (chat, _)| *chat != chat_id);
            inputs
                .lock()
                .expect("input map poisoned")
                .retain(|_, pending| pending.chat != chat_id);
            let _ = out.send(EngineEvent::Finished(result));
        });
    }

    /// Answer a parked permission request. `false` when it is already gone.
    pub fn answer_permission(&self, id: u64, choice: PermissionChoice) -> bool {
        self.permissions
            .lock()
            .expect("permission map poisoned")
            .remove(&id)
            .map(|(_, tx)| tx.send(choice).is_ok())
            .unwrap_or(false)
    }

    /// Record one question's answer; hands the whole set over once every
    /// question of the request has one. Returns whether it was the last.
    pub fn answer_question(
        &self,
        id: u64,
        index: usize,
        labels: Vec<String>,
        free_text: Option<String>,
    ) -> bool {
        let mut inputs = self.inputs.lock().expect("input map poisoned");
        let Some(pending) = inputs.get_mut(&id) else {
            return false;
        };
        if index >= pending.answers.len() {
            return false;
        }
        pending.answers[index] = Some((labels, free_text));
        if pending.answers.iter().any(Option::is_none) {
            return false;
        }
        let Some(pending) = inputs.remove(&id) else {
            return false;
        };
        let answers = pending
            .questions
            .iter()
            .zip(pending.answers)
            .map(|(question, answer)| {
                let (labels, free_text) = answer.unwrap_or_default();
                UserInputAnswer {
                    question_id: question.id.clone(),
                    labels,
                    free_text,
                }
            })
            .collect();
        pending.tx.send(answers).is_ok()
    }

    /// Ask one harness which models it exposes. Blocks the calling thread, so
    /// call it from a background thread, never the UI thread.
    pub fn models(&self, harness: &dyn Harness) -> Result<Vec<Model>, String> {
        self.runtime
            .block_on(harness.models())
            .map_err(|error| error.to_string())
    }

    /// Deliver a prompt into the live run. `false` when this chat is not
    /// running or its harness cannot steer.
    pub fn steer(&self, chat_id: u32, prompt: String) -> bool {
        self.live
            .lock()
            .expect("session map poisoned")
            .get(&chat_id)
            .filter(|live| live.steerable)
            .map(|live| {
                live.steer
                    .try_send(SteerMessage {
                        prompt,
                        message_id: None,
                    })
                    .is_ok()
            })
            .unwrap_or(false)
    }

    pub fn can_steer(&self, chat_id: u32) -> bool {
        self.live
            .lock()
            .expect("session map poisoned")
            .get(&chat_id)
            .map(|live| live.steerable)
            .unwrap_or(false)
    }

    pub fn running(&self, chat_id: u32) -> bool {
        self.live
            .lock()
            .expect("session map poisoned")
            .contains_key(&chat_id)
    }

    pub fn stop(&self, chat_id: u32) {
        if let Some(live) = self
            .live
            .lock()
            .expect("session map poisoned")
            .get(&chat_id)
        {
            live.interrupt.cancel();
        }
    }
}

/// Translate one harness event into host actions. `Some(end)` closes the run.
fn emit(event: AgentEvent, out: &mpsc::UnboundedSender<EngineEvent>) -> Option<Result<(), String>> {
    let action = match event {
        AgentEvent::SessionStarted {
            model, session_id, ..
        } => {
            let _ = out.send(EngineEvent::SessionId(session_id));
            Some(Action::Intent(format!("Connected · {model}")))
        }
        AgentEvent::TextDelta { text } => Some(Action::Say(text)),
        AgentEvent::ReasoningDelta { text } => Some(Action::Thought(text)),
        AgentEvent::ToolCall { call, .. } => {
            let (label, detail) = tool_display(&call);
            Some(Action::Tool {
                icon: tool_icon(&label),
                activity: format!("Running {label}"),
                label,
                detail,
            })
        }
        AgentEvent::ToolResult {
            is_error,
            output,
            diff,
            ..
        } => {
            if let Some(diff) = diff {
                let (added, removed) = diff_counts(&diff);
                let _ = out.send(EngineEvent::Action(Action::Files {
                    path: diff.path,
                    added,
                    removed,
                }));
            }
            Some(Action::Settle(cap(output
                .filter(|text| !text.trim().is_empty())
                .unwrap_or_else(|| {
                    if is_error { "Failed" } else { "Completed" }.to_string()
                }))))
        }
        AgentEvent::ToolProgress { text, append, .. } => {
            Some(Action::ToolProgress { text, append })
        }
        AgentEvent::RunPhase { phase } => Some(Action::Phase(match phase {
            komet_proto::RunPhase::Starting => "Starting…",
            komet_proto::RunPhase::WaitingModel => "Thinking…",
            komet_proto::RunPhase::Streaming => "Writing…",
            komet_proto::RunPhase::Tool => "Working…",
        })),
        AgentEvent::Usage {
            input_tokens,
            output_tokens,
            ..
        } => Some(Action::Usage {
            input: input_tokens,
            output: output_tokens,
        }),
        AgentEvent::Error { message, .. } => Some(Action::Error(message)),
        AgentEvent::Done {
            status,
            error,
            session_id,
            ..
        } => {
            if let Some(session_id) = session_id {
                let _ = out.send(EngineEvent::SessionId(session_id));
            }
            return Some(match status {
                DoneStatus::Completed | DoneStatus::Interrupted => Ok(()),
                DoneStatus::Errored => Err(error.unwrap_or_else(|| "agent run failed".into())),
            });
        }
        _ => None,
    };
    if let Some(action) = action {
        let _ = out.send(EngineEvent::Action(action));
    }
    None
}

/// Tool output is for one chip line: keep it short instead of the whole file.
fn cap(text: String) -> String {
    const LIMIT: usize = 400;
    if text.chars().count() <= LIMIT {
        return text;
    }
    let mut capped: String = text.chars().take(LIMIT).collect();
    capped.push('…');
    capped
}

/// The two words a tool chip shows: what ran, and its most useful argument.
fn tool_display(call: &ToolCall) -> (String, Option<String>) {
    match call {
        ToolCall::Exec { command } => ("Bash".to_string(), Some(command.clone())),
        ToolCall::ReadFile { path } => ("Read".to_string(), Some(path.clone())),
        ToolCall::WriteFile { path, .. } => ("Write".to_string(), Some(path.clone())),
        ToolCall::EditFile { path, .. } => ("Edit".to_string(), Some(path.clone())),
        ToolCall::ApplyPatch { path } => ("Patch".to_string(), path.clone()),
        ToolCall::Search { pattern, path } => (
            "Grep".to_string(),
            Some(match path {
                Some(path) => format!("{pattern} in {path}"),
                None => pattern.clone(),
            }),
        ),
        ToolCall::Glob { pattern } => ("Glob".to_string(), Some(pattern.clone())),
        ToolCall::WebFetch { url, .. } => ("Fetch".to_string(), Some(url.clone())),
        ToolCall::WebSearch { query } => ("Search".to_string(), Some(query.clone())),
        ToolCall::Todo { items } => ("Plan".to_string(), Some(format!("{} steps", items.len()))),
        ToolCall::Mcp { tool, .. } => (tool.clone(), None),
        ToolCall::Unknown { name, .. } => (name.clone(), None),
    }
}

/// Lines changed between the old and new file text, ignoring the common
/// head and tail. Good enough for a chip; not a diff algorithm.
fn diff_counts(diff: &ToolDiff) -> (u32, u32) {
    let old: Vec<&str> = diff.old_text.as_deref().unwrap_or("").lines().collect();
    let new: Vec<&str> = diff.new_text.lines().collect();
    let mut head = 0;
    while head < old.len() && head < new.len() && old[head] == new[head] {
        head += 1;
    }
    let mut tail = 0;
    while tail < old.len() - head
        && tail < new.len() - head
        && old[old.len() - 1 - tail] == new[new.len() - 1 - tail]
    {
        tail += 1;
    }
    (
        (new.len() - head - tail) as u32,
        (old.len() - head - tail) as u32,
    )
}

/// Icon id for a tool label, drawn from the bundled glyph set.
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

#[cfg(test)]
mod tests {
    use super::*;
    use komet_proto::HarnessId;

    fn question(id: &str) -> UserInputQuestion {
        UserInputQuestion {
            id: id.to_string(),
            header: String::new(),
            question: format!("{id}?"),
            options: Vec::new(),
            multi_select: false,
            option_descriptions: Vec::new(),
            allow_free_text: true,
            secret: false,
            default: None,
        }
    }

    #[test]
    fn questions_resolve_only_when_every_question_is_answered() {
        let sessions = Sessions::new().unwrap();
        let (tx, rx) = oneshot::channel();
        sessions.inputs.lock().unwrap().insert(
            7,
            PendingInput {
                chat: 1,
                answers: vec![None, None],
                questions: vec![question("q1"), question("q2")],
                tx,
            },
        );
        assert!(!sessions.answer_question(7, 0, vec!["A".into()], None));
        assert!(sessions.answer_question(7, 1, Vec::new(), Some("free".into())));
        let answers = rx.blocking_recv().unwrap();
        assert_eq!(answers.len(), 2);
        assert_eq!(answers[0].question_id, "q1");
        assert_eq!(answers[0].labels, vec!["A".to_string()]);
        assert_eq!(answers[1].free_text.as_deref(), Some("free"));
        assert!(!sessions.answer_question(7, 0, vec!["late".into()], None));
    }

    #[test]
    fn permission_answer_reaches_the_harness_once() {
        let sessions = Sessions::new().unwrap();
        let (tx, rx) = oneshot::channel();
        sessions.permissions.lock().unwrap().insert(9, (1, tx));
        assert!(sessions.answer_permission(9, PermissionChoice::Deny));
        assert_eq!(rx.blocking_recv().unwrap(), PermissionChoice::Deny);
        assert!(!sessions.answer_permission(9, PermissionChoice::Allow));
    }

    #[test]
    fn text_and_session_id_reach_the_ui() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        assert!(emit(AgentEvent::TextDelta { text: "hi".into() }, &tx).is_none());
        assert!(matches!(
            rx.try_recv(),
            Ok(EngineEvent::Action(Action::Say(text))) if text == "hi"
        ));

        assert!(
            emit(
                AgentEvent::SessionStarted {
                    harness: HarnessId::Mock,
                    model: "m".into(),
                    tools: Vec::new(),
                    cwd: "/".into(),
                    session_id: "sess-1".into(),
                    assistant_message_id: "a".into(),
                },
                &tx,
            )
            .is_none()
        );
        assert!(matches!(
            rx.try_recv(),
            Ok(EngineEvent::SessionId(id)) if id == "sess-1"
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(EngineEvent::Action(Action::Intent(_)))
        ));
    }

    #[test]
    fn done_closes_the_run_and_errors_carry_the_message() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let end = emit(
            AgentEvent::Done {
                status: DoneStatus::Errored,
                result: None,
                error: Some("boom".into()),
                session_id: None,
                reason: None,
            },
            &tx,
        );
        assert_eq!(end, Some(Err("boom".into())));

        let end = emit(
            AgentEvent::Done {
                status: DoneStatus::Completed,
                result: None,
                error: None,
                session_id: None,
                reason: None,
            },
            &tx,
        );
        assert_eq!(end, Some(Ok(())));
    }

    #[test]
    fn diff_counts_ignores_common_head_and_tail() {
        let diff = ToolDiff {
            path: "a.rs".into(),
            old_text: Some("one\ntwo\nthree\nfour\n".into()),
            new_text: "one\nTWO\nthree\nfour\n".into(),
        };
        assert_eq!(diff_counts(&diff), (1, 1));
    }

    #[test]
    fn diff_counts_for_new_file_counts_every_line() {
        let diff = ToolDiff {
            path: "new.rs".into(),
            old_text: None,
            new_text: "a\nb\n".into(),
        };
        assert_eq!(diff_counts(&diff), (2, 0));
    }

    #[test]
    fn diff_counts_handles_deletions_to_empty() {
        let diff = ToolDiff {
            path: "gone.rs".into(),
            old_text: Some("a\nb".into()),
            new_text: String::new(),
        };
        assert_eq!(diff_counts(&diff), (0, 2));
    }
}
