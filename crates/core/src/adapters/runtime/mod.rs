//! Built-in agent CLI adapters.

mod claude;
mod codex;
mod devin;
mod gemini;
mod mock;
mod opencode;

// Preserve the original public module path while contracts live inward in the
// application ports and process supervision lives in infrastructure.
pub use crate::application::ports::runtime::*;
pub use crate::infrastructure::process as cli;

/// Standard registry for the installed CLI harnesses supported by Aucteris.
pub fn register_builtins(registry: &mut RuntimeRegistry) {
    registry.register(claude::ClaudeAdapter);
    registry.register(codex::CodexAdapter);
    registry.register(opencode::OpenCodeAdapter);
    registry.register(gemini::GeminiAdapter);
    registry.register(devin::DevinAdapter);
    registry.register(mock::MockAdapter);
}
