//! Engine-level sandbox (slice 02): bubblewrap-based filesystem
//! confinement applied to EVERY runtime adapter, not just those with
//! their own sandbox.
//!
//! Design decisions (measured, not assumed):
//! - bubblewrap (userns + mount/pid namespaces) is the industry-standard
//!   unprivileged sandbox; hand-rolling Landlock+seccomp is not worth it.
//! - The real boundary is $HOME: we ro-bind system dirs plus ONLY the
//!   host paths each adapter declares (install + config). Everything
//!   else under $HOME (~/.aws, ~/.ssh, documents) is invisible inside.
//! - Network stays available: agent CLIs need it for their own API.
//!   Egress control per-tool is the runtime's job (measured per adapter)
//!   or a future proxy — declared as a limitation, never hidden.
//! - If bwrap is unavailable the engine degrades to runtime-only
//!   isolation and SAYS SO in evidence. No silent downgrade.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Mount declarations an adapter needs inside the sandbox.
#[derive(Debug, Clone, Default)]
pub struct SandboxMounts {
    /// Host dirs bound read-only (install roots, read-only config).
    pub ro_dirs: Vec<PathBuf>,
    /// Host dirs bound read-WRITE (runtime state dirs it must update,
    /// e.g. ~/.codex logs/sessions). Sensitive files inside should be
    /// listed in `ro_files` to stay protected.
    pub rw_dirs: Vec<PathBuf>,
    /// Individual files bound read-only ON TOP of an rw parent —
    /// credentials/config the runtime reads but must never rewrite
    /// (auth.json, config.toml, hooks).
    pub ro_files: Vec<PathBuf>,
    /// Dirs masked by a fresh tmpfs AFTER all binds — for runtime
    /// caches that must not leak host state into the sandbox (or
    /// sandbox state back out), e.g. shell snapshots.
    pub tmpfs_over: Vec<PathBuf>,
}

/// What the engine sandboxes for one attempt.
#[derive(Debug, Clone)]
pub struct SandboxPlan {
    /// Agent workspace — the only writable project directory.
    pub workspace: PathBuf,
    /// Adapter-declared mounts (see SandboxMounts).
    pub mounts: SandboxMounts,
    /// Whether the sandboxed process gets network access. Agent CLIs
    /// need it for their API; per-tool egress remains runtime-dependent.
    pub allow_network: bool,
}

static BWRAP: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Locate bubblewrap once. None = engine sandbox unavailable.
pub fn bwrap_path() -> Option<&'static Path> {
    BWRAP
        .get_or_init(|| {
            // `bwrap` on PATH, or the `bubblewrap` binary name.
            let out = Command::new("bwrap")
                .arg("--version")
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|_| PathBuf::from("bwrap"));
            out.or_else(|| {
                Command::new("bubblewrap")
                    .arg("--version")
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|_| PathBuf::from("bubblewrap"))
            })
        })
        .as_deref()
}

/// System dirs every dynamically-linked agent runtime needs read-only.
/// Deliberately excludes $HOME — that is the security boundary.
const SYSTEM_RO_BINDS: &[&str] = &["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/opt"];

/// Expand ~ and canonicalize-ish (must exist on host).
fn host_dir(p: &Path) -> Option<PathBuf> {
    let s = p.to_string_lossy();
    let expanded = if let Some(rest) = s.strip_prefix("~/") {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(rest))?
    } else {
        p.to_path_buf()
    };
    expanded.is_dir().then_some(expanded)
}

/// Build the full argv: [bwrap, flags..., binds..., --, program, args...].
/// `program`/`args` are the UNSANDBOXED runtime invocation.
pub fn wrap_argv(plan: &SandboxPlan, program: &str, args: &[String]) -> Result<Vec<String>> {
    let bwrap = bwrap_path().ok_or_else(|| anyhow::anyhow!("bubblewrap not available"))?;

    let mut argv: Vec<String> = vec![
        bwrap.to_string_lossy().to_string(),
        // Kill the sandbox if the engine dies; isolate pid view.
        "--die-with-parent".into(),
        "--unshare-pid".into(),
        "--proc".into(),
        "/proc".into(),
        "--dev".into(),
        "/dev".into(),
        // Ephemeral scratch space. HOME keeps its real value: only the
        // adapter-declared subdirs materialize inside, so credential
        // discovery (~/.codex/auth.json) works while ~/.aws stays absent.
        "--tmpfs".into(),
        "/tmp".into(),
        // Deterministic shell for agent tool calls: never inherit the
        // user's interactive shell (its rc files aren't bound anyway,
        // but runtime shell-snapshots could replay host env).
        "--setenv".into(),
        "SHELL".into(),
        "/bin/bash".into(),
        "--chdir".into(),
        plan.workspace.display().to_string(),
    ];
    if !plan.allow_network {
        argv.push("--unshare-net".into());
    }

    for d in SYSTEM_RO_BINDS {
        if Path::new(d).is_dir() {
            argv.push("--ro-bind".into());
            argv.push((*d).to_string());
            argv.push((*d).to_string());
        }
    }
    for d in &plan.mounts.rw_dirs {
        if let Some(dir) = host_dir(d) {
            argv.push("--bind".into());
            argv.push(dir.display().to_string());
            argv.push(dir.display().to_string());
        }
    }
    for d in &plan.mounts.ro_dirs {
        if let Some(dir) = host_dir(d) {
            argv.push("--ro-bind".into());
            argv.push(dir.display().to_string());
            argv.push(dir.display().to_string());
        }
    }
    // Sensitive files stay read-only even inside an rw dir. Applied
    // after parents so the overlay wins.
    for f in &plan.mounts.ro_files {
        if f.is_file() {
            argv.push("--ro-bind".into());
            argv.push(f.display().to_string());
            argv.push(f.display().to_string());
        }
    }
    // tmpfs masks applied last so they win over any parent bind.
    for d in &plan.mounts.tmpfs_over {
        if let Some(dir) = host_dir(d) {
            argv.push("--tmpfs".into());
            argv.push(dir.display().to_string());
        }
    }
    // Workspace is the ONLY writable project dir.
    argv.push("--bind".into());
    argv.push(plan.workspace.display().to_string());
    argv.push(plan.workspace.display().to_string());

    argv.push("--".into());
    argv.push(program.to_string());
    argv.extend(args.iter().cloned());
    Ok(argv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_binds_workspace_and_system_dirs() {
        let plan = SandboxPlan {
            workspace: PathBuf::from("/tmp/ws"),
            mounts: SandboxMounts {
                ro_dirs: vec![PathBuf::from("/nonexistent-should-skip")],
                ..Default::default()
            },
            allow_network: true,
        };
        // bwrap may be absent on CI-less envs; test argv shape only if present.
        if bwrap_path().is_none() {
            return;
        }
        let argv = wrap_argv(&plan, "codex", &["exec".into()]).unwrap();
        let s = argv.join(" ");
        assert!(s.contains("--die-with-parent"));
        assert!(s.contains("--unshare-pid"));
        assert!(s.contains("--bind /tmp/ws /tmp/ws"));
        assert!(s.contains("--ro-bind /usr /usr"));
        assert!(!s.contains("--unshare-net"));
        assert!(!s.contains("nonexistent-should-skip"));
    }

    #[test]
    fn argv_unshares_net_when_denied() {
        let plan = SandboxPlan {
            workspace: PathBuf::from("/tmp/ws"),
            mounts: SandboxMounts::default(),
            allow_network: false,
        };
        if bwrap_path().is_none() {
            return;
        }
        let argv = wrap_argv(&plan, "codex", &[]).unwrap();
        assert!(argv.iter().any(|a| a == "--unshare-net"));
    }
}
