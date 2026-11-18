# Aucteris Desktop

The Rust GPUI desktop application for Aucteris. The app embeds `aucteris-core` directly and stores bot profiles and chat history locally.

From the workspace root, run:

```bash
cargo run -p aucteris-ui
```

Create a bot, select an installed agent CLI, set its working folder and instructions, then send a prompt in its chat. Supported CLIs are Claude Code, Codex CLI, OpenCode, and Gemini CLI when found on `PATH`.
