//! Mission executor — sequential task loop for slice 01.
//!
//! For each task: create Attempt (status=Running, persisted BEFORE spawn
//! so a crash leaves honest evidence) -> supervise runtime -> run
//! verify_cmd OURSELVES -> record evidence -> update statuses.
//!
//! Recovery: on entry, any Attempt still marked Running means the engine
//! died mid-flight. We mark it Unknown — we do NOT assume success or
//! failure, and we do NOT silently retry (doc 08 §5).

use crate::application::ports::runtime::{RunRequest, RuntimeEvent, RuntimeRegistry};
use crate::domain::*;
use crate::infrastructure::{process, store::Store};
use anyhow::Result;
use std::time::Instant;
use std::{process::Command, sync::Arc};

/// One normalized runtime event, tagged with the work item that emitted it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionEvent {
    pub mission_id: String,
    pub task_id: String,
    pub attempt_id: String,
    pub runtime: String,
    pub event: RuntimeEvent,
}

pub struct Executor<'a> {
    pub store: &'a Store,
    runtimes: RuntimeRegistry,
    event_sink: Option<Arc<dyn Fn(ExecutionEvent) + Send + Sync>>,
}

impl<'a> Executor<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self {
            store,
            runtimes: RuntimeRegistry::default(),
            event_sink: None,
        }
    }

    /// Construct an executor with adapters supplied by the embedding harness.
    pub fn with_registry(store: &'a Store, runtimes: RuntimeRegistry) -> Self {
        Self {
            store,
            runtimes,
            event_sink: None,
        }
    }

    /// Construct an executor that forwards normalized runtime events live.
    pub fn with_event_sink(
        store: &'a Store,
        runtimes: RuntimeRegistry,
        event_sink: impl Fn(ExecutionEvent) + Send + Sync + 'static,
    ) -> Self {
        Self {
            store,
            runtimes,
            event_sink: Some(Arc::new(event_sink)),
        }
    }

    /// Mark attempts left `Running` by a previous engine life as Unknown.
    /// Called at startup of any mission operation (recovery drill).
    pub fn reconcile_interrupted(&self, mission_id: &str) -> Result<usize> {
        let mut fixed = 0;
        for mut a in self.store.load_all_attempts(mission_id)? {
            if a.status == AttemptStatus::Running {
                a.status = AttemptStatus::Unknown;
                a.ended_at = Some(chrono::Utc::now().to_rfc3339());
                self.store.save_attempt(mission_id, &a)?;
                self.store.append_evidence(
                    mission_id,
                    &Evidence::SystemObservation {
                        detail: format!(
                            "attempt {} was Running at engine restart; marked unknown — effect state not established",
                            a.id
                        ),
                    },
                )?;
                fixed += 1;
            }
        }

        // Orphan sweep: an attempt marked Unknown may still have a live
        // agent process (engine died, child survived). Kill its process
        // group — but only if the pid still looks like our runtime.
        for a in self.store.load_all_attempts(mission_id)? {
            if a.status != AttemptStatus::Unknown {
                continue;
            }
            // cgroup.kill reaches every process in the attempt subtree —
            // including setsid() escapees that process-group kills miss.
            // Membership survives engine death, so this still works here.
            #[cfg(target_os = "linux")]
            let cg_killed = crate::cgroup::Cgroup::kill_named(&a.id);
            #[cfg(not(target_os = "linux"))]
            let cg_killed = false;

            let pid = a.pid.or_else(|| self.read_pid_file(mission_id, &a.id));
            let Some(pid) = pid else {
                if cg_killed {
                    self.store.append_evidence(
                        mission_id,
                        &Evidence::SystemObservation {
                            detail: format!(
                                "attempt {} orphan cgroup killed (no pid on record)",
                                a.id
                            ),
                        },
                    )?;
                }
                continue;
            };
            if process::pid_alive(pid) && process::pid_looks_like_runtime(pid, &a.runtime) {
                let (killed, escapees) = process::kill_process_tree(pid, libc::SIGKILL);
                self.store.append_evidence(
                    mission_id,
                    &Evidence::SystemObservation {
                        detail: format!(
                            "orphan agent pgid={} ({}) was still alive; killpg={} escapees_killed={} cgroup_kill={}",
                            pid, a.runtime, killed, escapees, cg_killed
                        ),
                    },
                )?;
            } else if cg_killed {
                self.store.append_evidence(
                    mission_id,
                    &Evidence::SystemObservation {
                        detail: format!("attempt {} orphan cgroup killed", a.id),
                    },
                )?;
            }
        }
        Ok(fixed)
    }

    /// PID sidecar written by the runtime engine at spawn time.
    fn read_pid_file(&self, mission_id: &str, attempt_id: &str) -> Option<u32> {
        let p = self
            .store
            .mission_dir(mission_id)
            .join("attempts")
            .join(format!("{}.pid", attempt_id));
        std::fs::read_to_string(p).ok()?.trim().parse().ok()
    }

    /// Run pending/failed/unknown tasks of a mission.
    /// Tasks whose dependencies are all Completed run in parallel waves
    /// (bounded); each attempt keeps its own sandbox/cgroup/evidence.
    /// Tasks already Completed are skipped (idempotent re-entry).
    ///
    /// HONEST LIMIT: parallel tasks share the mission workspace — like
    /// `make -j`, conflicting writes are the spec author's problem.
    /// Per-task workspaces are future work.
    pub fn run_mission(&self, mission_id: &str) -> Result<()> {
        /// Max concurrent attempts per wave.
        const MAX_PARALLEL: usize = 4;

        process::CANCEL_REQUESTED.store(false, std::sync::atomic::Ordering::SeqCst);

        let mission = self.store.load_mission(mission_id)?;
        let mut state = self.store.load_state(mission_id)?;
        self.reconcile_interrupted(mission_id)?;

        let mut mission = mission;
        mission.status = MissionStatus::Running;
        self.store.save_mission(&mission)?;

        loop {
            if process::CANCEL_REQUESTED.load(std::sync::atomic::Ordering::SeqCst) {
                mission.status = MissionStatus::Cancelled;
                self.store.save_mission(&mission)?;
                self.store.append_evidence(
                    mission_id,
                    &Evidence::SystemObservation {
                        detail:
                            "mission cancelled by operator (Ctrl+C); remaining tasks not started"
                                .into(),
                    },
                )?;
                return Ok(());
            }

            // Ready = not finished, deps all Completed (spec order).
            let ready: Vec<usize> = mission
                .tasks
                .iter()
                .enumerate()
                .filter(|(_, spec)| {
                    state
                        .iter()
                        .find(|r| r.task_id == spec.id)
                        .map(|r| r.status != TaskStatus::Completed)
                        .unwrap_or(false)
                })
                .filter(|(_, spec)| {
                    spec.depends_on.iter().all(|d| {
                        state
                            .iter()
                            .find(|r| &r.task_id == d)
                            .map(|r| r.status == TaskStatus::Completed)
                            .unwrap_or(false)
                    })
                })
                .map(|(i, _)| i)
                .take(MAX_PARALLEL)
                .collect();
            if ready.is_empty() {
                break;
            }

            for &i in &ready {
                state[i].status = TaskStatus::Running;
            }
            self.store.save_state(mission_id, &state)?;

            // Parallel wave: attempt + independent verify per task.
            // State is only mutated after the join — no races.
            let results = std::thread::scope(|s| -> Result<Vec<(usize, Attempt, bool)>> {
                let m = &mission;
                let handles: Vec<_> = ready
                    .iter()
                    .map(|&i| {
                        let spec = &m.tasks[i];
                        s.spawn(move || {
                            let attempt = self.execute_attempt(m, spec)?;
                            let verified = attempt.status == AttemptStatus::Succeeded
                                && self.verify_task(m, spec)?;
                            Ok::<(usize, Attempt, bool), anyhow::Error>((i, attempt, verified))
                        })
                    })
                    .collect();
                let mut out = Vec::with_capacity(handles.len());
                for h in handles {
                    out.push(
                        h.join()
                            .map_err(|_| anyhow::anyhow!("attempt thread panicked"))??,
                    );
                }
                Ok(out)
            })?;

            if process::CANCEL_REQUESTED.load(std::sync::atomic::Ordering::SeqCst) {
                // Attempts were killed via their process groups/cgroups;
                // effect state is uncertain — record honestly, stop.
                let n = results.len();
                for (i, attempt, _) in results {
                    state[i].attempts.push(attempt.id.clone());
                    state[i].status = TaskStatus::Cancelled;
                }
                self.store.save_state(mission_id, &state)?;
                mission.status = MissionStatus::Cancelled;
                self.store.save_mission(&mission)?;
                self.store.append_evidence(
                    mission_id,
                    &Evidence::SystemObservation {
                        detail: format!(
                            "mission cancelled; {} in-flight attempt(s) outcome uncertain (process groups killed)",
                            n
                        ),
                    },
                )?;
                return Ok(());
            }

            for (i, attempt, verified) in results {
                state[i].attempts.push(attempt.id.clone());
                state[i].status = if verified {
                    TaskStatus::Completed
                } else {
                    TaskStatus::Failed
                };
            }
            self.store.save_state(mission_id, &state)?;
        }

        // Anything still unfinished had unmet/failed deps — mark
        // honestly as Blocked rather than silently Pending.
        for r in state.iter_mut() {
            if r.status == TaskStatus::Pending || r.status == TaskStatus::Running {
                r.status = TaskStatus::Blocked;
            }
        }
        self.store.save_state(mission_id, &state)?;

        mission.status = if state.iter().all(|r| r.status == TaskStatus::Completed) {
            MissionStatus::Completed
        } else if state.iter().any(|r| r.status == TaskStatus::Failed) {
            MissionStatus::Failed
        } else {
            MissionStatus::Blocked
        };
        self.store.save_mission(&mission)?;

        // Post-mission memory sync (opt-in): push <mission>/memory to
        // Memexa. Runs for every final status — failure lessons are
        // memories too. Never fails the mission: outcome is evidence.
        self.maybe_sync_memory(&mission);
        Ok(())
    }

    /// Push the mission's local memory store to Memexa when the spec
    /// opted in (`sync_memory`). Delegates to the aucteris-memory
    /// binary's `sync push` — no sync logic duplicated here. All
    /// outcomes (including "not configured") are appended as system
    /// observations so the report shows exactly what happened.
    pub(crate) fn maybe_sync_memory(&self, mission: &Mission) {
        let note = |detail: String| {
            let _ = self
                .store
                .append_evidence(&mission.id, &Evidence::SystemObservation { detail });
        };
        if !mission.memory || !mission.sync_memory {
            return;
        }
        let Some(cfg) = std::env::var_os("AUCTERIS_MEMORY_SYNC_CONFIG") else {
            note("sync_memory requested but AUCTERIS_MEMORY_SYNC_CONFIG is not set".into());
            return;
        };
        let bin = std::env::var("AUCTERIS_MEMORY_BIN").unwrap_or_else(|_| "aucteris-memory".into());
        let bin_path = process::resolve_on_path(&bin);
        if !std::path::Path::new(&bin_path).is_file() {
            note(format!(
                "sync_memory requested but binary {bin:?} not found"
            ));
            return;
        }
        let memory_dir = self.store.mission_dir(&mission.id).join("memory");
        let out = Command::new(&bin_path)
            .args(["sync", "push", "--config"])
            .arg(&cfg)
            .env("AUCTERIS_MEMORY_DIR", &memory_dir)
            .output();
        match out {
            Ok(o) => {
                let tail = |b: &[u8]| {
                    String::from_utf8_lossy(b)
                        .chars()
                        .rev()
                        .take(500)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect::<String>()
                };
                note(format!(
                    "post-mission memory sync: exit={} stdout_tail={:?} stderr_tail={:?}",
                    o.status.code().unwrap_or(-1),
                    tail(&o.stdout),
                    tail(&o.stderr)
                ));
            }
            Err(e) => note(format!("post-mission memory sync failed to spawn: {e}")),
        }
    }

    pub(crate) fn execute_attempt(&self, mission: &Mission, spec: &TaskSpec) -> Result<Attempt> {
        // Attempt ids must be unique even for same-second parallel
        // retries — same collision class as the mission-id bug.
        let base_id = format!("a-{}-{}", spec.id, chrono::Utc::now().format("%H%M%S"));
        let mut attempt_id = base_id.clone();
        let mut suffix = 2;
        while self.store.attempt_path(&mission.id, &attempt_id).exists() {
            attempt_id = format!("{}-{}", base_id, suffix);
            suffix += 1;
        }
        let events_path = self.store.events_path(&mission.id, &attempt_id);

        let mut attempt = Attempt {
            id: attempt_id.clone(),
            task_id: spec.id.clone(),
            runtime: spec.runtime.clone(),
            status: AttemptStatus::Running,
            started_at: chrono::Utc::now().to_rfc3339(),
            ended_at: None,
            exit_code: None,
            events_path: events_path.clone(),
            agent_assertion: None,
            tool_calls: 0,
            isolation: None,
            limitations: vec![],
            pid: None,
        };
        // Persist BEFORE spawning: a crash now leaves an honest Running record.
        self.store.save_attempt(&mission.id, &attempt)?;
        self.store.append_evidence(
            &mission.id,
            &Evidence::SystemObservation {
                detail: format!(
                    "attempt {} started for task {} (runtime={})",
                    attempt_id, spec.id, spec.runtime
                ),
            },
        )?;

        let mut prompt = format!(
            "You are working in an isolated workspace for task \"{}\" of mission \"{}\".\n\
             Objective: {}\n\nTask: {}\n\n{}\n\
             Rules: work only inside the current directory; do not access files outside it; \
             do not use the network; when done, print a short summary of what you changed.",
            spec.id, mission.id, mission.objective, spec.title, spec.prompt
        );

        // Declare what this runtime can actually enforce (doc 04 §3).
        let adapter = self.runtimes.get(&spec.runtime)?;
        let caps = adapter.capabilities();

        // Per-task workspace: isolated tasks get `<mission>/iso/<id>/`
        // as their sandbox root — they cannot see sibling outputs.
        let task_ws = task_workspace(mission, spec);
        if spec.isolated {
            std::fs::create_dir_all(&task_ws)?;
        }

        // Engine sandbox: requested by mission policy; applied only if
        // bubblewrap is available — and the applied mode is recorded,
        // never assumed (P10). `mounts` is mutable: memory injection
        // below may add the memory DB dir + server binary binds.
        let mut sandbox_plan = mission.sandbox.then(|| crate::sandbox::SandboxPlan {
            workspace: task_ws.clone(),
            mounts: adapter.sandbox_mounts(),
            // Mission policy: deny egress => bwrap --unshare-net.
            // Per-tool egress inside an allowed network stays the
            // runtime's job (measured per adapter).
            allow_network: mission.allow_network,
        });

        // Egress allowlist proxy: only when the mission opted in AND
        // network is allowed — deny-all already covers the rest.
        // The adapter declares which domains it legitimately needs.
        let egress_filter =
            (mission.egress_filter && mission.allow_network).then(|| adapter.egress_domains());
        let sandbox_desc = match (&sandbox_plan, crate::sandbox::bwrap_path()) {
            (Some(_), Some(_)) => "bwrap(fs-ro+pid-ns)",
            (Some(_), None) => "UNAVAILABLE(degraded to runtime-only)",
            (None, _) => "disabled by mission spec",
        };
        attempt.isolation = Some(format!(
            "{:?}|engine={}",
            caps.workspace_isolation, sandbox_desc
        ));
        attempt.limitations = caps.limitations.clone();
        if sandbox_desc.starts_with("UNAVAILABLE") {
            attempt
                .limitations
                .push("engine sandbox requested but bubblewrap unavailable on this host".into());
        }

        // Persistent memory (opt-in mission policy): the engine writes
        // an MCP servers config into the task workspace and, inside the
        // sandbox, binds the memory DB dir rw + the server binary ro.
        // Adapters without MCP support get an honest limitation — the
        // request is never silently dropped.
        let mut mcp_plan = None;
        if mission.memory {
            match memory_mcp_setup(
                self.store,
                mission,
                &task_ws,
                sandbox_plan.as_mut(),
                adapter.supports_mcp(),
            ) {
                Ok(Some(plan)) => {
                    mcp_plan = Some(plan);
                    attempt.limitations.push(
                        "memory MCP injected; aucteris-memory is unmediated (agent can write/delete its own memories)".into(),
                    );
                }
                Ok(None) => {}
                Err(e) => {
                    attempt
                        .limitations
                        .push(format!("memory requested but setup failed: {e}"));
                }
            }
        }
        if mcp_plan.is_some() {
            prompt.push_str(&format!(
                "\n\nYou have persistent memory tools (remember/recall/forget/list). \
                 Use namespace \"{}\" for mission-scoped memories; recall relevant \
                 memories before starting non-trivial work.",
                mission.memory_namespace.as_deref().unwrap_or(&mission.id)
            ));
        }
        self.store.append_evidence(
            &mission.id,
            &Evidence::SystemObservation {
                detail: format!(
                    "attempt {} adapter={} engine_sandbox={} capabilities={:?} limitations={:?}",
                    attempt_id,
                    adapter.name(),
                    sandbox_desc,
                    caps.workspace_isolation,
                    caps.limitations
                ),
            },
        )?;

        let pid_file = self
            .store
            .mission_dir(&mission.id)
            .join("attempts")
            .join(format!("{}.pid", attempt_id));

        let start = Instant::now();
        let event_sink = self.event_sink.as_ref().map(|sink| {
            let sink = Arc::clone(sink);
            let mission_id = mission.id.clone();
            let task_id = spec.id.clone();
            let attempt_id = attempt_id.clone();
            let runtime_name = spec.runtime.clone();
            Arc::new(move |event| {
                sink(ExecutionEvent {
                    mission_id: mission_id.clone(),
                    task_id: task_id.clone(),
                    attempt_id: attempt_id.clone(),
                    runtime: runtime_name.clone(),
                    event,
                })
            }) as Arc<dyn Fn(RuntimeEvent) + Send + Sync>
        });
        let outcome = adapter.run(RunRequest {
            workspace: task_ws.clone(),
            prompt,
            events_path: events_path.clone(),
            max_turns: 30,
            pid_file: Some(pid_file),
            sandbox: sandbox_plan,
            cgroup_name: Some(attempt.id.clone()),
            egress_filter,
            mcp: mcp_plan,
            event_sink,
        });
        let elapsed = start.elapsed().as_secs();

        match outcome {
            Ok(o) => {
                attempt.status = if o.exit_code == 0 {
                    AttemptStatus::Succeeded
                } else {
                    AttemptStatus::Failed
                };
                attempt.exit_code = Some(o.exit_code);
                attempt.tool_calls = o.tool_calls;
                attempt.agent_assertion = o.agent_assertion.clone();
                attempt.pid = o.pid;
                attempt.ended_at = Some(chrono::Utc::now().to_rfc3339());

                self.store.append_evidence(
                    &mission.id,
                    &Evidence::AgentAssertion {
                        attempt_id: attempt.id.clone(),
                        text: o.agent_assertion.unwrap_or_else(|| "<none>".into()),
                    },
                )?;
                let usage = o
                    .usage
                    .map(|(i, out)| format!(" in_tokens={} out_tokens={}", i, out))
                    .unwrap_or_default();
                self.store.append_evidence(
                    &mission.id,
                    &Evidence::RuntimeObservation {
                        attempt_id: attempt.id.clone(),
                        detail: format!(
                            "exit_code={} tool_calls={} duration={}s{} cgroup_limited={}{}",
                            o.exit_code,
                            o.tool_calls,
                            elapsed,
                            usage,
                            o.cgroup_limited,
                            o.egress
                                .map(|(a, d)| format!(" egress_allowed={} egress_denied={}", a, d))
                                .unwrap_or_default()
                        ),
                    },
                )?;
            }
            Err(e) => {
                attempt.status = AttemptStatus::Failed;
                attempt.ended_at = Some(chrono::Utc::now().to_rfc3339());
                self.store.append_evidence(
                    &mission.id,
                    &Evidence::SystemObservation {
                        detail: format!("runtime spawn/wait error for {}: {}", attempt.id, e),
                    },
                )?;
            }
        }

        self.store.save_attempt(&mission.id, &attempt)?;
        Ok(attempt)
    }

    /// Aucteris runs the verification itself — independent of the agent.
    /// Runs in the task's own workspace (isolated tasks verify inside
    /// their `iso/<id>/` dir).
    pub(crate) fn verify_task(&self, mission: &Mission, spec: &TaskSpec) -> Result<bool> {
        let output = Command::new("sh")
            .args(["-c", &spec.verify_cmd])
            .current_dir(task_workspace(mission, spec))
            .output()?;

        let code = output.status.code().unwrap_or(-1);
        let tail = String::from_utf8_lossy(&output.stdout)
            .chars()
            .rev()
            .take(2000)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        let stderr_tail = String::from_utf8_lossy(&output.stderr)
            .chars()
            .rev()
            .take(2000)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();

        self.store.append_evidence(
            &mission.id,
            &Evidence::VerificationResult {
                task_id: spec.id.clone(),
                verify_cmd: spec.verify_cmd.clone(),
                exit_code: code,
                output_tail: format!("stdout:\n{}\nstderr:\n{}", tail, stderr_tail),
            },
        )?;

        Ok(code == 0)
    }
}

/// Workspace a task runs in: isolated tasks get `<mission>/iso/<id>/`,
/// shared tasks the mission workspace itself.
fn task_workspace(mission: &Mission, spec: &TaskSpec) -> std::path::PathBuf {
    if spec.isolated {
        mission.workspace.join("iso").join(&spec.id)
    } else {
        mission.workspace.clone()
    }
}

/// Engine-side setup for mission memory: resolves the aucteris-memory
/// binary, writes `{"mcpServers": {...}}` into `<task_ws>/.aucteris/`,
/// and — when the engine sandbox is active — binds the per-mission
/// memory dir rw and the binary's parent dir ro so the MCP server
/// (spawned by the runtime INSIDE bwrap) can reach both.
///
/// Returns the config path for `RunRequest::mcp_config_path`, or None
/// with the reason pushed into attempt limitations by the caller.
/// Never fails the attempt: memory is an enhancement, not a gate.
fn memory_mcp_setup(
    store: &Store,
    mission: &Mission,
    task_ws: &std::path::Path,
    sandbox_plan: Option<&mut crate::sandbox::SandboxPlan>,
    adapter_supports_mcp: bool,
) -> Result<Option<crate::application::ports::runtime::McpPlan>> {
    if !adapter_supports_mcp {
        anyhow::bail!("runtime adapter does not support MCP injection");
    }
    let bin = std::env::var("AUCTERIS_MEMORY_BIN").unwrap_or_else(|_| "aucteris-memory".into());
    let bin_path = process::resolve_on_path(&bin);
    if !std::path::Path::new(&bin_path).is_file() {
        anyhow::bail!("aucteris-memory binary not found (looked for {bin:?} -> {bin_path})");
    }

    // Per-mission memory DB: shared by all tasks of the mission,
    // isolated from other missions and from the user's global store.
    let memory_dir = store.mission_dir(&mission.id).join("memory");
    std::fs::create_dir_all(&memory_dir)?;

    let cfg_dir = task_ws.join(".aucteris");
    std::fs::create_dir_all(&cfg_dir)?;
    let cfg_path = cfg_dir.join("mcp.json");
    let cfg = serde_json::json!({
        "mcpServers": {
            "aucteris-memory": {
                "command": bin_path,
                "args": [],
                "env": { "AUCTERIS_MEMORY_DIR": memory_dir }
            }
        }
    });
    std::fs::write(&cfg_path, serde_json::to_string_pretty(&cfg)?)?;

    if let Some(plan) = sandbox_plan {
        plan.mounts.rw_dirs.push(memory_dir.clone());
        if let Some(parent) = std::path::Path::new(&bin_path).parent() {
            plan.mounts.ro_dirs.push(parent.to_path_buf());
        }
    }
    Ok(Some(crate::application::ports::runtime::McpPlan {
        config_path: cfg_path,
        name: "aucteris-memory".into(),
        command: bin_path,
        env: vec![(
            "AUCTERIS_MEMORY_DIR".into(),
            memory_dir.display().to_string(),
        )],
    }))
}
