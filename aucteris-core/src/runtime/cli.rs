//! Generic engine for process-CLI runtimes.
//!
//! Handles the parts every CLI adapter shares: spawn with piped stdio,
//! stream stdout line-by-line into the events file, drain stderr on a
//! thread (pipe deadlock), wait, return exit status.
//!
//! Per-runtime differences (argv, event parsing, permission flags) live
//! in the spec modules — NOT here.

use crate::runtime::{RunOutcome, RunRequest};
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

/// PGIDs of runtime processes currently being supervised. On Unix each
/// child is spawned with process_group(0), so pgid == pid and
/// `kill(-pgid)` reaches the whole descendant tree. Multiple entries:
/// parallel mission waves supervise several agents at once.
pub static ACTIVE_PGIDS: Mutex<BTreeSet<u32>> = Mutex::new(BTreeSet::new());

/// Snapshot of supervised pgids — for the Ctrl+C handler.
pub fn active_pgids() -> Vec<u32> {
    ACTIVE_PGIDS
        .lock()
        .map(|s| s.iter().copied().collect())
        .unwrap_or_default()
}

fn track_pgid(pgid: u32) {
    if let Ok(mut s) = ACTIVE_PGIDS.lock() {
        s.insert(pgid);
    }
}

fn untrack_pgid(pgid: u32) {
    if let Ok(mut s) = ACTIVE_PGIDS.lock() {
        s.remove(&pgid);
    }
}

/// Removes the pgid from ACTIVE_PGIDS on ANY exit path — including `?`
/// early returns. A stale entry is worse than a lost one: the Ctrl+C
/// handler could otherwise signal a recycled pid's process group.
/// Orphaned children remain covered by the pid sidecar + cgroup.kill.
struct PgidGuard(u32);
impl Drop for PgidGuard {
    fn drop(&mut self) {
        untrack_pgid(self.0);
    }
}

/// Set by the Ctrl+C handler; the executor checks it between steps.
pub static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Request cancellation and terminate all currently supervised runtime trees.
/// Returns the number of process groups for which a signal was sent.
pub fn cancel_active() -> usize {
    CANCEL_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
    active_pgids()
        .into_iter()
        .filter(|pgid| kill_process_tree(*pgid, libc::SIGKILL).0)
        .count()
}

/// Kill a process group (negative pid). Returns true if signal sent.
pub fn kill_process_group(pgid: u32, sig: i32) -> bool {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pgid as i32), sig) == 0
    }
    #[cfg(not(unix))]
    {
        let _ = (pgid, sig);
        false
    }
}

/// PIDs of every process whose PGID equals `pgid` (Linux /proc scan).
#[cfg(target_os = "linux")]
fn pids_in_group(pgid: u32) -> Vec<u32> {
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir("/proc") {
        for e in rd.flatten() {
            let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            if let Ok(stat) = std::fs::read_to_string(e.path().join("stat")) {
                // Field 5 (pgrp) sits after the last ')' — comm may contain spaces.
                if let Some(rest) = stat.rsplit(')').next() {
                    let f: Vec<&str> = rest.split_whitespace().collect();
                    // rest starts at field 3 (state); pgrp is field 5 => index 2.
                    if f.get(2).and_then(|s| s.parse::<u32>().ok()) == Some(pgid) {
                        out.push(pid);
                    }
                }
            }
        }
    }
    out
}

/// Descendants of `roots` via PPID walk (Linux /proc scan).
/// Catches children that escaped the process group via setsid() —
/// observed with real runtimes (codex spawns helpers in their own group).
#[cfg(target_os = "linux")]
fn descendant_pids(roots: &[u32]) -> Vec<u32> {
    use std::collections::{HashMap, HashSet, VecDeque};
    let mut children_of: HashMap<u32, Vec<u32>> = HashMap::new();
    if let Ok(rd) = std::fs::read_dir("/proc") {
        for e in rd.flatten() {
            let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            if let Ok(status) = std::fs::read_to_string(e.path().join("status")) {
                for line in status.lines() {
                    if let Some(v) = line.strip_prefix("PPid:") {
                        if let Ok(ppid) = v.trim().parse::<u32>() {
                            children_of.entry(ppid).or_default().push(pid);
                        }
                        break;
                    }
                }
            }
        }
    }
    let mut seen: HashSet<u32> = roots.iter().copied().collect();
    let mut queue: VecDeque<u32> = roots.iter().copied().collect();
    let mut out = vec![];
    while let Some(p) = queue.pop_front() {
        if let Some(kids) = children_of.get(&p) {
            for &k in kids {
                if seen.insert(k) {
                    out.push(k);
                    queue.push_back(k);
                }
            }
        }
    }
    out
}

/// Kill a runtime's whole tree: the process group AND any descendants
/// that escaped it via setsid(). Returns (group_killed, extra_pids_killed).
/// Best-effort: a child spawned between scan and signal can still slip.
pub fn kill_process_tree(pgid: u32, sig: i32) -> (bool, usize) {
    #[cfg(target_os = "linux")]
    {
        let members = pids_in_group(pgid);
        let roots: Vec<u32> = if members.is_empty() {
            vec![pgid]
        } else {
            members
        };
        let escapees = descendant_pids(&roots);
        let group_ok = kill_process_group(pgid, sig);
        let mut extra = 0;
        for pid in escapees {
            #[cfg(unix)]
            unsafe {
                if libc::kill(pid as i32, sig) == 0 {
                    extra += 1;
                }
            }
        }
        (group_ok, extra)
    }
    #[cfg(not(target_os = "linux"))]
    {
        (kill_process_group(pgid, sig), 0)
    }
}

/// Is a pid alive? (kill(pid, 0) probe — does not signal.)
pub fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as i32, 0) == 0 || *libc::__errno_location() == libc::EPERM
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// Best-effort check that a live pid actually looks like our runtime
/// (guards against PID reuse killing an unrelated process group).
/// Linux reads /proc; elsewhere we trust kill-0 only (documented gap).
pub fn pid_looks_like_runtime(pid: u32, runtime: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        let needle = match runtime {
            "claude-code" => "claude",
            other => other,
        };
        std::fs::read(format!("/proc/{}/cmdline", pid))
            .map(|b| String::from_utf8_lossy(&b).contains(needle))
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (pid, runtime);
        true
    }
}

/// Specification of one CLI-based runtime.
pub struct CliSpec {
    /// Binary name resolved on PATH.
    pub program: &'static str,
    /// Build argv from the request. Index 0 is the subcommand if needed.
    pub build_args: fn(&RunRequest) -> Vec<String>,
    /// Parse one stdout line: raw line is ALWAYS persisted first by the
    /// engine; this only extracts the normalized view.
    pub parse_line: fn(&str) -> Vec<crate::runtime::RuntimeEvent>,
}

/// Resolve `program` to an absolute path via the engine's PATH.
/// Needed because inside the sandbox the PATH dirs (e.g. ~/.local/bin)
/// are not bound — we exec the canonical location instead.
pub(crate) fn resolve_on_path(program: &str) -> String {
    if program.contains('/') {
        return program.to_string();
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let cand = dir.join(program);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if cand
                    .metadata()
                    .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
                {
                    // Resolve symlinks: inside the sandbox only the
                    // canonical location (under an adapter bind) exists.
                    return cand
                        .canonicalize()
                        .unwrap_or(cand)
                        .to_string_lossy()
                        .to_string();
                }
            }
            #[cfg(not(unix))]
            if cand.is_file() {
                return cand.to_string_lossy().to_string();
            }
        }
    }
    program.to_string()
}

/// Run a CLI runtime per spec. Returns observed outcome.
pub fn run_cli(spec: &CliSpec, req: &RunRequest) -> Result<RunOutcome> {
    let events_file = File::create(&req.events_path)
        .with_context(|| format!("creating {}", req.events_path.display()))?;
    let events_writer = events_file.try_clone()?;

    let args = (spec.build_args)(req);
    let program = resolve_on_path(spec.program);

    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .current_dir(&req.workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Engine sandbox: wrap the whole invocation in bubblewrap when a
    // plan was requested and bwrap is available. Otherwise degrade to
    // runtime-only isolation — the executor records which happened.
    if let Some(plan) = &req.sandbox {
        if let Ok(argv) = crate::sandbox::wrap_argv(plan, &program, &args) {
            let mut wrapped = Command::new(&argv[0]);
            wrapped
                .args(&argv[1..])
                .current_dir(&req.workspace)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            cmd = wrapped;
        }
    }

    // Own process group: enables killing the whole descendant tree.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    // Egress filtering: local CONNECT proxy + *_PROXY env injection —
    // MUST be set before spawn. Best-effort mediation, see
    // src/egress.rs for the honest limits. The guard lives until
    // run_cli returns; Drop stops the listener.
    let egress = req
        .egress_filter
        .as_ref()
        .and_then(|domains| crate::egress::EgressProxy::start(domains.clone()).ok());
    if let Some(p) = &egress {
        let url = format!("http://{}", p.addr);
        for var in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ] {
            cmd.env(var, &url);
        }
        cmd.env("NO_PROXY", "localhost,127.0.0.1")
            .env("no_proxy", "localhost,127.0.0.1");
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawning {}", spec.program))?;
    track_pgid(child.id());
    let _pgid_guard = PgidGuard(child.id());
    if let Some(pf) = &req.pid_file {
        let _ = std::fs::write(pf, child.id().to_string());
    }

    // cgroup v2: move the child (and its whole future subtree) into a
    // per-attempt cgroup for resource limits + authoritative kill.
    // Guard lives until this function returns — Drop kills leftovers.
    #[cfg(target_os = "linux")]
    let cgroup = req
        .cgroup_name
        .as_deref()
        .and_then(crate::cgroup::Cgroup::for_attempt);
    #[cfg(target_os = "linux")]
    if let Some(c) = &cgroup {
        c.add_pid(child.id());
    }
    #[cfg(not(target_os = "linux"))]
    let cgroup: Option<()> = None;

    let stdout = child.stdout.take().context("stdout pipe")?;
    let stderr = child.stderr.take().context("stderr pipe")?;

    let stderr_thread = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = BufReader::new(stderr).read_to_string(&mut buf);
        buf
    });

    let mut writer = std::io::BufWriter::new(events_writer);
    let mut tool_calls = 0u32;
    let mut assertion: Option<String> = None;
    let mut usage: Option<(u64, u64)> = None;

    for line in BufReader::new(stdout).lines() {
        let line = line?;
        let _ = writeln!(writer, "{}", line); // raw evidence first

        for ev in (spec.parse_line)(&line) {
            if let Some(sink) = &req.event_sink {
                sink(ev.clone());
            }
            match ev {
                crate::runtime::RuntimeEvent::ToolCall { .. } => tool_calls += 1,
                crate::runtime::RuntimeEvent::FinalResult { text } => assertion = Some(text),
                crate::runtime::RuntimeEvent::Usage {
                    input_tokens,
                    output_tokens,
                } => usage = Some((input_tokens, output_tokens)),
                _ => {}
            }
        }
    }
    let _ = writer.flush();

    let status = child.wait().context("waiting for runtime")?;
    let stderr_tail = stderr_thread.join().unwrap_or_default();
    if !stderr_tail.trim().is_empty() {
        let _ = writeln!(
            writer,
            "{}",
            serde_json::json!({"type":"_stderr_tail","data": stderr_tail.chars().take(4000).collect::<String>()})
        );
    }

    Ok(RunOutcome {
        exit_code: status.code().unwrap_or(-1),
        tool_calls,
        agent_assertion: assertion,
        usage,
        pid: Some(child.id()),
        cgroup_limited: cgroup.is_some(),
        egress: egress.as_ref().map(|p| p.counts()),
    })
}

/// Convenience: build a RunRequest for tests/fixtures.
#[allow(dead_code)]
pub fn request_for(workspace: &Path, prompt: &str, events: &Path) -> RunRequest {
    RunRequest {
        workspace: workspace.to_path_buf(),
        prompt: prompt.to_string(),
        events_path: events.to_path_buf(),
        max_turns: 30,
        pid_file: None,
        sandbox: None,
        cgroup_name: None,
        egress_filter: None,
        mcp: None,
        event_sink: None,
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    use std::time::Duration;

    /// A runtime child that calls setsid() escapes the parent's process
    /// group (observed with real codex helpers). kill_process_tree must
    /// still reach it via the PPID walk.
    #[test]
    fn kill_process_tree_reaches_setsid_escapee() {
        let mut child = Command::new("sh")
            .args(["-c", "setsid sleep 300 & exec sleep 300"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .expect("spawn test child");
        let pgid = child.id();

        // Wait for a TRUE escapee: a descendant whose PGID differs from
        // the group (setsid). In-group descendants die with killpg and
        // would make the test vacuous.
        let pgrp_of = |pid: u32| -> Option<u32> {
            let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid)).ok()?;
            let rest = stat.rsplit(')').next()?;
            rest.split_whitespace().nth(2)?.parse().ok()
        };
        let mut escapee = None;
        for _ in 0..50 {
            for cand in descendant_pids(&[pgid]) {
                if pgrp_of(cand).is_some_and(|g| g != pgid) {
                    escapee = Some(cand);
                    break;
                }
            }
            if escapee.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let escapee = escapee.expect("setsid grandchild never appeared");
        assert!(pid_alive(escapee));

        let (group_ok, _extra) = kill_process_tree(pgid, libc::SIGKILL);
        assert!(group_ok);

        // Reap our direct child (a zombie still answers kill(pid, 0)).
        let _ = child.wait();
        std::thread::sleep(Duration::from_millis(300));
        assert!(!pid_alive(pgid), "group leader still alive");
        assert!(!pid_alive(escapee), "setsid escapee still alive");
    }
}
