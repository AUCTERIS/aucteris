//! Devin CLI adapter — spec only.
//!
//! Headless mode: `devin -p --permission-mode dangerous
//! --respect-workspace-trust false --process-model in-process -- <prompt>`.
//!
//! `-p` prints the agent's response as plain text on stdout — there is
//! no structured event stream, so each stdout line becomes an
//! AssistantMessage and no tool-call/usage detail is available.
//!
//! `--permission-mode dangerous` auto-approves every tool: the other
//! modes prompt for approvals, which print mode cannot answer. FS
//! confinement still comes from the engine bubblewrap sandbox.
//!
//! `--process-model in-process` keeps the run inside the supervised
//! child — the shared background daemon may not be reachable/reliable
//! inside the sandbox.

use crate::runtime::cli::{CliSpec, run_cli};
use crate::runtime::{
    AdapterCapabilities, IsolationLevel, RunOutcome, RunRequest, RuntimeAdapter, RuntimeEvent,
};
use anyhow::Result;
use std::cell::Cell;

/// The desktop engine embeds the chosen catalog model as a
/// `[Model: <id>]` marker inside the prompt text (RunRequest has no
/// model field). Extract it once and forward it as `--model`.
fn extract_model(prompt: &str) -> (Option<String>, String) {
    let Some(start) = prompt.find("[Model: ") else {
        return (None, prompt.to_string());
    };
    let rest = &prompt[start + "[Model: ".len()..];
    let Some(end) = rest.find(']') else {
        return (None, prompt.to_string());
    };
    let model = rest[..end].trim().to_string();
    if model.is_empty() {
        return (None, prompt.to_string());
    }
    let mut cleaned = String::with_capacity(prompt.len());
    cleaned.push_str(&prompt[..start]);
    cleaned.push_str(&rest[end + 1..]);
    // Drop the newline that separated the marker from the prompt body.
    if cleaned[start..].starts_with('\n') {
        cleaned.remove(start);
    }
    (Some(model), cleaned)
}

fn build_args(req: &RunRequest) -> Vec<String> {
    let (model, prompt) = extract_model(&req.prompt);
    let mut args = vec![
        "-p".into(),
        "--permission-mode".into(),
        "dangerous".into(),
        "--respect-workspace-trust".into(),
        "false".into(),
        "--process-model".into(),
        "in-process".into(),
    ];
    if let Some(model) = model {
        args.push("--model".into());
        args.push(model);
    }
    // `--` terminates option parsing: a prompt starting with '-' or
    // containing flag-like text is passed through verbatim.
    args.push("--".into());
    args.push(prompt);
    args
}

thread_local! {
    /// run_cli reads stdout on one thread per attempt; the flag emits
    /// SessionStarted exactly once per run on that thread.
    static SESSION_STARTED: Cell<bool> = const { Cell::new(false) };
}

/// Print mode has no event protocol: stdout IS the response text.
/// Each line is streamed to the UI as assistant text; the first line
/// also carries SessionStarted so the shell can show "Connected".
fn parse_line(line: &str) -> Vec<RuntimeEvent> {
    let mut out = Vec::new();
    let first = SESSION_STARTED.with(|f| !f.replace(true));
    if first {
        out.push(RuntimeEvent::SessionStarted { model: None });
    }
    out.push(RuntimeEvent::AssistantMessage {
        text: format!("{line}\n"),
    });
    out
}

const SPEC: CliSpec = CliSpec {
    program: "devin",
    build_args,
    parse_line,
};

pub struct DevinAdapter;

impl RuntimeAdapter for DevinAdapter {
    fn name(&self) -> &'static str {
        "devin"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            mediated_tools: false,
            cancellable: true,
            // Lines stream live as AssistantMessage events, but there is
            // no tool-call or token-usage telemetry to normalize.
            streaming_events: true,
            workspace_isolation: IsolationLevel::ExternalOnly,
            resource_accounting: false,
            limitations: vec![
                "print mode emits plain text only: no tool-call events, no usage accounting".into(),
                "--permission-mode dangerous auto-approves all tools; fs confinement relies on the engine bwrap sandbox".into(),
                "tool calls are not mediated by Aucteris".into(),
            ],
        }
    }

    fn run(&self, req: RunRequest) -> Result<RunOutcome> {
        let mut outcome = run_cli(&SPEC, &req)?;
        // Print mode has no result envelope: stdout IS the reply. Fold
        // the persisted raw lines back into agent_assertion so reports
        // carry the agent's claim instead of "<none>".
        if outcome.exit_code == 0
            && let Ok(raw) = std::fs::read_to_string(&req.events_path)
        {
            let text = raw
                .lines()
                .filter(|line| !line.starts_with("{\"type\":\"_stderr_tail\""))
                .collect::<Vec<_>>()
                .join("\n");
            if !text.trim().is_empty() {
                outcome.agent_assertion = Some(text);
            }
        }
        Ok(outcome)
    }

    fn sandbox_mounts(&self) -> crate::sandbox::SandboxMounts {
        let home = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        let data = home.join(".local/share/devin");
        let config = home.join(".config/devin");
        crate::sandbox::SandboxMounts {
            // Sessions/logs/binary versions live under data (rw — the
            // CLI writes session state); config holds user settings.
            // Credentials and config stay read-only on top.
            rw_dirs: vec![data.clone(), config.clone()],
            ro_dirs: vec![],
            ro_files: vec![data.join("credentials.toml"), config.join("config.json")],
            tmpfs_over: vec![],
        }
    }

    fn egress_domains(&self) -> Vec<String> {
        vec!["api.devin.ai".into()]
    }

    fn supports_mcp(&self) -> bool {
        // Devin CLI has its own MCP config surface; the engine's
        // canonical mcpServers JSON is not consumed — declared false
        // so memory injection records an honest limitation.
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_model_strips_marker() {
        let (model, cleaned) = extract_model("[Model: swe-2-flash-medium]\nDo the thing");
        assert_eq!(model.as_deref(), Some("swe-2-flash-medium"));
        assert_eq!(cleaned, "Do the thing");
    }

    #[test]
    fn extract_model_passthrough_without_marker() {
        let (model, cleaned) = extract_model("plain prompt");
        assert_eq!(model, None);
        assert_eq!(cleaned, "plain prompt");
    }

    #[test]
    fn build_args_include_model_and_print_mode() {
        let dir = std::env::temp_dir().join("aucteris-devin-adapter-test");
        std::fs::create_dir_all(&dir).unwrap();
        let req = RunRequest {
            workspace: dir.clone(),
            prompt: "[Model: swe-2-medium]\nhello".into(),
            events_path: dir.join("events.jsonl"),
            max_turns: 5,
            pid_file: None,
            sandbox: None,
            cgroup_name: None,
            egress_filter: None,
            mcp: None,
            event_sink: None,
            resume_session: None,
            interactions: None,
            steering: None,
        };
        let args = build_args(&req);
        assert_eq!(args[0], "-p");
        assert!(args.contains(&"--permission-mode".to_string()));
        assert!(args.contains(&"dangerous".to_string()));
        assert!(args.contains(&"--respect-workspace-trust".to_string()));
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"swe-2-medium".to_string()));
        // Prompt goes last, after the `--` separator, verbatim.
        assert_eq!(args[args.len() - 2], "--");
        assert_eq!(args[args.len() - 1], "hello");
        assert!(!args.iter().any(|a| a.contains("[Model:")));
    }
}
