//! Filesystem persistence for slice 01.
//!
//! Layout under <data_dir>/:
//!   missions/<id>/mission.json
//!   missions/<id>/state.json          (task runs)
//!   missions/<id>/attempts/<aid>.json
//!   missions/<id>/events/<aid>.jsonl  (raw runtime stream)
//!   missions/<id>/evidence.jsonl      (append-only evidence records)
//!   missions/<id>/workspace/          (agent working directory)
//!
//! This is deliberately a thin JSON store — NOT the final persistence
//! design (aucteris-architecture 08-STATE-AND-RECOVERY §3 stays open;
//! Restate is the orchestration candidate per ADR-0003). It exists to make
//! the slice's recovery semantics real.

use crate::domain::*;
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

pub struct Store {
    pub data_dir: PathBuf,
}

impl Store {
    pub fn new(data_dir: impl Into<PathBuf>) -> Result<Self> {
        let s = Self {
            data_dir: data_dir.into(),
        };
        fs::create_dir_all(s.missions_dir())?;
        Ok(s)
    }

    pub fn missions_dir(&self) -> PathBuf {
        self.data_dir.join("missions")
    }

    pub fn mission_dir(&self, id: &str) -> PathBuf {
        self.missions_dir().join(id)
    }

    // ----- mission -----

    pub fn create_mission(&self, spec: &MissionSpec) -> Result<Mission> {
        // Second-resolution timestamps collide on rapid successive
        // submissions — and a shared id means a shared Restate workflow
        // key, so the second mission would be silently deduplicated.
        let base = format!("m-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
        let mut id = base.clone();
        let mut n = 2;
        while self.missions_dir().join(&id).exists() {
            id = format!("{}-{}", base, n);
            n += 1;
        }
        let dir = self.mission_dir(&id);
        fs::create_dir_all(dir.join("attempts"))?;
        fs::create_dir_all(dir.join("events"))?;
        fs::create_dir_all(dir.join("workspace"))?;
        // Store absolute paths: runtimes resolve -C/cwd independently.
        let workspace = dir.join("workspace").canonicalize()?;

        let mission = Mission {
            id: id.clone(),
            objective: spec.objective.clone(),
            constraints: spec.constraints.clone(),
            tasks: spec.tasks.clone(),
            status: MissionStatus::Pending,
            created_at: chrono::Utc::now().to_rfc3339(),
            workspace,
            sandbox: spec.sandbox,
            allow_network: spec.allow_network,
            egress_filter: spec.egress_filter,
            memory: spec.memory,
            memory_namespace: spec.memory_namespace.clone(),
            sync_memory: spec.sync_memory,
            tool_approval: spec.tool_approval,
        };
        self.save_mission(&mission)?;
        self.save_state(
            &id,
            &mission
                .tasks
                .iter()
                .map(|t| TaskRun {
                    task_id: t.id.clone(),
                    status: TaskStatus::Pending,
                    attempts: vec![],
                    session_id: None,
                })
                .collect::<Vec<_>>(),
        )?;
        Ok(mission)
    }

    pub fn load_mission(&self, id: &str) -> Result<Mission> {
        let path = self.mission_dir(id).join("mission.json");
        let raw =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn save_mission(&self, m: &Mission) -> Result<()> {
        let path = self.mission_dir(&m.id).join("mission.json");
        fs::write(path, serde_json::to_string_pretty(m)?)?;
        Ok(())
    }

    pub fn list_missions(&self) -> Result<Vec<String>> {
        let mut ids = vec![];
        for e in fs::read_dir(self.missions_dir())? {
            let e = e?;
            if e.file_type()?.is_dir() {
                ids.push(e.file_name().to_string_lossy().to_string());
            }
        }
        ids.sort();
        Ok(ids)
    }

    // ----- task run state -----

    pub fn load_state(&self, mission_id: &str) -> Result<Vec<TaskRun>> {
        let path = self.mission_dir(mission_id).join("state.json");
        let raw = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn save_state(&self, mission_id: &str, runs: &[TaskRun]) -> Result<()> {
        let path = self.mission_dir(mission_id).join("state.json");
        fs::write(path, serde_json::to_string_pretty(runs)?)?;
        Ok(())
    }

    // ----- attempts -----

    /// Path of an attempt record — used for id-uniqueness checks.
    pub fn attempt_path(&self, mission_id: &str, attempt_id: &str) -> PathBuf {
        self.mission_dir(mission_id)
            .join("attempts")
            .join(format!("{}.json", attempt_id))
    }

    pub fn save_attempt(&self, mission_id: &str, a: &Attempt) -> Result<()> {
        let path = self
            .mission_dir(mission_id)
            .join("attempts")
            .join(format!("{}.json", a.id));
        fs::write(path, serde_json::to_string_pretty(a)?)?;
        Ok(())
    }

    pub fn load_all_attempts(&self, mission_id: &str) -> Result<Vec<Attempt>> {
        let mut out: Vec<Attempt> = vec![];
        let dir = self.mission_dir(mission_id).join("attempts");
        for e in fs::read_dir(&dir)? {
            let e = e?;
            // Skip non-attempt files (e.g. .pid sidecars written at spawn).
            if e.path().extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let raw = fs::read_to_string(e.path())?;
            out.push(serde_json::from_str(&raw)?);
        }
        out.sort_by(|a, b| a.started_at.cmp(&b.started_at));
        Ok(out)
    }

    pub fn events_path(&self, mission_id: &str, attempt_id: &str) -> PathBuf {
        self.mission_dir(mission_id)
            .join("events")
            .join(format!("{}.jsonl", attempt_id))
    }

    // ----- evidence (append-only) -----

    pub fn append_evidence(&self, mission_id: &str, ev: &Evidence) -> Result<()> {
        use std::io::Write;
        // evidence.jsonl is shared by parallel attempt threads —
        // serialize appends or JSONL lines interleave (observed:
        // two objects concatenated on one line).
        static EVIDENCE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = EVIDENCE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.mission_dir(mission_id).join("evidence.jsonl");
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        // Single write_all of one line: O_APPEND keeps it atomic.
        let mut line = serde_json::to_string(ev)?;
        line.push('\n');
        f.write_all(line.as_bytes())?;
        Ok(())
    }

    pub fn load_evidence(&self, mission_id: &str) -> Result<Vec<Evidence>> {
        let path = self.mission_dir(mission_id).join("evidence.jsonl");
        let raw = match fs::read_to_string(&path) {
            Ok(r) => r,
            Err(_) => return Ok(vec![]),
        };
        Ok(raw
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{MissionSpec, TaskSpec};

    fn spec(objective: &str) -> MissionSpec {
        MissionSpec {
            objective: objective.into(),
            constraints: vec![],
            sandbox: false,
            allow_network: true,
            egress_filter: false,
            memory: false,
            memory_namespace: None,
            sync_memory: false,
            tool_approval: false,
            tasks: vec![TaskSpec {
                id: "t".into(),
                title: "t".into(),
                prompt: "p".into(),
                verify_cmd: "true".into(),
                depends_on: vec![],
                runtime: "mock".into(),
                isolated: false,
            }],
        }
    }

    /// Regression: two missions created within the same second must NOT
    /// share an id — a shared id means a shared Restate workflow key and
    /// the second submission is silently deduplicated (observed in CI).
    #[test]
    fn create_mission_same_second_gets_unique_ids() {
        let dir = std::env::temp_dir().join(format!("aucteris-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = Store::new(&dir).unwrap();

        let m1 = store.create_mission(&spec("first")).unwrap();
        let m2 = store.create_mission(&spec("second")).unwrap();

        assert_ne!(m1.id, m2.id, "same-second missions collided");
        assert_eq!(m1.objective, "first");
        assert_eq!(m2.objective, "second");

        // Both missions must load back with their own spec — no clobbering.
        assert_eq!(store.load_mission(&m1.id).unwrap().objective, "first");
        assert_eq!(store.load_mission(&m2.id).unwrap().objective, "second");

        let _ = fs::remove_dir_all(&dir);
    }
}
