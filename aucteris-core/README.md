# aucteris-core

Reusable Rust engine for **controlled, reliable, and verifiable execution** of work performed by specialized agents — across runtimes, domains, and platforms.

Part of the [AUCTERIS](https://github.com/AUCTERIS) project. Architecture dossier: [aucteris-architecture](https://github.com/AUCTERIS/aucteris-architecture).

> Status: pre-alpha, Vertical Slice 01. See `VERTICAL-SLICE-01` criteria in the architecture repo before assuming anything is stable.

## Embedding the engine

`aucteris-core` now exposes both a Rust library and the `aucteris-core` CLI.
The CLI is a client of the same library API used by a desktop app or another
harness. Add the crate as a dependency (in the future unified repository this
will be a workspace dependency):

```toml
[dependencies]
aucteris-core = { path = "../aucteris-core" }
```

```rust,no_run
use aucteris_core::{AucterisCore, CoreConfig, MissionSpec};

let core = AucterisCore::open(CoreConfig::new("./aucteris-data"))?;
let mission: MissionSpec = serde_yaml::from_str(mission_yaml)?;
let created = core.submit(&mission)?;
core.run(&created.id)?;
let report = core.report(&created.id)?;
# Ok::<(), anyhow::Error>(())
```

The library also exports `RuntimeRegistry` and `RuntimeAdapter`. An embedding
harness can register a custom runtime adapter and pass the registry to
`AucterisCore::open_with_registry`; built-in Claude Code, Codex, OpenCode,
Gemini, and mock adapters remain registered by default. Custom adapters receive
the same `RunRequest` policy and return observed `RunOutcome` data, so the
executor retains ownership of persistence, verification, and mission status.
Use `run_with_events` to receive normalized runtime events during execution and
forward them into an application event channel without polling the evidence
files.

## What it does today (slice 01)

- Accepts a structured mission (YAML): objective + tasks with prompts and **independent verification commands**.
- Executes tasks through pluggable **runtime adapters** (`claude-code`, `codex`, `opencode`, `gemini`, `mock` test-double; registry in `src/runtime/`).
- Persists every attempt *before* spawning — a crash leaves an honest `Running`/`Unknown` record, never a silent guess.
- Runs verification **itself** (`verify_cmd`), outside the agent.
- Produces a report separating **agent assertions / runtime observations / independent verification / declared limitations / unknown outcomes**.

## Quick start

```bash
cargo build

# run a mission (requires `codex` or `claude` CLI on PATH + auth)
AUCTERIS_DATA_DIR=./aucteris-data ./target/debug/aucteris-core run examples/mission-slice01.yaml

# inspect / resume / report
./target/debug/aucteris-core list
./target/debug/aucteris-core resume <mission-id>
./target/debug/aucteris-core report <mission-id>
```

Data lives in `$AUCTERIS_DATA_DIR/missions/<id>/` (state JSON, attempt records, raw runtime event streams, evidence log, workspace, report.md, `durable-result.json` for durable runs).

## Durable mode (engine-neutral, ADR-0003)

Orchestration is engine-neutral behind `src/orchestrator/` (`AUCTERIS_ENGINE`, default `restate`; supported: `restate`, `local`, `temporal` — unknown values are an explicit error).

### Restate (default)

Requires a `restate-server` (admin `:9070`, ingress `:8080`) and the service endpoint:

```bash
restate-server --base-dir /tmp/restate-data &
./target/debug/aucteris-core serve &                       # endpoint :9080
curl -X POST localhost:9070/deployments \
  -H 'content-type: application/json' -d '{"uri":"http://localhost:9080"}'

./target/debug/aucteris-core run-durable examples/mission-slice01.yaml [--approval]
./target/debug/aucteris-core approve <mission-id> approved|rejected
```

The Restate workflow owns the task lifecycle durably; each attempt still runs through the supervised executor (sandbox, process-group + cgroup supervision, evidence). A service kill mid-attempt is retried after `reconcile_interrupted` marks the orphan `unknown` — recorded, never silent. `durable-result.json` is stamped with `workflow_version`.

`AUCTERIS_ENGINE=local` selects the in-process non-durable engine — same port, no server, no retry, no approval gate. It exists to prove the orchestration boundary is engine-neutral.

### Temporal (second durable engine)

Requires a Temporal frontend (`TEMPORAL_ADDRESS`, default `127.0.0.1:7233`) — e.g. `temporal server start-dev`. `serve` runs a **worker** (no HTTP endpoint; Temporal's model is worker-polls-task-queue, not service endpoint):

```bash
AUCTERIS_ENGINE=temporal ./target/debug/aucteris-core serve &   # worker on task queue aucteris-missions
./target/debug/aucteris-core run-durable examples/mission-slice01.yaml [--approval]
./target/debug/aucteris-core approve <mission-id> approved|rejected   # signal, not awakeable resolve
```

Same semantics measured end-to-end: approval gate parks the workflow until signaled; dependency-gated waves fan out attempt activities concurrently (verifies sequential); `kill -9` of the worker mid-attempt is detected via activity heartbeat timeout (~60s), the retried activity reconciles the orphan to `unknown` first, and a fresh attempt completes. `durable-result.json` is stamped `engine: "temporal"` + `workflow_version`.

## Architecture notes

- `src/domain.rs` — slice subset of the domain model (Mission/Task/Attempt/Evidence). Full model: architecture repo doc 02.
- `src/runtime/` — `RuntimeAdapter` trait + capability declarations (doc 04 §3). Per-runtime files are *specs* (argv + event parser) over a shared CLI engine (`cli.rs`). Raw events are evidence; a normalized `RuntimeEvent` view feeds logic.
- `src/sandbox.rs` — engine-level sandbox via bubblewrap (userns+mount/pid ns): system dirs ro, workspace rw, adapter-declared mounts only — the rest of `$HOME` is invisible. `sandbox: false` in the mission YAML disables it; when bwrap is missing the engine degrades and records it.
- `src/cgroup.rs` — per-attempt cgroup v2 under the delegated user slice: `memory.max`/`cpu.max`/`pids.max` bound the whole subtree (descendants keep membership across `setsid`), and `cgroup.kill` is the authoritative cleanup — used on attempt end and by orphan reconciliation after engine death. No delegated root → degrades to process-group-only (recorded).
- `src/executor.rs` — dependency-gated parallel waves (max 4 concurrent attempts, each with own sandbox/cgroup/evidence), recovery reconciliation (`Running` → `Unknown` on restart, never silent retry), process-group + `/proc` descendant supervision (`kill_process_tree` reaches `setsid` escapees).
- `src/report.rs` — evidence-separated reporting (doc 09 §4).
- `src/orchestrator/` — engine-neutral orchestration port; `restate.rs` is the Restate adapter (workflow + activities + awakeables). Other engines implement the same port.

## Honest limitations (slice 01)

- Agent tool calls are **not mediated**; isolation is layered: engine bwrap sandbox (fs confinement) + whatever each runtime declares (`workspace-write` for codex, advisory flags for claude).
- cgroups require a delegated cgroup v2 root (systemd user slice); elsewhere attempts run without resource limits (recorded via `cgroup_limited=false`).
- Network egress has two engine controls: `allow_network: false` → `bwrap --unshare-net` (full deny, sandbox only); `egress_filter: true` → per-attempt local CONNECT proxy with a domain allowlist declared by each adapter (`RuntimeAdapter::egress_domains`), injected via `*_PROXY` env. **The proxy is mediation, not enforcement** — runtimes ignoring proxy env vars bypass it; hard enforcement needs netns+veth (root). Allow/deny counts are recorded in evidence (`egress_allowed`/`egress_denied`).
- Sandbox is Linux/bubblewrap-only today; other platforms degrade to runtime-only isolation (recorded).
- Persistence is a JSON file store for evidence/state; durable orchestration runs on Restate (ADR-0003 accepted) via `src/orchestrator/` — the local `run` path remains the non-durable fallback.
- Parallel tasks share the mission workspace by default (like `make -j`); `isolated: true` on a task gives it `<mission>/iso/<task-id>/` as its sandbox root — it cannot see sibling files (one-directional isolation: shared tasks still see `iso/`). Max 4 concurrent attempts per wave. The Restate durable workflow also runs dependency-gated waves: attempt activities fan out concurrently (max 4); verify activities run sequentially after each wave's join — concurrent verifies stalled on restate-sdk 0.12.1 in testing (measured), sequential verifies are cheap.
- `memory: true` on the mission spec gives every task's agent persistent memory tools: the engine writes an MCP `mcpServers` config (`<task-ws>/.aucteris/mcp.json`) pointing at the `aucteris-memory` binary (`AUCTERIS_MEMORY_BIN` or PATH) with `AUCTERIS_MEMORY_DIR=<mission>/memory` — a per-mission SQLite store shared by all tasks, isolated from the user's global store. Inside the engine sandbox the memory dir is bound rw and the binary's parent dir ro. Each adapter materializes its native mechanism (`RuntimeAdapter::supports_mcp`): claude-code `--mcp-config`, codex `-c mcp_servers.*` overrides, gemini `.gemini/settings.json`, opencode `opencode.json`; runtimes without support record an honest limitation. Memory is **unmediated** — the agent can write/delete its own memories; recorded as a limitation. `memory_namespace` sets the namespace hint (default: mission id — set it to match a sync `[[targets]]` entry); `sync_memory: true` additionally runs `aucteris-memory sync push` on `<mission>/memory` after the mission ends (any final status), pushing memories to Memexa per the MEMORY-SYNC-V2 protocol. Sync needs `AUCTERIS_MEMORY_SYNC_CONFIG` (path to sync.toml) + the aucteris-memory binary; every outcome is recorded as system-observation evidence and never fails the mission. Local-engine path only for now — durable engines don't auto-sync yet.

## Roadmap pointer

Next slices: hard egress enforcement (netns+veth, root), workflow versioning/migration for Restate, multi-agent teams, professional profiles.
