//! Local engine — in-process, NON-durable reference implementation of
//! the orchestration port.
//!
//! Runs the mission through the same supervised executor as `run`.
//! The approval gate is a plain file poll on `approval.json` — enough
//! for development and CI, NOT durable: a restart while parked simply
//! leaves the mission unrun (the gate file keeps the last known state).
//! It exists to prove the port in `mod.rs` is not Restate-shaped: a
//! second engine implements the same surface with different semantics,
//! and the CLI cannot tell the difference.

use crate::executor::Executor;
use crate::store::Store;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How often the parked gate re-reads `approval.json`.
const APPROVAL_POLL: Duration = Duration::from_millis(250);

/// Nothing to serve: the local engine has no orchestration endpoint.
pub async fn serve() -> Result<()> {
    anyhow::bail!("local engine runs in-process; there is no orchestration endpoint to serve")
}

fn approval_path(data_dir: &Path, mission_id: &str) -> PathBuf {
    data_dir
        .join("missions")
        .join(mission_id)
        .join("approval.json")
}

/// Mission ids become path segments — reject anything that could escape
/// the mission dir before they are joined into `approval.json`.
fn check_mission_id(mission_id: &str) -> Result<()> {
    if mission_id.is_empty() || mission_id.contains(['/', '\\']) || mission_id.contains("..") {
        anyhow::bail!("invalid mission id {mission_id:?}");
    }
    Ok(())
}

/// Submit = run the mission inline, synchronously. Durable engines
/// return a receipt immediately; this one only returns when the
/// mission is finished — the receipt says so honestly.
pub async fn submit(mission_id: &str, require_approval: bool) -> Result<String> {
    submit_in(&crate::data_dir(), mission_id, require_approval).await
}

/// [`submit`] against an explicit data dir — used by tests so they do
/// not have to mutate the process-wide `AUCTERIS_DATA_DIR`.
pub async fn submit_in(
    data_dir: &Path,
    mission_id: &str,
    require_approval: bool,
) -> Result<String> {
    check_mission_id(mission_id)?;
    let store = Store::new(data_dir)?;

    if require_approval {
        let path = approval_path(data_dir, mission_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            &path,
            serde_json::json!({"status": "awaiting_decision", "engine": "local"}).to_string(),
        )?;

        // Park until `approve` records a decision. There is no timeout:
        // the gate is operator-driven, and an abandoned gate leaves the
        // mission Pending — honest for a non-durable engine.
        let decision = loop {
            if let Ok(raw) = std::fs::read_to_string(&path)
                && let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw)
                && let Some(d) = v.get("decision").and_then(|d| d.as_str())
            {
                break d.to_string();
            }
            tokio::time::sleep(APPROVAL_POLL).await;
        };

        if decision != "approved" {
            return Ok(format!(
                "mission {} rejected at approval gate: {}",
                mission_id, decision
            ));
        }
        std::fs::write(
            &path,
            serde_json::json!({"decision": "approved", "engine": "local"}).to_string(),
        )?;
    }

    let exec = Executor::new(&store);
    exec.reconcile_interrupted(mission_id)?;
    exec.run_mission(mission_id)?;
    Ok(format!(
        "mission {} finished (local engine — non-durable, no retry)",
        mission_id
    ))
}

/// Resolve a pending approval gate for a mission.
pub async fn approve(mission_id: &str, decision: &str) -> Result<String> {
    approve_in(&crate::data_dir(), mission_id, decision).await
}

/// [`approve`] against an explicit data dir.
pub async fn approve_in(data_dir: &Path, mission_id: &str, decision: &str) -> Result<String> {
    check_mission_id(mission_id)?;
    match decision {
        "approved" | "rejected" => {}
        other => anyhow::bail!("decision must be 'approved' or 'rejected', got {other:?}"),
    }
    let path = approval_path(data_dir, mission_id);
    let raw = std::fs::read_to_string(&path)
        .map_err(|_| anyhow::anyhow!("no pending approval for {}", mission_id))?;
    let v: serde_json::Value = serde_json::from_str(&raw)?;
    anyhow::ensure!(
        v.get("decision").is_none(),
        "approval for {} already resolved",
        mission_id
    );
    std::fs::write(
        &path,
        serde_json::json!({"decision": decision, "engine": "local"}).to_string(),
    )?;
    Ok(format!(
        "approval '{}' recorded for {} (local engine)",
        decision, mission_id
    ))
}
