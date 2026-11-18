//! Mission report — separates agent assertions, runtime observations,
//! independent verification, and unresolved uncertainty (doc 09 §4).

use crate::domain::*;
use crate::store::Store;
use anyhow::Result;
use std::fmt::Write as _;

pub fn build_report(store: &Store, mission_id: &str) -> Result<String> {
    let mission = store.load_mission(mission_id)?;
    let state = store.load_state(mission_id)?;
    let attempts = store.load_all_attempts(mission_id)?;
    let evidence = store.load_evidence(mission_id)?;

    let mut md = String::new();
    writeln!(md, "# Mission Report — {}", mission.id)?;
    writeln!(md)?;
    writeln!(md, "- **Objective:** {}", mission.objective)?;
    writeln!(md, "- **Status:** {:?}", mission.status)?;
    writeln!(md, "- **Created:** {}", mission.created_at)?;
    writeln!(md, "- **Workspace:** `{}`", mission.workspace.display())?;
    writeln!(md)?;

    writeln!(md, "## Tasks")?;
    writeln!(md, "| Task | Status | Attempts |")?;
    writeln!(md, "|---|---|---|")?;
    for r in &state {
        writeln!(
            md,
            "| {} | {:?} | {} |",
            r.task_id,
            r.status,
            r.attempts.len()
        )?;
    }
    writeln!(md)?;

    writeln!(md, "## Agent assertions (unverified claims)")?;
    let mut any = false;
    for a in &attempts {
        if let Some(text) = &a.agent_assertion {
            any = true;
            writeln!(md, "### Attempt {} (task {})", a.id, a.task_id)?;
            writeln!(md, "{}", text)?;
            writeln!(md)?;
        }
    }
    if !any {
        writeln!(md, "_None._")?;
    }
    writeln!(md)?;

    writeln!(md, "## Runtime observations")?;
    for e in &evidence {
        if let Evidence::RuntimeObservation { attempt_id, detail } = e {
            writeln!(md, "- `{}` {}: ", attempt_id, detail)?;
        }
    }
    writeln!(md)?;

    writeln!(md, "## Independent verification (run by Aucteris)")?;
    let mut verified = false;
    for e in &evidence {
        if let Evidence::VerificationResult {
            task_id,
            verify_cmd,
            exit_code,
            output_tail,
        } = e
        {
            verified = true;
            writeln!(md, "### Task {}", task_id)?;
            writeln!(md, "- Command: `{}`", verify_cmd)?;
            writeln!(md, "- Exit code: **{}**", exit_code)?;
            writeln!(md, "- Output tail:")?;
            writeln!(md, "```")?;
            writeln!(md, "{}", output_tail)?;
            writeln!(md, "```")?;
        }
    }
    if !verified {
        writeln!(md, "_No verification results._")?;
    }
    writeln!(md)?;

    writeln!(md, "## System observations / uncertainty")?;
    for e in &evidence {
        if let Evidence::SystemObservation { detail } = e {
            writeln!(md, "- {}", detail)?;
        }
    }
    let unknowns: Vec<_> = attempts
        .iter()
        .filter(|a| a.status == AttemptStatus::Unknown)
        .collect();
    if !unknowns.is_empty() {
        writeln!(md)?;
        writeln!(
            md,
            "**{} attempt(s) have UNKNOWN outcome** — the engine cannot establish \
             whether their external effects occurred. They are listed, not hidden.",
            unknowns.len()
        )?;
    }
    writeln!(md)?;

    writeln!(md, "## Declared runtime limitations")?;
    let mut seen = std::collections::BTreeSet::new();
    for a in &attempts {
        if seen.insert(a.runtime.clone()) {
            writeln!(md, "### {}", a.runtime)?;
            if let Some(iso) = &a.isolation {
                writeln!(md, "- Workspace isolation: `{}`", iso)?;
            }
            for l in &a.limitations {
                writeln!(md, "- {}", l)?;
            }
        }
    }
    if attempts.is_empty() {
        writeln!(md, "_No attempts recorded._")?;
    }
    writeln!(md)?;

    writeln!(md, "## Known limitations (slice 01)")?;
    writeln!(
        md,
        "- Persistence is a JSON file store, not the final durable design (ADR-0003 pending)."
    )?;
    // Engine sandbox state is per-attempt; summarize honestly.
    let sandboxed = attempts.iter().any(|a| {
        a.isolation
            .as_deref()
            .map(|i| i.contains("engine=bwrap"))
            .unwrap_or(false)
    });
    let degraded = attempts.iter().any(|a| {
        a.limitations
            .iter()
            .any(|l| l.contains("bubblewrap unavailable"))
    });
    if sandboxed {
        writeln!(
            md,
            "- Engine sandbox (bubblewrap) applied to attempts; fs reads confined to bound dirs, network egress remains runtime-dependent."
        )?;
    }
    if degraded {
        writeln!(
            md,
            "- Engine sandbox was REQUESTED but bubblewrap unavailable on this host; those attempts ran with runtime-only isolation."
        )?;
    }

    Ok(md)
}

pub fn write_report(store: &Store, mission_id: &str) -> Result<std::path::PathBuf> {
    let md = build_report(store, mission_id)?;
    let path = store.mission_dir(mission_id).join("report.md");
    std::fs::write(&path, md)?;
    Ok(path)
}
