//! Local engine — in-process, NON-durable reference implementation of
//! the orchestration port.
//!
//! Runs the mission through the same supervised executor as `run`,
//! with no durability, no approval gate and no retry. It exists to
//! prove the port in `mod.rs` is not Restate-shaped: a second engine
//! implements the same surface with different semantics, and the CLI
//! cannot tell the difference.

use crate::executor::Executor;
use crate::store::Store;
use anyhow::Result;

/// Nothing to serve: the local engine has no orchestration endpoint.
pub async fn serve() -> Result<()> {
    anyhow::bail!("local engine runs in-process; there is no orchestration endpoint to serve")
}

/// Submit = run the mission inline, synchronously. Durable engines
/// return a receipt immediately; this one only returns when the
/// mission is finished — the receipt says so honestly.
pub async fn submit(mission_id: &str, require_approval: bool) -> Result<String> {
    if require_approval {
        anyhow::bail!(
            "local engine has no durable approval gate; resubmit with AUCTERIS_ENGINE=restate for --approval missions"
        );
    }
    let store = Store::new(crate::data_dir())?;
    let exec = Executor::new(&store);
    exec.reconcile_interrupted(mission_id)?;
    exec.run_mission(mission_id)?;
    Ok(format!(
        "mission {} finished (local engine — non-durable, no retry)",
        mission_id
    ))
}

/// No parked awakeable exists to resolve.
pub async fn approve(_mission_id: &str, _decision: &str) -> Result<String> {
    anyhow::bail!("local engine has no durable approval gate")
}
