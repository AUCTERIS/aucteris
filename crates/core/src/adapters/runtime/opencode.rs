//! OpenCode adapter — spec only.
//!
//! Headless mode: `opencode run --format json --auto "<prompt>"`.
//! Events are JSONL on stdout: step_start / text / tool_* / step_finish.
//! Provider+model come from the user's opencode config (or the
//! OPENCODE_MODEL env override) — quality is provider-dependent and
//! declared as a limitation, not hidden.

use crate::runtime::cli::{CliSpec, run_cli};
use crate::runtime::{
    AdapterCapabilities, IsolationLevel, RunOutcome, RunRequest, RuntimeAdapter, RuntimeEvent,
};
use anyhow::Result;

fn build_args(req: &RunRequest) -> Vec<String> {
    let mut args = vec![
        "run".into(),
        "--format".into(),
        "json".into(),
        "--auto".into(), // auto-approve permissions not explicitly denied
    ];
    // Optional model override: OPENCODE_MODEL=provider/model#variant.
    if let Ok(m) = std::env::var("OPENCODE_MODEL")
        && !m.is_empty()
    {
        args.push("--model".into());
        args.push(m);
    }
    args.push(req.prompt.clone());
    args
}

/// Parse one opencode JSONL event.
fn parse_line(line: &str) -> Vec<RuntimeEvent> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return vec![];
    };
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match ty {
        "step_start" => vec![RuntimeEvent::SessionStarted { model: None }],
        "text" => {
            // Assistant text; run_cli keeps the LAST FinalResult as the
            // closing claim — progress text is overwritten like codex.
            let text = v
                .pointer("/part/text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            if text.is_empty() {
                vec![]
            } else {
                vec![RuntimeEvent::FinalResult { text }]
            }
        }
        "tool_use" | "tool_call" => vec![RuntimeEvent::ToolCall {
            name: v
                .pointer("/part/tool")
                .or_else(|| v.pointer("/part/name"))
                .and_then(|t| t.as_str())
                .unwrap_or("tool")
                .to_string(),
        }],
        "tool_result" => vec![RuntimeEvent::ToolResult {
            ok: v.pointer("/part/state/status").and_then(|s| s.as_str()) != Some("error"),
        }],
        "step_finish" => {
            let t = v.pointer("/part/tokens");
            vec![RuntimeEvent::Usage {
                input_tokens: t
                    .and_then(|t| t.get("input"))
                    .and_then(|n| n.as_u64())
                    .unwrap_or(0),
                output_tokens: t
                    .and_then(|t| t.get("output"))
                    .and_then(|n| n.as_u64())
                    .unwrap_or(0),
            }]
        }
        "error" => vec![RuntimeEvent::Error {
            message: v.to_string(),
        }],
        _ => vec![],
    }
}

const SPEC: CliSpec = CliSpec {
    program: "opencode",
    build_args,
    parse_line,
};

pub struct OpenCodeAdapter;

impl RuntimeAdapter for OpenCodeAdapter {
    fn name(&self) -> &'static str {
        "opencode"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            mediated_tools: false,
            cancellable: true,
            streaming_events: true,
            // No runtime sandbox of its own measured yet; the engine
            // bwrap sandbox is the primary fs boundary.
            workspace_isolation: IsolationLevel::ExternalOnly,
            resource_accounting: true, // tokens in step_finish
            limitations: vec![
                "no runtime-level sandbox measured; relies on engine bwrap sandbox".into(),
                "provider/model resolved by user opencode config — quality and quota are provider-dependent".into(),
                "tool calls are not mediated by Aucteris".into(),
            ],
        }
    }

    fn run(&self, req: RunRequest) -> Result<RunOutcome> {
        if let Some(plan) = &req.mcp {
            write_project_config(&req.workspace, plan)?;
        }
        run_cli(&SPEC, &req)
    }

    fn sandbox_mounts(&self) -> crate::sandbox::SandboxMounts {
        let home = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        let data = home.join(".local/share/opencode");
        crate::sandbox::SandboxMounts {
            // Binary + config ro; session DB/logs rw (runtime state),
            // with auth file protected.
            ro_dirs: vec![home.join(".opencode"), home.join(".config/opencode")],
            rw_dirs: vec![data.clone()],
            ro_files: vec![data.join("auth.json")],
            tmpfs_over: vec![],
        }
    }

    fn egress_domains(&self) -> Vec<String> {
        // Provider is user-configured (opencode.json / OPENCODE_MODEL) —
        // the engine cannot know which API hosts are needed. Declaring
        // an empty list means filtering denies everything through the
        // proxy; operators needing opencode+filter must extend this.
        vec![]
    }

    fn supports_mcp(&self) -> bool {
        // opencode auto-loads opencode.json from the project root.
        true
    }
}

/// Merge the engine's MCP server into `<workspace>/opencode.json`
/// (`"mcp"` section, local stdio server). Existing config keys are
/// preserved; only `mcp.<name>` is set.
fn write_project_config(workspace: &std::path::Path, plan: &crate::runtime::McpPlan) -> Result<()> {
    let path = workspace.join("opencode.json");
    let mut config: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if !config.is_object() {
        config = serde_json::json!({});
    }
    let env: serde_json::Map<String, serde_json::Value> = plan
        .env
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();
    let server = serde_json::json!({
        "type": "local",
        "command": [plan.command],
        "environment": env,
        "enabled": true,
    });
    config
        .as_object_mut()
        .expect("object")
        .entry("mcp".to_string())
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .expect("mcp object")
        .insert(plan.name.clone(), server);
    std::fs::write(&path, serde_json::to_string_pretty(&config)?)?;
    Ok(())
}
