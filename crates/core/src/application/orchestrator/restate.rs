//! Restate adapter for the orchestration port (ADR-0003).
//!
//! All Restate SDK mechanics live here — services, workflows, awakeables,
//! typed ingress clients. The engine-neutral surface is
//! `crate::orchestrator`; nothing outside this module should import
//! `restate_sdk` types.
//!
//! Architecture (measured in the ri-001-restate spike):
//! - `MissionDurable` (Workflow) owns the task lifecycle durably:
//!   sequential tasks, per-task attempt + independent verify activities,
//!   optional human-approval awakeable. Survives engine restarts by replay.
//! - `AttemptRunner` (shared service = Restate "activity") executes ONE
//!   attempt through the existing supervised path (spawn, process-tree
//!   kill, bwrap sandbox, file-store evidence). It reconciles BEFORE
//!   spawning so a Restate retry after engine death can never duplicate
//!   an agent — the old Running attempt becomes Unknown first.
//! - The file store remains the evidence/attempt record; Restate holds
//!   orchestration state. They are complementary, not redundant.
//!
//! Commands:
//!   aucteris serve                  run the Restate endpoint (:9080)
//!   aucteris run-durable <yaml>     persist mission, submit workflow
//!   aucteris approve <mission> <d>  resolve a pending approval awakeable

use restate_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::domain::{AttemptStatus, TaskSpec};
use crate::executor::Executor;
use crate::store::Store;

// ---------- activity payloads ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptTask {
    pub mission_id: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptOutcome {
    pub attempt_id: String,
    /// "succeeded" | "failed" | "unknown" — agent-level outcome only.
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInput {
    pub mission_id: String,
    pub tasks: Vec<TaskSpec>,
    /// If true, the workflow parks on an awakeable until `aucteris approve`.
    #[serde(default)]
    pub require_approval: bool,
}

fn store() -> anyhow::Result<&'static Store> {
    static STORE: std::sync::OnceLock<Store> = std::sync::OnceLock::new();
    if let Some(s) = STORE.get() {
        return Ok(s);
    }
    let s = Store::new(crate::data_dir())?;
    Ok(STORE.get_or_init(|| s))
}

fn executor() -> anyhow::Result<Executor<'static>> {
    Ok(Executor::new(store()?))
}

// ---------- activity: attempt runner + verifier ----------

/// Shared Restate service = durable activity. Long inactivity timeout:
/// an agent attempt legitimately runs for minutes without journaling.
pub struct AttemptRunner;

#[restate_sdk::service(
    inactivity_timeout = "20m",
    abort_timeout = "30s",
    invocation_retry_policy(max_attempts = 100, on_max_attempts = "pause")
)]
impl AttemptRunner {
    /// Run ONE attempt end-to-end (spawn → supervise → evidence).
    /// Reconciles first: after an engine death, Restate retries this
    /// activity; the reconcile marks the orphaned Running attempt
    /// Unknown and kills its process tree BEFORE anything new spawns.
    #[handler]
    async fn run_attempt(
        &self,
        _ctx: Context<'_>,
        input: Json<AttemptTask>,
    ) -> HandlerResult<Json<AttemptOutcome>> {
        let input = input.0;
        let exec = executor().map_err(|e| TerminalError::new(format!("store: {e}")))?;
        exec.reconcile_interrupted(&input.mission_id)
            .map_err(|e| TerminalError::new(format!("reconcile: {e}")))?;

        let store = store()?;
        let mission = store
            .load_mission(&input.mission_id)
            .map_err(|e| TerminalError::new(format!("load mission: {e}")))?;
        let spec = mission
            .tasks
            .iter()
            .find(|t| t.id == input.task_id)
            .cloned()
            .ok_or_else(|| TerminalError::new(format!("task {} not found", input.task_id)))?;
        // Resume the agent session a previous attempt of this task
        // reported, when the runtime supports persistent sessions.
        let mut state = store
            .load_state(&input.mission_id)
            .map_err(|e| TerminalError::new(format!("load state: {e}")))?;
        let resume = state
            .iter()
            .find(|r| r.task_id == input.task_id)
            .and_then(|r| r.session_id.clone());

        let attempt = exec
            .execute_attempt(&mission, &spec, resume)
            .map_err(|e| TerminalError::new(format!("attempt: {e}")))?;
        if let Some(session_id) = &attempt.session_id
            && let Some(run) = state.iter_mut().find(|r| r.task_id == input.task_id)
        {
            run.session_id = Some(session_id.clone());
            store
                .save_state(&input.mission_id, &state)
                .map_err(|e| TerminalError::new(format!("save state: {e}")))?;
        }

        let status = match attempt.status {
            AttemptStatus::Succeeded => "succeeded",
            AttemptStatus::Failed => "failed",
            AttemptStatus::Unknown => "unknown",
            AttemptStatus::Running => "running",
            _ => "other",
        };
        Ok(Json(AttemptOutcome {
            attempt_id: attempt.id,
            status: status.to_string(),
        }))
    }

    /// Independent verification — a separate journaled step, run by the
    /// engine (never the agent). Preserves evidence separation.
    #[handler(inactivity_timeout = "5m")]
    async fn verify(
        &self,
        _ctx: Context<'_>,
        input: Json<AttemptTask>,
    ) -> HandlerResult<Json<bool>> {
        let input = input.0;
        let exec = executor().map_err(|e| TerminalError::new(format!("store: {e}")))?;
        let mission = store()?
            .load_mission(&input.mission_id)
            .map_err(|e| TerminalError::new(format!("load mission: {e}")))?;
        let spec = mission
            .tasks
            .iter()
            .find(|t| t.id == input.task_id)
            .cloned()
            .ok_or_else(|| TerminalError::new(format!("task {} not found", input.task_id)))?;
        let ok = exec
            .verify_task(&mission, &spec)
            .map_err(|e| TerminalError::new(format!("verify: {e}")))?;
        Ok(Json(ok))
    }

    /// Post-mission memory sync (opt-in `sync_memory`): pushes
    /// <mission>/memory to Memexa via the aucteris-memory binary.
    /// maybe_sync_memory records every outcome as evidence and never
    /// fails — this handler therefore always succeeds; a mission-load
    /// failure is swallowed the same way (sync is best-effort).
    #[handler(inactivity_timeout = "5m")]
    async fn sync_memory(&self, _ctx: Context<'_>, input: Json<String>) -> HandlerResult<()> {
        let exec = executor().map_err(|e| TerminalError::new(format!("store: {e}")))?;
        if let Ok(mission) = store()?.load_mission(&input.0) {
            exec.maybe_sync_memory(&mission);
        }
        Ok(())
    }
}

// ---------- workflow: mission lifecycle ----------

pub struct MissionDurable;

#[restate_sdk::workflow]
impl MissionDurable {
    /// Sequential task execution, durable across restarts. Each task =
    /// attempt activity + (on success) verify activity. First failure
    /// or unknown outcome stops the mission — no silent continuation.
    #[handler]
    async fn run(
        &self,
        ctx: WorkflowContext<'_>,
        input: Json<WorkflowInput>,
    ) -> HandlerResult<String> {
        let input = input.0;

        if input.require_approval {
            let (awakeable_id, approval) = ctx.awakeable::<String>();
            // Persist the awakeable id so `aucteris approve` can resolve it.
            // ctx.run makes the write a journaled side effect (replay-safe).
            let mission_id = input.mission_id.clone();
            let aid = awakeable_id.clone();
            let _ = ctx
                .run(|| async move {
                    let dir = crate::data_dir().join("missions").join(&mission_id);
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::fs::write(
                        dir.join("approval.json"),
                        serde_json::json!({"awakeable_id": aid}).to_string(),
                    );
                    Ok(())
                })
                .await;

            let decision = approval.await?;
            if decision == "approved" {
                // Record the decision so observers stop showing the
                // mission as awaiting approval while waves run.
                // Additive journaled step — replay-compatible.
                let mid = input.mission_id.clone();
                let _ = ctx
                    .run(move || async move {
                        let dir = crate::data_dir().join("missions").join(&mid);
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = std::fs::write(
                            dir.join("approval.json"),
                            serde_json::json!({"decision": "approved", "engine": "restate"})
                                .to_string(),
                        );
                        Ok(())
                    })
                    .await;
            }
            if decision != "approved" {
                let mid = input.mission_id.clone();
                let d = decision.clone();
                let _ = ctx
                    .run(move || async move {
                        let dir = crate::data_dir().join("missions").join(&mid);
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = std::fs::write(
                            dir.join("durable-result.json"),
                            serde_json::json!({
                                "summary": format!("mission {} rejected at approval gate: {}", mid, d),
                                "workflow_version": WORKFLOW_VERSION,
                                "ts": chrono::Utc::now().to_rfc3339(),
                            })
                            .to_string(),
                        );
                        Ok(())
                    })
                    .await;
                return Ok(format!(
                    "mission {} rejected at approval gate: {}",
                    input.mission_id, decision
                ));
            }
        }

        // Dependency-gated parallel waves (max 4): attempt activities
        // fan out concurrently — agent runtime is the expensive part.
        // Verify activities then run SEQUENTIALLY after the join:
        // measured on restate-sdk 0.12.1, concurrent verify calls
        // stalled (2/3 completed, third stuck "ready" for minutes) —
        // sequential verifies are cheap and keep the journal simple.
        const MAX_PARALLEL: usize = 4;
        let mut completed: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut pending: Vec<TaskSpec> = input.tasks.clone();
        let mut outcomes: Vec<String> = vec![];
        let mut wave_failed = false;

        while !wave_failed {
            let ready: Vec<TaskSpec> = pending
                .iter()
                .filter(|t| t.depends_on.iter().all(|d| completed.contains(d)))
                .take(MAX_PARALLEL)
                .cloned()
                .collect();
            if ready.is_empty() {
                break;
            }
            for t in &ready {
                pending.retain(|p| p.id != t.id);
            }

            let mission_id = input.mission_id.clone();
            let wctx = &ctx;
            let results = futures_util::future::join_all(ready.iter().map(|task| {
                let mission_id = mission_id.clone();
                let task_id = task.id.clone();
                async move {
                    let runner = wctx.service_client::<AttemptRunnerClient>();
                    let task_input = AttemptTask {
                        mission_id,
                        task_id,
                    };
                    let tid = task_input.task_id.clone();
                    let outcome = runner
                        .run_attempt(Json(task_input))
                        .call()
                        .await
                        .map_err(|e| TerminalError::new(format!("attempt activity: {e}")))?;
                    // failed / unknown / running: honest stop for this
                    // task — dependents never become ready.
                    Ok::<(String, String), TerminalError>((tid, outcome.0.status.clone()))
                }
            }))
            .await;

            let runner = ctx.service_client::<AttemptRunnerClient>();
            for r in results {
                let (task_id, status) = r?;
                if status != "succeeded" {
                    wave_failed = true;
                    outcomes.push(format!("{}:{}", task_id, status));
                    continue;
                }
                let verified = runner
                    .verify(Json(AttemptTask {
                        mission_id: input.mission_id.clone(),
                        task_id: task_id.clone(),
                    }))
                    .call()
                    .await
                    .map_err(|e| TerminalError::new(format!("verify activity: {e}")))?;
                let vstatus = if verified.0 {
                    completed.insert(task_id.clone());
                    "completed"
                } else {
                    wave_failed = true;
                    "failed-verify"
                };
                outcomes.push(format!("{}:{}", task_id, vstatus));
            }
        }

        // Post-mission memory sync (opt-in) — journaled service call,
        // best-effort: sync failures stay inside evidence and must
        // never fail the workflow verdict, so the result is ignored.
        let _ = ctx
            .service_client::<AttemptRunnerClient>()
            .sync_memory(Json(input.mission_id.clone()))
            .call()
            .await;

        let summary = format!(
            "mission {} finished: {}",
            input.mission_id,
            outcomes.join(", ")
        );
        // Journaled side effect: the file store records the durable
        // orchestration verdict alongside the evidence trail.
        let mid = input.mission_id.clone();
        let s = summary.clone();
        let _ = ctx
            .run(move || async move {
                let dir = crate::data_dir().join("missions").join(&mid);
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::write(
                    dir.join("durable-result.json"),
                    serde_json::json!({
                        "summary": s,
                        "workflow_version": WORKFLOW_VERSION,
                        "ts": chrono::Utc::now().to_rfc3339(),
                    })
                    .to_string(),
                );
                Ok(())
            })
            .await;
        Ok(summary)
    }
}

// ---------- serve / submit / approve ----------

/// Version of the durable workflow semantics, stamped into every
/// `durable-result.json`. Bump when the workflow's journaled step
/// sequence changes incompatibly — in-flight workflows keep their
/// original semantics; migration policy is tracked in ADR-0003.
/// v2: sync_memory service call added before the verdict write.
/// Additive journaled calls replay compatibly on restate-sdk 0.12
/// (no patch API): in-flight v1 workflows simply gain the step.
const WORKFLOW_VERSION: u32 = 2;

/// Run the Restate endpoint. Requires a restate-server reachable at
/// RESTATE_ADMIN (default http://localhost:9070) for registration, and
/// ingress at RESTATE_INGRESS (default http://localhost:8080).
pub async fn serve() -> anyhow::Result<()> {
    HttpServer::new(
        Endpoint::builder()
            .bind(AttemptRunner)
            .bind(MissionDurable)
            .build(),
    )
    .listen_and_serve("0.0.0.0:9080".parse()?)
    .await;
    Ok(())
}

/// Submit a persisted mission as a durable workflow (fire-and-forget).
pub async fn submit(mission_id: &str, require_approval: bool) -> anyhow::Result<String> {
    let store = Store::new(crate::data_dir())?;
    let mission = store.load_mission(mission_id)?;

    let client = restate_sdk::ingress::ReqwestClient::connect(
        std::env::var("RESTATE_INGRESS")
            .unwrap_or_else(|_| "http://localhost:8080".into())
            .parse()?,
    )?;
    let wf = MissionDurableIngressClient::from_client(client, mission_id.to_string());

    let response = wf
        .run(Json(WorkflowInput {
            mission_id: mission_id.to_string(),
            tasks: mission.tasks.clone(),
            require_approval,
        }))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("submit failed: {e}"))?;

    Ok(format!(
        "workflow submitted: {} ({:?})",
        response.invocation_handle().invocation_id(),
        response.send_status()
    ))
}

/// Resolve a pending approval awakeable for a mission.
pub async fn approve(mission_id: &str, decision: &str) -> anyhow::Result<String> {
    let path = crate::data_dir()
        .join("missions")
        .join(mission_id)
        .join("approval.json");
    let raw = std::fs::read_to_string(&path)
        .map_err(|_| anyhow::anyhow!("no pending approval for {}", mission_id))?;
    let v: serde_json::Value = serde_json::from_str(&raw)?;
    let awakeable_id = v["awakeable_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("approval.json malformed"))?;

    let url = format!(
        "{}/restate/awakeables/{}/resolve",
        std::env::var("RESTATE_INGRESS").unwrap_or_else(|_| "http://localhost:8080".into()),
        awakeable_id
    );
    let resp = reqwest::Client::new()
        .post(&url)
        .header("content-type", "application/json")
        .body(format!("\"{}\"", decision))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "resolve failed: {}",
        resp.status()
    );
    Ok(format!("approval '{}' sent for {}", decision, mission_id))
}
