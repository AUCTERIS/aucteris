//! Engine-neutral orchestration port (ADR-0003 condition).
//!
//! Restate is an *adapter* behind this port, not a leak into the domain.
//! The port covers the mission lifecycle control surface: serve the
//! orchestration endpoint, submit a persisted mission, resolve a human
//! approval gate. Workflow/activity internals are engine-specific by
//! nature — Temporal or a custom journal would implement the same port
//! with their own durable primitives.
//!
//! Engine selection: `AUCTERIS_ENGINE` env var (default `restate`).
//! Unknown engines are an explicit error — never a silent fallback.

pub mod local;
pub mod restate;
pub mod temporal;

use anyhow::Result;

/// Supported orchestration engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    Restate,
    /// In-process, non-durable reference engine — proves the port is
    /// engine-neutral (ADR-0003) without external infrastructure.
    Local,
    /// Temporal engine — second durable adapter (worker + typed
    /// client). Requires a Temporal frontend (TEMPORAL_ADDRESS).
    Temporal,
}

impl EngineKind {
    /// Resolve the engine from `AUCTERIS_ENGINE` (default: restate).
    pub fn from_env() -> Result<Self> {
        match std::env::var("AUCTERIS_ENGINE").as_deref() {
            Ok("restate") | Err(_) => Ok(Self::Restate),
            Ok("local") => Ok(Self::Local),
            Ok("temporal") => Ok(Self::Temporal),
            Ok(other) => anyhow::bail!(
                "unknown orchestration engine {:?}; supported: [\"restate\", \"local\", \"temporal\"]",
                other
            ),
        }
    }
}

/// Run the engine's orchestration endpoint (blocking server loop).
pub async fn serve(kind: EngineKind) -> Result<()> {
    match kind {
        EngineKind::Restate => restate::serve().await,
        EngineKind::Local => local::serve().await,
        EngineKind::Temporal => temporal::serve().await,
    }
}

/// Submit a persisted mission for durable execution (fire-and-forget).
/// Returns a human-readable receipt including the engine invocation id.
pub async fn submit(kind: EngineKind, mission_id: &str, require_approval: bool) -> Result<String> {
    match kind {
        EngineKind::Restate => restate::submit(mission_id, require_approval).await,
        EngineKind::Local => local::submit(mission_id, require_approval).await,
        EngineKind::Temporal => temporal::submit(mission_id, require_approval).await,
    }
}

/// Resolve a pending human-approval gate for a mission.
pub async fn approve(kind: EngineKind, mission_id: &str, decision: &str) -> Result<String> {
    match kind {
        EngineKind::Restate => restate::approve(mission_id, decision).await,
        EngineKind::Local => local::approve(mission_id, decision).await,
        EngineKind::Temporal => temporal::approve(mission_id, decision).await,
    }
}
