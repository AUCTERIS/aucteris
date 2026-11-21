//! Integration tests driven by the deterministic `mock` runtime fixtures.
//!
//! These exercise the real pipeline end-to-end — YAML spec → store →
//! executor → supervised `sh` process → verify_cmd → evidence — with no
//! provider credentials. The fixtures live in `tests/fixtures/`.
//!
//! Tests that spawn supervised runtimes hold `SERIAL`: `cancel_active`
//! signals every tracked process group globally, so parallel tests
//! could kill each other's mock agents.

use aucteris_core::domain::{
    Attempt, AttemptStatus, Evidence, MissionSpec, MissionStatus, TaskSpec, TaskStatus,
};
use aucteris_core::executor::Executor;
use aucteris_core::infrastructure::process;
use aucteris_core::infrastructure::store::Store;
use aucteris_core::orchestrator::local;
use std::sync::Mutex;
use std::time::{Duration, Instant};

static SERIAL: Mutex<()> = Mutex::new(());

fn fixture(name: &str) -> MissionSpec {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let raw = std::fs::read_to_string(&path).expect("reading fixture");
    serde_yaml::from_str(&raw).expect("parsing fixture")
}

fn temp_store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("store");
    (dir, store)
}

fn mission_completed(store: &Store, mission_id: &str) -> bool {
    store
        .load_mission(mission_id)
        .map(|m| m.status == MissionStatus::Completed)
        .unwrap_or(false)
}

/// mission-mock-ok: the mock writes out.txt, the engine verifies it
/// itself, and the mission lands Completed with separated evidence.
#[test]
fn mock_ok_mission_completes_with_evidence() {
    let _serial = SERIAL.lock().unwrap();
    let (_dir, store) = temp_store();
    let mission = store
        .create_mission(&fixture("mission-mock-ok.yaml"))
        .unwrap();

    Executor::new(&store).run_mission(&mission.id).unwrap();

    let mission = store.load_mission(&mission.id).unwrap();
    assert_eq!(mission.status, MissionStatus::Completed);
    let state = store.load_state(&mission.id).unwrap();
    assert!(state.iter().all(|r| r.status == TaskStatus::Completed));

    let out = mission.workspace.join("out.txt");
    assert!(out.is_file(), "mock did not write out.txt");
    assert!(
        std::fs::read_to_string(out)
            .unwrap()
            .contains("mock output")
    );

    let evidence = store.load_evidence(&mission.id).unwrap();
    assert!(
        evidence
            .iter()
            .any(|e| matches!(e, Evidence::VerificationResult { exit_code: 0, .. })),
        "no passing verification evidence recorded"
    );
    assert!(
        evidence.iter().any(|e| matches!(
            e,
            Evidence::AgentAssertion { text, .. } if text.contains("mock assertion")
        )),
        "agent assertion missing from evidence"
    );
}

/// mission-mock-sleep: simulate an engine killed mid-attempt — a live
/// orphan process plus an Attempt stuck in Running — then prove
/// `reconcile_interrupted` marks it Unknown, kills the orphan, and the
/// resumed mission completes.
#[cfg(unix)]
#[test]
fn mock_sleep_recovers_after_engine_kill() {
    use std::os::unix::process::CommandExt;

    let _serial = SERIAL.lock().unwrap();
    let (_dir, store) = temp_store();
    let mission = store
        .create_mission(&fixture("mission-mock-sleep.yaml"))
        .unwrap();
    let task_id = mission.tasks[0].id.clone();

    // The "dead engine's" orphan: a process still alive whose cmdline
    // contains the runtime name, recorded as a Running attempt.
    let mut orphan = std::process::Command::new("sh")
        .args(["-c", "sleep 300", "mock"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .expect("spawn orphan");
    let orphan_pid = orphan.id();

    let attempt = Attempt {
        id: "a-slow-write-orphan".into(),
        task_id,
        runtime: "mock".into(),
        status: AttemptStatus::Running,
        started_at: chrono::Utc::now().to_rfc3339(),
        ended_at: None,
        exit_code: None,
        events_path: store.events_path(&mission.id, "a-slow-write-orphan"),
        agent_assertion: None,
        tool_calls: 0,
        isolation: None,
        limitations: vec![],
        pid: Some(orphan_pid),
        session_id: None,
    };
    store.save_attempt(&mission.id, &attempt).unwrap();

    let exec = Executor::new(&store);
    assert_eq!(exec.reconcile_interrupted(&mission.id).unwrap(), 1);

    let attempts = store.load_all_attempts(&mission.id).unwrap();
    let orphan_attempt = attempts
        .iter()
        .find(|a| a.id == "a-slow-write-orphan")
        .unwrap();
    assert_eq!(orphan_attempt.status, AttemptStatus::Unknown);

    // Reap the killed orphan, then confirm it is really gone.
    let _ = orphan.wait();
    assert!(!process::pid_alive(orphan_pid), "orphan survived reconcile");

    // Resume: the interrupted task is retried and the mission completes.
    exec.run_mission(&mission.id).unwrap();
    let mission = store.load_mission(&mission.id).unwrap();
    assert_eq!(mission.status, MissionStatus::Completed);
    assert!(mission.workspace.join("out.txt").is_file());
}

/// mission-mock-approval through the local engine's file gate: the
/// submit parks on approval.json, the mission does not run, then
/// `approve` unblocks it and the mission completes.
#[test]
fn mock_approval_gate_blocks_then_unblocks() {
    let _serial = SERIAL.lock().unwrap();
    let (dir, store) = temp_store();
    let mission = store
        .create_mission(&fixture("mission-mock-approval.yaml"))
        .unwrap();
    let mission_id = mission.id.clone();
    let data_dir = dir.path().to_path_buf();
    let approval_file = data_dir
        .join("missions")
        .join(&mission_id)
        .join("approval.json");

    let submitter = {
        let data_dir = data_dir.clone();
        let mission_id = mission_id.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(local::submit_in(&data_dir, &mission_id, true))
        })
    };

    // Wait for the gate to park, then prove nothing ran yet.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !approval_file.exists() {
        assert!(Instant::now() < deadline, "approval gate never parked");
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !mission.workspace.join("ok.txt").exists(),
        "mission ran before approval"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let receipt = rt
        .block_on(local::approve_in(&data_dir, &mission_id, "approved"))
        .unwrap();
    assert!(receipt.contains("approved"));

    let receipt = submitter.join().unwrap().unwrap();
    assert!(
        receipt.contains("finished"),
        "unexpected receipt: {receipt}"
    );
    assert!(mission_completed(&store, &mission_id));
    assert!(mission.workspace.join("ok.txt").is_file());
}

/// Same gate, rejected: the mission never executes and the receipt says
/// so honestly.
#[test]
fn mock_approval_rejection_skips_execution() {
    let _serial = SERIAL.lock().unwrap();
    let (dir, store) = temp_store();
    let mission = store
        .create_mission(&fixture("mission-mock-approval.yaml"))
        .unwrap();
    let mission_id = mission.id.clone();
    let data_dir = dir.path().to_path_buf();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let submitter = {
        let data_dir = data_dir.clone();
        let mission_id = mission_id.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(local::submit_in(&data_dir, &mission_id, true))
        })
    };

    let approval_file = data_dir
        .join("missions")
        .join(&mission_id)
        .join("approval.json");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !approval_file.exists() {
        assert!(Instant::now() < deadline, "approval gate never parked");
        std::thread::sleep(Duration::from_millis(50));
    }

    rt.block_on(local::approve_in(&data_dir, &mission_id, "rejected"))
        .unwrap();
    let receipt = submitter.join().unwrap().unwrap();
    assert!(
        receipt.contains("rejected"),
        "unexpected receipt: {receipt}"
    );

    // No attempt ever ran: the task is still pending, no files written.
    let attempts = store.load_all_attempts(&mission_id).unwrap();
    assert!(attempts.is_empty(), "rejected mission spawned attempts");
    assert!(!mission.workspace.join("ok.txt").exists());
}

/// Approving a mission with no parked gate is an explicit error, and a
/// bogus decision is rejected before touching the file.
#[test]
fn local_approve_requires_pending_gate() {
    let _serial = SERIAL.lock().unwrap();
    let (dir, store) = temp_store();
    let mission = store
        .create_mission(&fixture("mission-mock-ok.yaml"))
        .unwrap();
    let data_dir = dir.path().to_path_buf();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    assert!(
        rt.block_on(local::approve_in(&data_dir, &mission.id, "approved"))
            .is_err(),
        "approve succeeded without a pending gate"
    );
    assert!(
        rt.block_on(local::approve_in(&data_dir, "../escape", "approved"))
            .is_err(),
        "path-traversal mission id accepted"
    );
}

/// Dependency-gated waves: t2 can only verify once t1's output exists.
/// If waves ignored `depends_on`, t2's verify would race t1's write.
#[test]
fn executor_respects_task_dependencies() {
    let _serial = SERIAL.lock().unwrap();
    let (_dir, store) = temp_store();
    let task = |id: &str, prompt: &str, verify: &str, deps: Vec<&str>| TaskSpec {
        id: id.into(),
        title: id.into(),
        prompt: prompt.into(),
        verify_cmd: verify.into(),
        depends_on: deps.into_iter().map(str::to_string).collect(),
        runtime: "mock".into(),
        isolated: false,
    };
    let spec = MissionSpec {
        objective: "dependency wave test".into(),
        constraints: vec![],
        sandbox: false,
        allow_network: true,
        egress_filter: false,
        memory: false,
        memory_namespace: None,
        sync_memory: false,
        tool_approval: false,
        tasks: vec![
            task(
                "t1",
                "mock:write=a.txt write a",
                "grep -q 'mock output' a.txt",
                vec![],
            ),
            task(
                "t2",
                "mock:write=b.txt write b",
                "test -f a.txt && grep -q 'mock output' b.txt",
                vec!["t1"],
            ),
            task(
                "t3",
                "mock:write=c.txt write c",
                "grep -q 'mock output' c.txt",
                vec![],
            ),
        ],
    };
    let mission = store.create_mission(&spec).unwrap();

    Executor::new(&store).run_mission(&mission.id).unwrap();

    let mission = store.load_mission(&mission.id).unwrap();
    assert_eq!(mission.status, MissionStatus::Completed);
    for f in ["a.txt", "b.txt", "c.txt"] {
        assert!(
            mission.workspace.join(f).is_file(),
            "{f} missing from workspace"
        );
    }
}

/// Ctrl+C path: `cancel_active` sets the flag and kills the supervised
/// process tree; the executor must record the mission as Cancelled
/// rather than completing or failing it.
#[test]
fn cancel_active_marks_mission_cancelled() {
    let _serial = SERIAL.lock().unwrap();
    let (dir, store) = temp_store();
    let mission = store
        .create_mission(&fixture("mission-mock-sleep.yaml"))
        .unwrap();
    let mission_id = mission.id.clone();

    let runner = {
        let dir = dir.path().to_path_buf();
        let mission_id = mission_id.clone();
        std::thread::spawn(move || {
            let store = Store::new(&dir).unwrap();
            Executor::new(&store).run_mission(&mission_id).unwrap();
        })
    };

    // Wait until the mock attempt is actually Running, then cancel —
    // deterministic window inside the fixture's 20s sleep.
    let watcher = Store::new(dir.path()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let running = watcher
            .load_all_attempts(&mission_id)
            .map(|as_| as_.iter().any(|a| a.status == AttemptStatus::Running))
            .unwrap_or(false);
        if running {
            break;
        }
        assert!(Instant::now() < deadline, "attempt never started");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        process::cancel_active() > 0,
        "no supervised process to cancel"
    );

    runner.join().expect("executor thread panicked");

    let mission = watcher.load_mission(&mission_id).unwrap();
    assert_eq!(mission.status, MissionStatus::Cancelled);

    // Leave no global flag set for later tests (run_mission also resets
    // it on entry, but be explicit).
    process::CANCEL_REQUESTED.store(false, std::sync::atomic::Ordering::SeqCst);
}
