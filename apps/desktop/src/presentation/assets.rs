//! Embedded assets: the icon set and mascots are compiled into the binary,
//! so the app has no runtime asset directory to lose.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

macro_rules! icon_table {
    ($($name:literal),* $(,)?) => {
        pub const ICONS: &[(&str, &[u8])] = &[
            $(
                (
                    concat!("icons/", $name, ".svg"),
                    include_bytes!(concat!("../../assets/icons/", $name, ".svg")) as &[u8],
                ),
            )*
        ];
    };
}

icon_table!(
    "plus",
    "search",
    "send",
    "paperclip",
    "chevron-down",
    "chevron-right",
    "monitor",
    "clock",
    "folder",
    "terminal",
    "sparkle",
    "check",
    "x",
    "stop",
    "panel",
    "sidebar",
    "refresh",
    "mail",
    "calendar",
    "globe",
    "more",
    "bot",
    "sun",
    "moon",
    "edit",
    "file",
    "copy",
    "gear",
    "settings-outline",
    "sliders",
    "pin",
    "trash",
    "users",
    "anthropic-mark",
    "google-color",
    "notion",
    "claude-mark",
    "openai-mark",
    "opencode-mark",
    "gemini-mark",
    "devin-mark",
    "star",
);

macro_rules! mascot_svg_table {
    ($($shape:literal),* $(,)?) => {
        pub const MASCOT_SVGS: &[(&str, &[u8])] = &[
            $(
                (
                    concat!("icons/mascots/shape-", $shape, ".svg"),
                    include_bytes!(concat!("../../assets/icons/mascots/shape-", $shape, ".svg")) as &[u8],
                ),
                (
                    concat!("icons/mascots/face-", $shape, ".svg"),
                    include_bytes!(concat!("../../assets/icons/mascots/face-", $shape, ".svg")) as &[u8],
                ),
                (
                    concat!("icons/mascots/volume-", $shape, ".svg"),
                    include_bytes!(concat!("../../assets/icons/mascots/volume-", $shape, ".svg")) as &[u8],
                ),
            )*
        ];
    };
}

mascot_svg_table!(
    "circle", "triangle", "square", "diamond", "pentagon", "hexagon", "octagon", "star",
);

/// gpui asset source over embedded [`ICONS`] and [`MASCOT_SVGS`], with filesystem fallback.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = ICONS.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(*bytes)));
        }
        if let Some((_, bytes)) = MASCOT_SVGS.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(*bytes)));
        }
        // Fallback to reading file from assets/ directory if present
        if let Ok(bytes) = std::fs::read(format!("assets/{}", path)) {
            return Ok(Some(Cow::Owned(bytes)));
        }
        Ok(None)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut results: Vec<SharedString> = ICONS
            .iter()
            .chain(MASCOT_SVGS.iter())
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect();
        results.sort();
        results.dedup();
        Ok(results)
    }
}
