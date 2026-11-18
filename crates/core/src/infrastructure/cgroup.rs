//! cgroup v2 limits + kill for agent processes (Linux).
//!
//! Each attempt gets its own cgroup under the delegated user slice:
//!   <root>/aucteris/<attempt-id>/
//!
//! - `memory.max`, `cpu.max`, `pids.max` bound the whole subtree —
//!   descendants keep the cgroup even across setsid()/new process
//!   groups, which process-group kills cannot guarantee.
//! - `cgroup.kill` terminates every process in the cgroup atomically —
//!   the authoritative cleanup path, used by Drop and by orphan
//!   reconciliation (cgroup membership survives engine death).
//!
//! Delegated root candidates: $AUCTERIS_CGROUP_ROOT, then the systemd
//! user service slice (owned by the user on systemd systems). If no
//! writable delegated root exists the engine degrades to
//! process-group-only supervision — recorded honestly, never assumed.

use std::path::{Path, PathBuf};

const LIMITS: &[(&str, &str)] = &[
    ("memory.max", "4G"),
    ("cpu.max", "200000 100000"), // 2 CPUs
    ("pids.max", "512"),
];

fn delegated_root() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AUCTERIS_CGROUP_ROOT") {
        let p = PathBuf::from(p);
        if writable(&p) {
            return Some(p);
        }
    }
    // systemd user slice: writable + cpu/memory/pids delegated to us.
    let user = std::env::var("USER").unwrap_or_default();
    let uid = users_uid().unwrap_or(user);
    let candidate = PathBuf::from(format!(
        "/sys/fs/cgroup/user.slice/user-{uid}.slice/user@{uid}.service"
    ));
    if writable(&candidate) {
        Some(candidate)
    } else {
        None
    }
}

fn users_uid() -> Option<String> {
    // /proc/self loginuid is "4294967295" (unset) for services; fall back
    // to uid from /proc/self/status.
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(v) = line.strip_prefix("Uid:") {
            return v.split_whitespace().next().map(str::to_string);
        }
    }
    None
}

fn writable(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("cgroup.controllers"))
        .map(|c| c.contains("memory") || c.contains("cpu") || c.contains("pids"))
        .unwrap_or(false)
        && dir.metadata().is_ok_and(|m| !m.permissions().readonly())
}

fn parent_dir() -> Option<PathBuf> {
    let root = delegated_root()?;
    let parent = root.join("aucteris");
    std::fs::create_dir_all(&parent).ok()?;
    // Enable controllers for children. The `+` prefix is required —
    // a bare write tries to *replace* the whole set and fails EINVAL.
    let _ = std::fs::write(parent.join("cgroup.subtree_control"), "+cpu +memory +pids");
    Some(parent)
}

/// Sanitize an attempt id for use as a cgroup dir name.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// An active attempt cgroup. Created empty; `add_pid` moves the
/// supervised process in (children inherit membership automatically).
pub struct Cgroup {
    dir: PathBuf,
}

impl Cgroup {
    /// Create `<parent>/aucteris/<name>` with limits applied.
    /// Returns None when no delegated cgroup root is available.
    pub fn for_attempt(name: &str) -> Option<Self> {
        let parent = parent_dir()?;
        let dir = parent.join(sanitize(name));
        if dir.exists() {
            // Stale dir from a previous attempt with the same id —
            // clear it (kill leftovers first) and recreate.
            let _ = std::fs::write(dir.join("cgroup.kill"), "1");
            let _ = std::fs::remove_dir_all(&dir);
        }
        std::fs::create_dir(&dir).ok()?;
        for (file, val) in LIMITS {
            let _ = std::fs::write(dir.join(file), val);
        }
        Some(Self { dir })
    }

    /// Move a process into this cgroup.
    pub fn add_pid(&self, pid: u32) {
        let _ = std::fs::write(self.dir.join("cgroup.procs"), pid.to_string());
    }

    /// Kill every process in the cgroup (cgroup v2 `cgroup.kill`).
    /// Best-effort and idempotent.
    pub fn kill_all(&self) {
        let _ = std::fs::write(self.dir.join("cgroup.kill"), "1");
    }

    /// Same as `for_attempt` + kill, for orphan reconciliation where
    /// only the attempt id is known. Returns true if a live cgroup dir
    /// was found and killed.
    pub fn kill_named(name: &str) -> bool {
        let Some(parent) = parent_dir() else {
            return false;
        };
        let dir = parent.join(sanitize(name));
        // cgroup.kill needs kernel >= 5.14; absent file = unsupported.
        if !dir.join("cgroup.kill").exists() {
            return false;
        }
        let _ = std::fs::write(dir.join("cgroup.kill"), "1");
        true
    }
}

impl Drop for Cgroup {
    fn drop(&mut self) {
        // Ensure nothing survives the attempt, then release the dir.
        self.kill_all();
        for _ in 0..10 {
            if std::fs::remove_dir(&self.dir).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}
