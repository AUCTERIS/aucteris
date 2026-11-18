//! Deterministic test-double runtime — NOT an agent.
//!
//! Exists so the full pipeline (executor → sandbox → evidence →
//! durable orchestration) can be exercised in CI without provider
//! credentials. Capabilities are declared honestly: it performs no
//! reasoning and its "effects" are scripted via `mock:` directives
//! embedded in the task prompt:
//!
//!   mock:sleep=<secs>   pause before finishing (recovery drills)
//!   mock:write=<path>   create <path> in the workspace with fixed content
//!   mock:fetch=<url>    curl <url> honoring *_PROXY env, write the HTTP
//!                       status code to fetch-<n>.txt — exercises the
//!                       engine egress proxy end-to-end in tests/CI
//!
//! Everything else just emits a synthetic event stream and exits 0.

use crate::runtime::cli::{run_cli, CliSpec};
use crate::runtime::{
    AdapterCapabilities, IsolationLevel, RunOutcome, RunRequest, RuntimeAdapter, RuntimeEvent,
};
use anyhow::Result;

/// Directives are whitespace-separated tokens — mission YAML `>-`
/// folds newlines, so line-based parsing would silently miss them.
fn write_targets(prompt: &str) -> Vec<String> {
    prompt
        .split_whitespace()
        .filter_map(|t| t.strip_prefix("mock:write="))
        .map(|p| {
            p.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || "._-/".contains(c)))
                .to_string()
        })
        .filter(|p| !p.starts_with('/') && !p.split('/').any(|seg| seg == "..") && !p.is_empty())
        .collect()
}

fn sleep_secs(prompt: &str) -> Option<u64> {
    prompt.split_whitespace().find_map(|t| {
        t.strip_prefix("mock:sleep=")
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&s| s <= 3600)
    })
}

fn fetch_urls(prompt: &str) -> Vec<String> {
    prompt
        .split_whitespace()
        .filter_map(|t| t.strip_prefix("mock:fetch="))
        .map(|u| {
            u.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || ":./-_~%?&=#".contains(c)))
                .to_string()
        })
        .filter(|u| u.starts_with("http://") || u.starts_with("https://"))
        .collect()
}

fn build_args(req: &RunRequest) -> Vec<String> {
    let mut script = String::from("echo '{\"type\":\"mock.start\"}'\n");
    if let Some(secs) = sleep_secs(&req.prompt) {
        script.push_str(&format!("sleep {}\n", secs));
    }
    for path in write_targets(&req.prompt) {
        script.push_str(&format!("printf 'mock output\\n' > '{}'\n", path));
    }
    for (i, url) in fetch_urls(&req.prompt).iter().enumerate() {
        // curl honors HTTP(S)_PROXY — this is the egress-filter probe.
        // Status code (000 on connect failure) lands in fetch-<n>.txt
        // so verify_cmd can assert on it deterministically.
        script.push_str(&format!(
            "code=$(curl -s -o /dev/null -w '%{{http_code}}' --max-time 15 '{}'); \
             printf '%s' \"$code\" > 'fetch-{}.txt'; \
             echo '{{\"type\":\"mock.fetch\",\"url\":\"{}\",\"code\":\"'$code'\"}}'\n",
            url, i, url
        ));
    }
    script
        .push_str("echo '{\"type\":\"mock.final\",\"text\":\"mock assertion: task completed\"}'\n");
    vec!["-c".into(), script]
}

fn parse_line(line: &str) -> Vec<RuntimeEvent> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return vec![];
    };
    match v.get("type").and_then(|t| t.as_str()) {
        Some("mock.start") => vec![RuntimeEvent::SessionStarted {
            model: Some("mock".into()),
        }],
        Some("mock.fetch") => vec![RuntimeEvent::ToolCall {
            name: format!(
                "fetch:{}",
                v.get("code").and_then(|c| c.as_str()).unwrap_or("?")
            ),
        }],
        Some("mock.final") => vec![RuntimeEvent::FinalResult {
            text: v
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
        }],
        _ => vec![],
    }
}

const SPEC: CliSpec = CliSpec {
    program: "sh",
    build_args,
    parse_line,
};

pub struct MockAdapter;

impl RuntimeAdapter for MockAdapter {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            mediated_tools: false,
            cancellable: true,
            streaming_events: true,
            workspace_isolation: IsolationLevel::ExternalOnly,
            resource_accounting: false,
            limitations: vec![
                "deterministic test double — no agent reasoning; for pipeline and CI testing only".into(),
                "effects are scripted via mock: directives; not a substitute for real runtime contract tests".into(),
            ],
        }
    }

    fn run(&self, req: RunRequest) -> Result<RunOutcome> {
        run_cli(&SPEC, &req)
    }

    fn egress_domains(&self) -> Vec<String> {
        // Test hook: AUCTERIS_MOCK_EGRESS="d1,d2" extends the allowlist
        // so CI can exercise allowed/denied paths deterministically.
        std::env::var("AUCTERIS_MOCK_EGRESS")
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }
}
