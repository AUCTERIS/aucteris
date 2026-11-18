//! Gemini CLI adapter — spec only.
//!
//! Headless mode: `gemini -p "<prompt>" --output-format stream-json`.
//! `--skip-trust` is REQUIRED: headless runs refuse untrusted workspaces
//! (exit 55) — measured on gemini-cli 0.63.0.
//!
//! Auth note: Google retired the individual Code Assist tier
//! (IneligibleTierError) — the adapter is wired and parse-ready but
//! live runs depend on a working credential (API key / paid tier /
//! migrated Antigravity account). Declared as a limitation.

use crate::runtime::cli::{CliSpec, run_cli};
use crate::runtime::{
    AdapterCapabilities, IsolationLevel, RunOutcome, RunRequest, RuntimeAdapter, RuntimeEvent,
};
use anyhow::Result;

fn build_args(req: &RunRequest) -> Vec<String> {
    vec![
        "-p".into(),
        req.prompt.clone(),
        "--output-format".into(),
        "stream-json".into(),
        "--skip-trust".into(),
        // Headless non-interactive: never block on an approval prompt.
        "--approval-mode".into(),
        "yolo".into(),
    ]
}

/// Parse one gemini stream-json event. The format is best-effort:
/// message content can be a string or content parts; tool events carry
/// `tool_name`/`name`. Unknown shapes degrade to no events — the raw
/// JSONL is always preserved as evidence regardless.
fn parse_line(line: &str) -> Vec<RuntimeEvent> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return vec![];
    };
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match ty {
        "init" => vec![RuntimeEvent::SessionStarted {
            model: v.get("model").and_then(|m| m.as_str()).map(str::to_string),
        }],
        "message" => {
            if v.get("role").and_then(|r| r.as_str()) != Some("assistant") {
                return vec![];
            }
            let text = match v.get("content") {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Array(parts)) => parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join(""),
                _ => String::new(),
            };
            if text.is_empty() {
                vec![]
            } else {
                vec![RuntimeEvent::FinalResult { text }]
            }
        }
        "tool_use" | "tool_call" => vec![RuntimeEvent::ToolCall {
            name: v
                .get("tool_name")
                .or_else(|| v.get("name"))
                .or_else(|| v.pointer("/tool/name"))
                .and_then(|t| t.as_str())
                .unwrap_or("tool")
                .to_string(),
        }],
        "result" => {
            let mut evs = vec![];
            if let Some(resp) = v.get("response").and_then(|r| r.as_str()) {
                evs.push(RuntimeEvent::FinalResult {
                    text: resp.to_string(),
                });
            }
            if let Some(stats) = v.get("stats") {
                evs.push(RuntimeEvent::Usage {
                    input_tokens: stats
                        .pointer("/tokens/input")
                        .or_else(|| stats.get("input_tokens"))
                        .and_then(|n| n.as_u64())
                        .unwrap_or(0),
                    output_tokens: stats
                        .pointer("/tokens/output")
                        .or_else(|| stats.get("output_tokens"))
                        .and_then(|n| n.as_u64())
                        .unwrap_or(0),
                });
            }
            evs
        }
        "error" => vec![RuntimeEvent::Error {
            message: v.to_string(),
        }],
        _ => vec![],
    }
}

const SPEC: CliSpec = CliSpec {
    program: "gemini",
    build_args,
    parse_line,
};

pub struct GeminiAdapter;

impl RuntimeAdapter for GeminiAdapter {
    fn name(&self) -> &'static str {
        "gemini"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            mediated_tools: false,
            cancellable: true,
            streaming_events: true,
            // gemini has --sandbox but its enforcement is unmeasured;
            // the engine bwrap sandbox is the primary fs boundary.
            workspace_isolation: IsolationLevel::ExternalOnly,
            resource_accounting: true, // stats in result events
            limitations: vec![
                "Google retired the individual Code Assist tier (IneligibleTierError); requires a working credential".into(),
                "stream-json parser is best-effort and unverified against a live authenticated run".into(),
                "--approval-mode yolo auto-approves all tools; no runtime sandbox measured".into(),
            ],
        }
    }

    fn run(&self, req: RunRequest) -> Result<RunOutcome> {
        if let Some(plan) = &req.mcp {
            write_project_settings(&req.workspace, plan)?;
        }
        run_cli(&SPEC, &req)
    }

    fn sandbox_mounts(&self) -> crate::sandbox::SandboxMounts {
        let home = dirs_home();
        let gemini = home.join(".gemini");
        crate::sandbox::SandboxMounts {
            // Config + OAuth creds live under ~/.gemini; the CLI writes
            // session/cache state there, so rw with sensitive files ro.
            rw_dirs: vec![gemini.clone()],
            ro_dirs: vec![],
            ro_files: vec![
                gemini.join("oauth_creds.json"),
                gemini.join("settings.json"),
            ],
            tmpfs_over: vec![],
        }
    }

    fn egress_domains(&self) -> Vec<String> {
        vec![
            "generativelanguage.googleapis.com".into(),
            "oauth2.googleapis.com".into(),
            "accounts.google.com".into(),
        ]
    }

    fn supports_mcp(&self) -> bool {
        // Gemini CLI auto-loads .gemini/settings.json from the cwd —
        // the engine writes the project-level file per attempt.
        true
    }
}

/// Merge the engine's MCP server into `<workspace>/.gemini/settings.json`
/// (project settings are discovered from cwd). Existing keys are
/// preserved; only `mcpServers.<name>` is set. IO failure propagates —
/// a silently missing server is worse than a visible attempt failure.
fn write_project_settings(
    workspace: &std::path::Path,
    plan: &crate::runtime::McpPlan,
) -> Result<()> {
    let canonical: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&plan.config_path)?)?;
    let server = canonical
        .pointer(&format!("/mcpServers/{}", plan.name))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("mcp config missing server {}", plan.name))?;

    let dir = workspace.join(".gemini");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("settings.json");
    let mut settings: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if !settings.is_object() {
        settings = serde_json::json!({});
    }
    settings
        .as_object_mut()
        .expect("object")
        .entry("mcpServers".to_string())
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .expect("mcpServers object")
        .insert(plan.name.clone(), server);
    std::fs::write(&path, serde_json::to_string_pretty(&settings)?)?;
    Ok(())
}

fn dirs_home() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
}
