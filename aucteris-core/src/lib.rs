//! Reusable Aucteris agent execution engine.
//!
//! The library owns the mission lifecycle and runtime adapters. The CLI is
//! only one client of this API; desktop apps and other harnesses can embed it
//! directly and provide additional [`RuntimeAdapter`](runtime::RuntimeAdapter)s.

pub mod cgroup;
pub mod domain;
pub mod egress;
pub mod executor;
pub mod orchestrator;
pub mod report;
pub mod runtime;
pub mod sandbox;
pub mod store;

use anyhow::Result;
use std::path::PathBuf;

/// Resolve the default persistence directory used by durable orchestrators.
/// Embedded callers should generally pass an explicit path through
/// [`CoreConfig`] instead.
pub fn data_dir() -> PathBuf {
    std::env::var("AUCTERIS_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("aucteris-data"))
}

pub use domain::{Mission, MissionSpec};
pub use executor::ExecutionEvent;
pub use runtime::{RuntimeAdapter, RuntimeRegistry};

/// Configuration for an embedded core instance.
#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub data_dir: PathBuf,
}

impl CoreConfig {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }
}

/// Public application API shared by the CLI, desktop UI, and external harnesses.
pub struct AucterisCore {
    store: store::Store,
    runtimes: RuntimeRegistry,
}

impl AucterisCore {
    pub fn open(config: CoreConfig) -> Result<Self> {
        Self::open_with_registry(config, RuntimeRegistry::default())
    }

    pub fn open_with_registry(config: CoreConfig, runtimes: RuntimeRegistry) -> Result<Self> {
        Ok(Self {
            store: store::Store::new(config.data_dir)?,
            runtimes,
        })
    }

    /// Persist a mission and return its assigned id/workspace.
    pub fn submit(&self, spec: &MissionSpec) -> Result<Mission> {
        self.store.create_mission(spec)
    }

    /// Persist a mission whose agent should work directly in `workspace`.
    /// The mission's evidence and attempt records remain under the core data dir.
    pub fn submit_in_workspace(
        &self,
        spec: &MissionSpec,
        workspace: impl Into<PathBuf>,
    ) -> Result<Mission> {
        let mut mission = self.store.create_mission(spec)?;
        let workspace = workspace.into();
        std::fs::create_dir_all(&workspace)?;
        mission.workspace = workspace.canonicalize()?;
        self.store.save_mission(&mission)?;
        Ok(mission)
    }

    /// Execute a previously submitted mission synchronously.
    pub fn run(&self, mission_id: &str) -> Result<()> {
        executor::Executor::with_registry(&self.store, self.runtimes.clone())
            .run_mission(mission_id)
    }

    /// Execute a mission and synchronously forward normalized runtime events
    /// as they arrive. The callback can send them to a UI or IPC event channel.
    pub fn run_with_events(
        &self,
        mission_id: &str,
        on_event: impl Fn(ExecutionEvent) + Send + Sync + 'static,
    ) -> Result<()> {
        executor::Executor::with_event_sink(&self.store, self.runtimes.clone(), on_event)
            .run_mission(mission_id)
    }

    /// Request cancellation of the currently supervised runtime process tree.
    /// The core currently supervises one active run per process.
    pub fn cancel_active(&self) -> usize {
        runtime::cli::cancel_active()
    }

    /// Persist and execute a mission in one call.
    pub fn execute(&self, spec: &MissionSpec) -> Result<Mission> {
        let mission = self.submit(spec)?;
        self.run(&mission.id)?;
        self.mission(&mission.id)
    }

    /// Reconcile interrupted attempts, then continue a mission.
    pub fn resume(&self, mission_id: &str) -> Result<()> {
        self.run(mission_id)
    }

    pub fn mission(&self, mission_id: &str) -> Result<Mission> {
        self.store.load_mission(mission_id)
    }

    pub fn missions(&self) -> Result<Vec<String>> {
        self.store.list_missions()
    }

    pub fn report(&self, mission_id: &str) -> Result<String> {
        report::build_report(&self.store, mission_id)
    }

    pub fn store(&self) -> &store::Store {
        &self.store
    }

    pub fn runtimes(&self) -> &RuntimeRegistry {
        &self.runtimes
    }
}
