//! Temporal adapter for the orchestration port (ADR-0003).
//!
//! Second durable engine behind the same port as `restate.rs` — proves
//! the boundary is real, not Restate-shaped. All Temporal SDK mechanics
//! (worker, workflow macros, activity contexts) live here.
//!
//! Architecture (temporalio-sdk 1.2.0, server 1.32 dev-mode):
//! - `MissionTemporal` (#[workflow], task queue `aucteris-missions`) owns
//!   the lifecycle: approval gate via signal, dependency-gated parallel
//!   waves (concurrent `execute_activity` futures joined), sequential
//!   verifies, honest stop on failed/unknown.
//! - `MissionActivities` (#[activities]) executes ONE attempt through
//!   the same supervised path as Restate (`reconcile_interrupted`
//!   first, so a Temporal retry after worker death can never duplicate
//!   an agent). Activities heartbeat every 10s so a dead worker is
//!   detected in ~heartbeat_timeout, not after the full timeout.
//! - `serve` = Temporal *worker* (polls the task queue; there is no
//!   HTTP endpoint like Restate's). `submit` = typed client
//!   `start_workflow`; `approve` = typed `signal` to the workflow id.
//!
//! Commands (AUCTERIS_ENGINE=temporal):
//!   aucteris serve                  run the Temporal worker
//!   aucteris run-durable <yaml>     persist mission, start workflow
//!   aucteris approve <mission> <d>  signal the approval decision
//!
//! Requires a Temporal frontend at TEMPORAL_ADDRESS
//! (default 127.0.0.1:7233) — e.g. `temporal server start-dev`.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use temporalio_client::{
    Client, ClientOptions, Connection, RetryPolicy, WorkflowSignalOptions, WorkflowStartOptions,
    envconfig::LoadClientConfigProfileOptions,
};
use temporalio_macros::{activities, workflow, workflow_methods};
use temporalio_sdk::{
    ActivityOptions, Runtime, Worker, WorkerOptions, WorkflowContext, WorkflowResult,
    activities::{ActivityContext, ActivityError},
};

use crate::domain::{AttemptStatus, TaskSpec};
use crate::executor::Executor;
use crate::store::Store;

const TASK_QUEUE: &str = "aucteris-missions";
/// Attempts legitimately run for minutes; heartbeats (10s cadence,
/// 60s timeout) bound dead-worker detection — the long
/// start_to_close is only a backstop.
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(4 * 3600);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(60);

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
    #[serde(default)]
    pub require_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistInput {
    pub mission_id: String,
    pub summary: String,
    pub rejected: bool,
}

fn store() -> anyhow::Result<&'static Store> {
    static STORE: std::sync::OnceLock<Store> = std::sync::OnceLock::new();
    if let Some(s) = STORE.get() {
        return Ok(s);
    }
    let s = Store::new(crate::data_dir())?;
    Ok(STORE.get_or_init(|| s))
}

fn activity_err<E: Into<anyhow::Error>>(e: E) -> ActivityError {
    ActivityError::application(temporalio_sdk::ApplicationFailure::new(e.into()))
}

// ---------- activities ----------

pub struct MissionActivities;

#[activities]
impl MissionActivities {
    /// Run ONE attempt end-to-end. Reconciles first (Temporal retries
    /// this activity after worker death): the orphaned Running attempt
    /// becomes Unknown and its tree is killed BEFORE anything respawns.
    /// Blocking executor work runs on spawn_blocking; the async shell
    /// heartbeats every 10s so Temporal detects a dead worker in ~60s.
    #[activity]
    pub async fn run_attempt(
        ctx: ActivityContext,
        input: AttemptTask,
    ) -> Result<AttemptOutcome, ActivityError> {
        let mission_id = input.mission_id.clone();
        let task_id = input.task_id.clone();
        let mut work = tokio::task::spawn_blocking(move || -> anyhow::Result<AttemptOutcome> {
            let exec = Executor::new(store()?);
            exec.reconcile_interrupted(&mission_id)?;
            let mission = store()?.load_mission(&mission_id)?;
            let spec = mission
                .tasks
                .iter()
                .find(|t| t.id == task_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?;
            // Resume the agent session a previous attempt reported.
            let store = store()?;
            let mut state = store.load_state(&mission_id)?;
            let resume = state
                .iter()
                .find(|r| r.task_id == task_id)
                .and_then(|r| r.session_id.clone());
            let attempt = exec.execute_attempt(&mission, &spec, resume)?;
            if let Some(session_id) = &attempt.session_id
                && let Some(run) = state.iter_mut().find(|r| r.task_id == task_id)
            {
                run.session_id = Some(session_id.clone());
                store.save_state(&mission_id, &state)?;
            }
            let status = match attempt.status {
                AttemptStatus::Succeeded => "succeeded",
                AttemptStatus::Failed => "failed",
                AttemptStatus::Unknown => "unknown",
                _ => "other",
            };
            Ok(AttemptOutcome {
                attempt_id: attempt.id,
                status: status.to_string(),
            })
        });

        let mut hb = tokio::time::interval(Duration::from_secs(10));
        let joined = loop {
            tokio::select! {
                r = &mut work => break r,
                _ = hb.tick() => {
                    if let Err(e) = ctx.record_heartbeat(()).await {
                        tracing::warn!("heartbeat failed: {e}");
                    }
                }
            }
        };
        joined
            .map_err(|e| activity_err(anyhow::anyhow!("activity join: {e}")))?
            .map_err(activity_err)
    }

    /// Independent verification — separate journaled activity, run by
    /// the engine (never the agent). Same workspace semantics as the
    /// attempt (isolated tasks verify inside iso/<id>/).
    #[activity]
    pub async fn verify(ctx: ActivityContext, input: AttemptTask) -> Result<bool, ActivityError> {
        let mission_id = input.mission_id.clone();
        let task_id = input.task_id.clone();
        let mut work = tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
            let exec = Executor::new(store()?);
            let mission = store()?.load_mission(&mission_id)?;
            let spec = mission
                .tasks
                .iter()
                .find(|t| t.id == task_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?;
            exec.verify_task(&mission, &spec)
        });
        let mut hb = tokio::time::interval(Duration::from_secs(10));
        let joined = loop {
            tokio::select! {
                r = &mut work => break r,
                _ = hb.tick() => {
                    if let Err(e) = ctx.record_heartbeat(()).await {
                        tracing::warn!("heartbeat failed: {e}");
                    }
                }
            }
        };
        joined
            .map_err(|e| activity_err(anyhow::anyhow!("activity join: {e}")))?
            .map_err(activity_err)
    }

    /// Post-mission memory sync (opt-in `sync_memory`): pushes
    /// <mission>/memory to Memexa via the aucteris-memory binary.
    /// Best-effort by design — maybe_sync_memory records every outcome
    /// as evidence and never fails; this activity therefore always
    /// succeeds (load errors are swallowed the same way).
    #[activity]
    pub async fn post_mission_sync(
        _ctx: ActivityContext,
        mission_id: String,
    ) -> Result<(), ActivityError> {
        let _ = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let exec = Executor::new(store()?);
            if let Ok(mission) = store()?.load_mission(&mission_id) {
                exec.maybe_sync_memory(&mission);
            }
            Ok(())
        })
        .await;
        Ok(())
    }

    /// Journaled side effect: mark the mission as awaiting an approval
    /// decision (`approval.json`), same contract as the Restate awakeable
    /// write — lets observers (e.g. aucteris-companion) see the parked
    /// gate without querying Temporal.
    #[activity]
    pub async fn mark_approval_pending(
        _ctx: ActivityContext,
        mission_id: String,
    ) -> Result<(), ActivityError> {
        let dir = crate::data_dir().join("missions").join(&mission_id);
        std::fs::create_dir_all(&dir).map_err(activity_err)?;
        std::fs::write(
            dir.join("approval.json"),
            serde_json::json!({"status": "awaiting_decision", "engine": "temporal"}).to_string(),
        )
        .map_err(activity_err)
    }

    /// Journaled side effect: record the approval decision so observers
    /// stop showing the mission as awaiting a decision while the waves
    /// run (rejection is already recorded by `persist_result`).
    #[activity]
    pub async fn resolve_approval(
        _ctx: ActivityContext,
        mission_id: String,
    ) -> Result<(), ActivityError> {
        let dir = crate::data_dir().join("missions").join(&mission_id);
        std::fs::create_dir_all(&dir).map_err(activity_err)?;
        std::fs::write(
            dir.join("approval.json"),
            serde_json::json!({"decision": "approved", "engine": "temporal"}).to_string(),
        )
        .map_err(activity_err)
    }

    /// Journaled side effect: persist the orchestration verdict next
    /// to the evidence trail (`durable-result.json` / `approval.json`),
    /// same contract as the Restate adapter.
    #[activity]
    pub async fn persist_result(
        _ctx: ActivityContext,
        input: PersistInput,
    ) -> Result<(), ActivityError> {
        let dir = crate::data_dir().join("missions").join(&input.mission_id);
        std::fs::create_dir_all(&dir).map_err(activity_err)?;
        if input.rejected {
            std::fs::write(
                dir.join("approval.json"),
                serde_json::json!({"decision": "rejected", "engine": "temporal"}).to_string(),
            )
            .map_err(activity_err)?;
        }
        std::fs::write(
            dir.join("durable-result.json"),
            serde_json::json!({
                "summary": input.summary,
                "engine": "temporal",
                "workflow_version": WORKFLOW_VERSION,
                "ts": chrono::Utc::now().to_rfc3339(),
            })
            .to_string(),
        )
        .map_err(activity_err)
    }
}

// ---------- workflow ----------

#[workflow]
#[derive(Default)]
pub struct MissionTemporal {
    /// None = awaiting decision; Some(true/false) = approved/rejected.
    decision: Option<bool>,
}

#[workflow_methods]
impl MissionTemporal {
    #[run]
    pub async fn run(
        ctx: &mut WorkflowContext<Self>,
        input: WorkflowInput,
    ) -> WorkflowResult<String> {
        if input.require_approval {
            // Park until `aucteris approve` signals a decision.
            // Patch-gated: v2 histories replaying under this code must
            // not see a command their journal lacks.
            if ctx.patched("approval-pending-marker") {
                ctx.execute_activity(
                    MissionActivities::mark_approval_pending,
                    input.mission_id.clone(),
                    quick_opts(),
                )
                .await?;
            }
            ctx.wait_condition(|s| s.decision.is_some()).await?;
            let approved = ctx.state(|s| s.decision.unwrap_or(false));
            if approved && ctx.patched("approval-resolve-marker") {
                ctx.execute_activity(
                    MissionActivities::resolve_approval,
                    input.mission_id.clone(),
                    quick_opts(),
                )
                .await?;
            }
            if !approved {
                let summary = format!("mission {} rejected at approval gate", input.mission_id);
                ctx.execute_activity(
                    MissionActivities::persist_result,
                    PersistInput {
                        mission_id: input.mission_id.clone(),
                        summary: summary.clone(),
                        rejected: true,
                    },
                    quick_opts(),
                )
                .await?;
                return Ok(summary);
            }
        }

        // Dependency-gated parallel waves (max 4): concurrent
        // execute_activity futures joined — Temporal journals each
        // activity scheduling, so replay stays deterministic.
        // Verifies run SEQUENTIALLY after the join (same measured
        // rationale as Restate: concurrency buys nothing on cheap
        // verify_cmds and keeps scheduling simple).
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
            // join_all preserves order → results[i] pairs with ready[i].
            let results = futures_util::future::join_all(ready.iter().map(|task| {
                let mission_id = mission_id.clone();
                let task_id = task.id.clone();
                ctx.execute_activity(
                    MissionActivities::run_attempt,
                    AttemptTask {
                        mission_id,
                        task_id,
                    },
                    attempt_opts(),
                )
            }))
            .await;

            for (task, r) in ready.iter().zip(results) {
                let outcome = r?;
                if outcome.status != "succeeded" {
                    wave_failed = true;
                    outcomes.push(format!("{}:{}", task.id, outcome.status));
                    continue;
                }
                let task_id = task.id.clone();
                let verified = ctx
                    .execute_activity(
                        MissionActivities::verify,
                        AttemptTask {
                            mission_id: input.mission_id.clone(),
                            task_id: task_id.clone(),
                        },
                        quick_opts(),
                    )
                    .await?;
                let vstatus = if verified {
                    completed.insert(task_id.clone());
                    "completed"
                } else {
                    wave_failed = true;
                    "failed-verify"
                };
                outcomes.push(format!("{task_id}:{vstatus}"));
            }
        }

        // Post-mission memory sync (opt-in) — gated by patched() so
        // workflows started before this step replay deterministically:
        // v1 histories carry no marker -> patched() returns false ->
        // the step is skipped on replay, matching their journal.
        // Best-effort: failures stay inside evidence, never fail
        // the workflow verdict.
        if ctx.patched("post-mission-memory-sync") {
            ctx.execute_activity(
                MissionActivities::post_mission_sync,
                input.mission_id.clone(),
                quick_opts(),
            )
            .await?;
        }

        let summary = format!(
            "mission {} finished: {}",
            input.mission_id,
            outcomes.join(", ")
        );
        ctx.execute_activity(
            MissionActivities::persist_result,
            PersistInput {
                mission_id: input.mission_id.clone(),
                summary: summary.clone(),
                rejected: false,
            },
            quick_opts(),
        )
        .await?;
        Ok(summary)
    }

    /// Human approval gate: `aucteris approve <mission> approved|rejected`.
    #[signal]
    pub fn decide(&mut self, _ctx: &mut temporalio_sdk::SyncWorkflowContext<Self>, approved: bool) {
        self.decision = Some(approved);
    }
}

fn attempt_opts() -> ActivityOptions {
    ActivityOptions::with_start_to_close_timeout(ATTEMPT_TIMEOUT)
        .heartbeat_timeout(HEARTBEAT_TIMEOUT)
        .retry_policy(
            RetryPolicy::builder()
                .initial_interval(Duration::from_secs(2))
                .backoff_coefficient(2.0)
                .maximum_interval(Duration::from_secs(60))
                .maximum_attempts(5)
                .non_retryable_error_types(Vec::<String>::new())
                .build(),
        )
        .build()
}

fn quick_opts() -> ActivityOptions {
    ActivityOptions::with_start_to_close_timeout(Duration::from_secs(300))
        .retry_policy(
            RetryPolicy::builder()
                .initial_interval(Duration::from_secs(1))
                .backoff_coefficient(2.0)
                .maximum_attempts(3)
                .non_retryable_error_types(Vec::<String>::new())
                .build(),
        )
        .build()
}

// ---------- serve / submit / approve ----------

/// Version of durable workflow semantics — same contract as Restate's
/// WORKFLOW_VERSION; bump on incompatible journaled-step changes.
/// v2: post_mission_sync step, gated by ctx.patched() so v1 histories
/// replay without it.
const WORKFLOW_VERSION: u32 = 2;

/// TEMPORAL_ADDRESS (default 127.0.0.1:7233) is honored by envconfig.
async fn connect() -> anyhow::Result<Client> {
    let (conn_opts, client_opts) =
        ClientOptions::load_from_config(LoadClientConfigProfileOptions::default())
            .map_err(|e| anyhow::anyhow!("temporal client config: {e}"))?;
    let connection = Connection::connect(conn_opts).await?;
    Ok(Client::new(connection, client_opts)?)
}

/// Run the Temporal worker: polls TASK_QUEUE, executes workflow +
/// activities. Blocks until killed — durability lives in Temporal
/// server (event history), not in this process.
pub async fn serve() -> anyhow::Result<()> {
    let runtime = Runtime::from_current_tokio(Default::default())?;
    let client = connect().await?;
    let worker_options = WorkerOptions::new(TASK_QUEUE)
        .register_workflow::<MissionTemporal>()?
        .register_activities(MissionActivities)
        .build();
    let mut worker = Worker::new(&runtime, client, worker_options)?;
    tracing::info!("temporal worker polling {TASK_QUEUE}");
    worker.run().await?;
    Ok(())
}

/// Start the mission workflow (fire-and-forget — the workflow itself
/// persists durable-result.json via the persist_result activity).
pub async fn submit(mission_id: &str, require_approval: bool) -> anyhow::Result<String> {
    let store = Store::new(crate::data_dir())?;
    let mission = store.load_mission(mission_id)?;
    let client = connect().await?;

    let handle = client
        .start_workflow(
            MissionTemporal::run,
            WorkflowInput {
                mission_id: mission_id.to_string(),
                tasks: mission.tasks.clone(),
                require_approval,
            },
            WorkflowStartOptions::new(TASK_QUEUE, mission_id.to_string()).build(),
        )
        .await?;

    Ok(format!(
        "workflow started: {} (run_id: {:?})",
        mission_id,
        handle.run_id()
    ))
}

/// Signal the approval decision to a parked workflow.
pub async fn approve(mission_id: &str, decision: &str) -> anyhow::Result<String> {
    let approved = match decision {
        "approved" => true,
        "rejected" => false,
        other => anyhow::bail!("decision must be 'approved' or 'rejected', got {other:?}"),
    };
    let client = connect().await?;
    let handle = client.get_workflow_handle::<MissionTemporal>(mission_id.to_string());
    handle
        .signal(
            MissionTemporal::decide,
            approved,
            WorkflowSignalOptions::default(),
        )
        .await?;
    Ok(format!("approval '{decision}' signaled for {}", mission_id))
}
