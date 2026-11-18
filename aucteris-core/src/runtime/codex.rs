//! OpenAI Codex CLI adapter — spec only.
//!
//! Headless mode: `codex exec --json --sandbox workspace-write`.
//! Events are JSONL on stdout: thread/turn/item lifecycle.
//! `workspace-write` sandbox is OS-enforced on Linux (Landlock/seccomp)
//! — a stronger isolation story than advisory CLI flags.

use crate::runtime::cli::{run_cli, CliSpec};
use crate::runtime::{
    AdapterCapabilities, IsolationLevel, RunOutcome, RunRequest, RuntimeAdapter, RuntimeEvent,
};
use anyhow::Result;

fn toml_str(s: &str) -> String {
    // JSON string escaping is a valid TOML basic-string escape set.
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

fn build_args(req: &RunRequest) -> Vec<String> {
    let mut args = vec![
        "exec".into(),
        "--skip-git-repo-check".into(),
        "--sandbox".into(),
        "workspace-write".into(),
        "--ephemeral".into(),
        "--json".into(),
        "-C".into(),
        req.workspace.display().to_string(),
    ];
    // Engine-provided MCP server via -c config overrides (never writes
    // to the user's ~/.codex/config.toml). mcp_servers.<name>.* keys
    // match the config.toml schema; env is an inline TOML table.
    if let Some(plan) = &req.mcp {
        let key = format!("mcp_servers.{}", plan.name);
        args.push("-c".into());
        args.push(format!("{key}.command={}", toml_str(&plan.command)));
        args.push("-c".into());
        args.push(format!("{key}.args=[]"));
        let env_tbl = plan
            .env
            .iter()
            .map(|(k, v)| format!("{}={}", toml_str(k), toml_str(v)))
            .collect::<Vec<_>>()
            .join(",");
        args.push("-c".into());
        args.push(format!("{key}.env={{{env_tbl}}}"));
    }
    args.push(req.prompt.clone());
    args
}

/// Parse one codex exec JSONL event.
fn parse_line(line: &str) -> Vec<RuntimeEvent> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return vec![];
    };
    match v.get("type").and_then(|t| t.as_str()) {
        Some("thread.started") => vec![RuntimeEvent::SessionStarted { model: None }],
        Some("item.completed") | Some("item.started") => {
            let Some(item) = v.get("item") else {
                return vec![];
            };
            match item.get("type").and_then(|t| t.as_str()) {
                // Every completed agent_message becomes a FinalResult
                // candidate; run_cli keeps the LAST one = the agent's
                // closing claim. Progress messages are overwritten.
                Some("agent_message") if v["type"] == "item.completed" => {
                    vec![RuntimeEvent::FinalResult {
                        text: item
                            .get("text")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string(),
                    }]
                }
                Some("command_execution")
                | Some("file_change")
                | Some("mcp_tool_call")
                | Some("web_search") => {
                    if v["type"] == "item.completed" {
                        vec![RuntimeEvent::ToolResult {
                            ok: item.get("status").and_then(|s| s.as_str()) != Some("failed"),
                        }]
                    } else {
                        vec![RuntimeEvent::ToolCall {
                            name: item
                                .get("type")
                                .and_then(|t| t.as_str())
                                .unwrap_or("tool")
                                .to_string(),
                        }]
                    }
                }
                _ => vec![],
            }
        }
        Some("turn.completed") => {
            let u = v.get("usage");
            vec![RuntimeEvent::Usage {
                input_tokens: u
                    .and_then(|u| u.get("input_tokens"))
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0),
                output_tokens: u
                    .and_then(|u| u.get("output_tokens"))
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0),
            }]
        }
        Some("turn.failed") | Some("error") => vec![RuntimeEvent::Error {
            message: v.to_string(),
        }],
        _ => vec![],
    }
}

const SPEC: CliSpec = CliSpec {
    program: "codex",
    build_args,
    parse_line,
};

pub struct CodexAdapter;

impl RuntimeAdapter for CodexAdapter {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            mediated_tools: false,
            cancellable: true,
            streaming_events: true,
            // Measured 2026-11-12, codex-cli 0.166.0, workspace-write:
            // writes to $HOME blocked (read-only fs), /tmp writable,
            // network egress blocked (DNS + direct IP), reads outside
            // the workspace NOT confined (~/.aws, /etc readable).
            workspace_isolation: IsolationLevel::RuntimeAssisted,
            resource_accounting: true,
            limitations: vec![
                "runtime sandbox alone: writes confined to workspace+/tmp; reads NOT confined — engine bwrap sandbox adds fs-ro confinement when available".into(),
                "network egress blocked under workspace-write for tool calls (measured codex-cli 0.166.0); agent API egress stays open".into(),
                "user-site packages (~/.local/lib/python*) are invisible inside the engine sandbox".into(),
                "tool calls are not mediated by Aucteris".into(),
                "approval policy 'never' assumed via exec mode; verify per-version".into(),
            ],
        }
    }

    fn run(&self, req: RunRequest) -> Result<RunOutcome> {
        run_cli(&SPEC, &req)
    }

    fn sandbox_mounts(&self) -> crate::sandbox::SandboxMounts {
        let home = dirs_home();
        let codex = home.join(".codex");
        crate::sandbox::SandboxMounts {
            // Binary + packages + auth live under ~/.codex. The runtime
            // must WRITE there (logs sqlite, sessions, caches) so the
            // dir is rw — but credentials/config/hooks stay ro on top.
            rw_dirs: vec![codex.clone()],
            ro_dirs: vec![],
            ro_files: vec![
                codex.join("auth.json"),
                codex.join("config.toml"),
                codex.join("hooks.json"),
                codex.join("AGENTS.md"),
            ],
            // Shell snapshots cache the HOST shell env (aliases/functions);
            // replaying them inside the sandbox broke tool calls (rtk).
            // Mask them: fresh tmpfs per run, no host↔sandbox leakage.
            tmpfs_over: vec![codex.join("shell_snapshots")],
        }
    }

    fn egress_domains(&self) -> Vec<String> {
        vec![
            "chatgpt.com".into(), // codex backend API
            "api.openai.com".into(),
            "auth.openai.com".into(), // token refresh
        ]
    }

    fn supports_mcp(&self) -> bool {
        true // -c mcp_servers.<name>.* overrides, no config.toml writes
    }
}

fn dirs_home() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
}
