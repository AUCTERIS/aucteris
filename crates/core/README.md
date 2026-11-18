# aucteris-core

Reusable Rust mission engine for the Aucteris desktop application, CLI, and other harnesses. The crate owns mission lifecycle, runtime registration, execution evidence, verification, and reporting.

From the workspace root, the desktop and CLI clients can be started with:

```bash
cargo run -p aucteris-desktop
cargo run -p aucteris-cli -- run crates/core/examples/mission-slice01.yaml
```

An embedding application can use the public API:

```rust,no_run
use aucteris_core::{AucterisCore, CoreConfig, MissionSpec};

let core = AucterisCore::open(CoreConfig::new("./aucteris-data"))?;
let spec: MissionSpec = serde_yaml::from_str(mission_yaml)?;
let mission = core.submit_in_workspace(&spec, "./workspace")?;
core.run_with_events(&mission.id, |event| println!("{event:?}"))?;
# Ok::<(), anyhow::Error>(())
```

Runtime adapters implement `RuntimeAdapter` and register through `RuntimeRegistry`; the engine retains ownership of mission status, persistence, verification, cancellation, and reporting.
