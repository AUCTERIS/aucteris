//! aucteris — CLI for Vertical Slice 01.
//!
//!   aucteris run <mission.yaml>     submit + execute a mission
//!   aucteris resume <mission-id>    re-run non-completed tasks (new attempts)
//!   aucteris report <mission-id>    print/write the evidence-separated report
//!   aucteris list                   list missions in the data dir
//!
//! Data dir: $AUCTERIS_DATA_DIR or ./aucteris-data

use anyhow::{Context, Result};
use aucteris_core::{
    domain::MissionSpec, executor::Executor, infrastructure, orchestrator, report, store::Store,
};

fn data_dir() -> std::path::PathBuf {
    std::env::var("AUCTERIS_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("aucteris-data"))
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Ctrl+C: kill the supervised agent's whole process group, then let
    // the executor unwind honestly (attempt outcome stays uncertain).
    ctrlc::set_handler(|| {
        use std::sync::atomic::Ordering;
        infrastructure::process::CANCEL_REQUESTED.store(true, Ordering::SeqCst);
        for pgid in infrastructure::process::active_pgids() {
            infrastructure::process::kill_process_tree(pgid, libc::SIGKILL);
        }
    })
    .expect("installing Ctrl+C handler");

    let args: Vec<String> = std::env::args().collect();
    let store = Store::new(data_dir())?;
    let exec = Executor::new(&store);

    match args.get(1).map(|s| s.as_str()) {
        Some("run") => {
            let path = args.get(2).context("usage: aucteris run <mission.yaml>")?;
            let raw = std::fs::read_to_string(path)?;
            let spec: MissionSpec =
                serde_yaml::from_str(&raw).with_context(|| format!("parsing {}", path))?;
            let mission = store.create_mission(&spec)?;
            println!(
                "mission {} created (workspace: {})",
                mission.id,
                mission.workspace.display()
            );
            exec.run_mission(&mission.id)?;
            let report_path = report::write_report(&store, &mission.id)?;
            println!("report: {}", report_path.display());
            print!("{}", report::build_report(&store, &mission.id)?);
        }
        Some("resume") => {
            let id = args.get(2).context("usage: aucteris resume <mission-id>")?;
            exec.reconcile_interrupted(id)?;
            exec.run_mission(id)?;
            let report_path = report::write_report(&store, id)?;
            println!("report: {}", report_path.display());
        }
        Some("report") => {
            let id = args.get(2).context("usage: aucteris report <mission-id>")?;
            let path = report::write_report(&store, id)?;
            println!("{}", path.display());
            print!("{}", report::build_report(&store, id)?);
        }
        Some("list") => {
            for id in store.list_missions()? {
                let m = store.load_mission(&id)?;
                println!("{}  {:?}  {}", id, m.status, m.objective);
            }
        }
        Some("serve") => {
            // Durable orchestration endpoint (ADR-0003). Engine selected
            // via AUCTERIS_ENGINE (default: restate). Requires the
            // engine's backing service; see README "Durable mode".
            let engine = orchestrator::EngineKind::from_env()?;
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(orchestrator::serve(engine))?;
        }
        Some("run-durable") => {
            let path = args
                .get(2)
                .context("usage: aucteris run-durable <mission.yaml> [--approval]")?;
            let raw = std::fs::read_to_string(path)?;
            let spec: MissionSpec =
                serde_yaml::from_str(&raw).with_context(|| format!("parsing {}", path))?;
            let require_approval = args.iter().any(|a| a == "--approval");
            let mission = store.create_mission(&spec)?;
            println!(
                "mission {} persisted (workspace: {})",
                mission.id,
                mission.workspace.display()
            );
            let engine = orchestrator::EngineKind::from_env()?;
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            let out = rt.block_on(orchestrator::submit(engine, &mission.id, require_approval))?;
            println!("{}", out);
        }
        Some("approve") => {
            let id = args
                .get(2)
                .context("usage: aucteris approve <mission-id> <decision>")?;
            let decision = args
                .get(3)
                .context("decision required (approved|rejected)")?;
            let engine = orchestrator::EngineKind::from_env()?;
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            println!(
                "{}",
                rt.block_on(orchestrator::approve(engine, id, decision))?
            );
        }
        _ => {
            eprintln!("usage: aucteris <run|resume|report|list|serve|run-durable|approve> ...");
            std::process::exit(2);
        }
    }
    Ok(())
}
