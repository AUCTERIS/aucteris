# Aucteris

Aucteris is a local-first Rust workspace for creating and running agent bots through installed CLI harnesses. The GPUI desktop app and mission CLI both embed the same reusable execution engine; there is no separate Aucteris API service.

## Run

```bash
cargo run -p aucteris-desktop
```

Create a bot, select an installed CLI (`claude`, `codex`, `opencode`, or `gemini`), set its instructions and working folder, then send a prompt. The engine persists mission evidence separately from the bot's working folder and streams normalized events back to the desktop UI. Bot profiles and chat history are stored under `$XDG_DATA_HOME/aucteris`, or `~/.local/share/aucteris`.

The standalone mission CLI is available as `aucteris`:

```bash
cargo run -p aucteris-cli -- run crates/core/examples/mission-slice01.yaml
```

Agent CLIs use their own local installation and authentication. Aucteris does not host a model API or require a separate backend service.

## Workspace

- `apps/desktop` — GPUI composition root, bot/chat application state, local persistence, and presentation.
- `apps/cli` — command-line entry point for mission files, reports, and durable orchestrators.
- `crates/core` — reusable mission engine, domain, runtime adapters, persistence, sandboxing, and orchestration.

See [ARCHITECTURE.md](ARCHITECTURE.md) for module responsibilities and dependency rules, [docs/architecture](docs/architecture/README.md) for the implemented vertical slice, and [docs/adr](docs/adr/README.md) for decision records.
