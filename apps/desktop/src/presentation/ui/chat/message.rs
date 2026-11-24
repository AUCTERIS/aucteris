//! Bot-message rendering split into focused helpers for code, Markdown, activity, and parts.

mod activity;
mod code;
mod markdown;
mod view;

pub(super) use markdown::markdown_line;
