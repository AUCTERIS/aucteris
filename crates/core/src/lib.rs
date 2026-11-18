//! Reusable Aucteris execution engine.
//!
//! The crate is organized as a layered engine:
//! - `domain` contains mission and task state.
//! - `application` coordinates mission execution and reporting.
//! - `application::ports::runtime` defines the harness contract.
//! - `adapters::runtime` implements the built-in agent CLI harnesses.
//! - `infrastructure` owns storage, sandboxing, process control, and egress.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod infrastructure;

// Stable public module paths for clients that used the first embeddable API.
pub use adapters::runtime;
pub use application::{executor, orchestrator, report};
pub use infrastructure::{cgroup, egress, sandbox, store};

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

impl Default for RuntimeRegistry {
    fn default() -> Self {
        let mut registry = RuntimeRegistry::new();
        adapters::runtime::register_builtins(&mut registry);
        registry
    }
}

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

/// Public application API shared by the CLI, desktop app, and other harnesses.
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

    /// Execute a previously submitted mission and forward normalized runtime events.
    pub fn run_with_events(
        &self,
        mission_id: &str,
        on_event: impl Fn(ExecutionEvent) + Send + Sync + 'static,
    ) -> Result<()> {
        executor::Executor::with_event_sink(&self.store, self.runtimes.clone(), on_event)
            .run_mission(mission_id)
    }

    /// Request cancellation of the currently supervised runtime process tree.
    pub fn cancel_active(&self) -> usize {
        infrastructure::process::cancel_active()
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
