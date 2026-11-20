//! Aucteris harness layer — the single runtime/session path for chats and
//! missions (ADR-0001).
//!
//! - [`session`]: persistent agent sessions (komet drivers + CLI fallback)
//!   with approvals, questions, steering and interruption.
//! - [`cli_backend`]: `Harness` implementation over a core `RuntimeAdapter`
//!   for runtimes without a komet driver (gemini, mock).
//! - [`mission`]: `RuntimeAdapter` implementation over a komet `Harness`,
//!   so missions can consume the same session layer.
//!
//! komet types are re-exported so consumers never depend on komet crates
//! directly.

pub use komet_harness;
pub use komet_proto;

pub mod cli_backend;
pub mod mission;
pub mod session;
