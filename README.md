# Aucteris

Aucteris is a native GPUI desktop app backed by an embedded Rust agent engine. The UI and core live in one Cargo workspace; the desktop app calls the core directly, with no separate Aucteris API service.

## Run

```bash
cargo run -p aucteris-ui
```

Create a bot with a name, role, working folder, instructions, and an installed CLI agent. The available agents are detected from `PATH`: Claude Code (`claude`), Codex (`codex`), OpenCode (`opencode`), and Gemini CLI (`gemini`). Enter a prompt in the bot chat to start a real run in that bot's folder. The core records the mission and streams normalized runtime events back to the chat. Stop terminates the active CLI process tree.

Bot profiles and chat history are stored locally under `$XDG_DATA_HOME/aucteris/desktop-state.json`, or `~/.local/share/aucteris/desktop-state.json` when `XDG_DATA_HOME` is not set. Core mission records use the same Aucteris data directory.

## Structure

- `aucteris-ui/` — GPUI desktop shell, bot setup/editing, local persistence, and chat presentation.
- `aucteris-core/` — embeddable mission engine, runtime adapters, execution lifecycle, and evidence store.

To build the desktop app, Cargo needs the pinned GPUI fork dependencies and the core's protobuf build dependency. No Aucteris server or model API integration is required; each selected CLI uses its own local installation and authentication.
