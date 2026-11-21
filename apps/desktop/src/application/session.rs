//! Re-export of the shared session layer (`crates/harness`): chats and
//! missions consume one runtime path (ADR-0001). Desktop code keeps
//! importing through this module so the komet dependency stays inside the
//! harness crate.

pub use aucteris_harness::session::{Action, EngineEvent, Sessions, backend, komet_backend};
