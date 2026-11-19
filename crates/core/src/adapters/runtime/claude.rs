//! Claude Code adapter — spec only.
//!
//! Headless mode: `claude -p <prompt> --output-format stream-json --verbose`.
//! Events are JSONL on stdout. Final claim arrives as a `result` event.
//! Permission flags bound the tool surface (best-effort — NOT mediation).

use crate::runtime::cli::{CliSpec, run_cli};
use crate::runtime::{
    AdapterCapabilities, IsolationLevel, RunOutcome, RunRequest, RuntimeAdapter, RuntimeEvent,
};
use anyhow::Result;

/// Tools allowed for slice missions. WebFetch/WebSearch denied for the
/// no-egress posture; Bash remains a documented gap (threat model A1).
const ALLOWED_TOOLS: &str = "Read,Write,Edit,Bash,Grep,Glob";
const DISALLOWED_TOOLS: &str = "WebFetch,WebSearch";
// MCP tools are named mcp__<server>__<tool>; the allowlist entry
// mcp__<server> covers every tool of that server. The engine names the
// injected server, so the entry is built dynamically in build_args.
fn build_args(req: &RunRequest) -> Vec<String> {
    // Engine-provided MCP servers (e.g. aucteris-memory). Claude Code
    // accepts the standard {"mcpServers": {...}} JSON file directly.
    let allowed = if let Some(plan) = &req.mcp {
        format!("{ALLOWED_TOOLS},mcp__{}", plan.name)
    } else {
        ALLOWED_TOOLS.to_string()
    };
    let mut args = vec![
        "-p".into(),
        req.prompt.clone(),
        "--output-format".into(),
        "stream-json".into(),
        "--verbose".into(),
        "--max-turns".into(),
        req.max_turns.to_string(),
        "--allowedTools".into(),
        allowed,
        "--disallowedTools".into(),
        DISALLOWED_TOOLS.into(),
    ];
    if let Some(plan) = &req.mcp {
        args.push("--mcp-config".into());
        args.push(plan.config_path.display().to_string());
    }
    args
}

/// Parse one stream-json line into normalized events.
fn parse_line(line: &str) -> Vec<RuntimeEvent> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return vec![];
    };
    match v.get("type").and_then(|t| t.as_str()) {
        Some("system") if v.get("subtype").and_then(|s| s.as_str()) == Some("init") => {
            vec![RuntimeEvent::SessionStarted {
                model: v
                    .pointer("/model")
                    .and_then(|m| m.as_str())
                    .map(String::from),
            }]
        }
        Some("assistant") => {
            let mut out = vec![];
            if let Some(content) = v.pointer("/message/content").and_then(|c| c.as_array()) {
                for block in content {
                    match block.get("type").and_then(|t| t.as_str()) {
                        Some("tool_use") => out.push(RuntimeEvent::ToolCall {
                            name: block
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("unknown")
                                .to_string(),
                        }),
                        Some("text") => {
                            if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                                out.push(RuntimeEvent::AssistantMessage {
                                    text: t.to_string(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
            out
        }
        Some("result") => {
            let text = v
                .get("result")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();
            let usage = v.pointer("/usage");
            let mut out = vec![RuntimeEvent::FinalResult { text }];
            if let Some(u) = usage {
                out.push(RuntimeEvent::Usage {
                    input_tokens: u.get("input_tokens").and_then(|t| t.as_u64()).unwrap_or(0),
                    output_tokens: u.get("output_tokens").and_then(|t| t.as_u64()).unwrap_or(0),
                });
            }
            out
        }
        _ => vec![],
    }
}

const SPEC: CliSpec = CliSpec {
    program: "claude",
    build_args,
    parse_line,
};

pub struct ClaudeAdapter;

impl RuntimeAdapter for ClaudeAdapter {
    fn name(&self) -> &'static str {
        "claude-code"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            mediated_tools: false, // CLI flags are advisory, not interception
            cancellable: true,     // SIGTERM to process; descendants best-effort
            streaming_events: true,
            workspace_isolation: IsolationLevel::RuntimeAssisted,
            resource_accounting: true, // usage in result event
            limitations: vec![
                "tool calls are not mediated by Aucteris; allowedTools/disallowedTools are CLI-level best effort".into(),
                "Bash can still reach the network despite WebFetch/WebSearch denial".into(),
                "no OS-level sandbox; confinement relies on workspace dir + prompt rules".into(),
            ],
        }
    }

    fn run(&self, req: RunRequest) -> Result<RunOutcome> {
        run_cli(&SPEC, &req)
    }

    fn sandbox_mounts(&self) -> crate::sandbox::SandboxMounts {
        let home = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        crate::sandbox::SandboxMounts {
            // Versions dir holds the versioned binary; ~/.claude holds
            // config/credentials — read-only suffices for claude.
            ro_dirs: vec![home.join(".local/share/claude"), home.join(".claude")],
            ..Default::default()
        }
    }

    fn egress_domains(&self) -> Vec<String> {
        vec![
            "api.anthropic.com".into(),
            "statsig.anthropic.com".into(), // telemetry
        ]
    }

    fn supports_mcp(&self) -> bool {
        true // --mcp-config accepts the standard mcpServers JSON
    }
}
