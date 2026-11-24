use super::super::table;
use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use gpui::{
    Animation, AnimationExt, AnyElement, Context, HighlightStyle, IntoElement, ParentElement,
    Styled, StyledText, div, prelude::*, px,
};
use std::time::Duration;

type FileDiffRow = (Option<usize>, Option<usize>, char, String, gpui::Hsla);

pub(super) fn pulsing_icon(icon: AnyElement, id: (&'static str, usize)) -> AnyElement {
    div()
        .flex_shrink_0()
        .child(icon)
        .with_animation(
            id,
            Animation::new(Duration::from_millis(900)).repeat(),
            |element, delta| element.opacity(0.55 + 0.45 * (delta * std::f32::consts::PI).sin()),
        )
        .into_any_element()
}

pub(super) fn public_tool_label(label: &str) -> String {
    let normalized = label.to_ascii_lowercase();
    if normalized.contains("notion") {
        "Using Notion".to_string()
    } else if normalized.contains("google") || normalized.contains("drive") {
        "Using Google Drive".to_string()
    } else if normalized.contains("slack") {
        "Using Slack".to_string()
    } else {
        match normalized.as_str() {
            "bash" | "exec" | "command" => "Running a command".to_string(),
            "read" | "readfile" | "reading a file" => "Reading a file".to_string(),
            "write" | "edit" | "patch" | "applypatch" | "updating a file" => {
                "Updating a file".to_string()
            }
            "grep" | "search" | "searching files" => "Searching files".to_string(),
            "glob" | "finding files" => "Finding files".to_string(),
            "fetch" | "webfetch" | "opening a webpage" => "Opening a webpage".to_string(),
            "websearch" | "searching the web" => "Searching the web".to_string(),
            "plan" | "todo" | "planning next steps" => "Planning next steps".to_string(),
            "running a command" => "Running a command".to_string(),
            _ if normalized.starts_with("using ") => label.to_string(),
            _ => "Working with a tool".to_string(),
        }
    }
}

pub(super) fn public_tool_detail(label: &str, detail: Option<&str>) -> Option<String> {
    let label = label.to_ascii_lowercase();
    let is_file_action = matches!(
        label.as_str(),
        "read" | "readfile" | "write" | "edit" | "patch" | "applypatch"
    ) || label == "reading a file"
        || label == "updating a file";
    if !is_file_action {
        return None;
    }
    let detail = detail?;
    std::path::Path::new(detail)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .or_else(|| Some(detail.to_string()))
}

pub(super) fn file_diff_view(old: &str, new: &str, theme: &Theme) -> AnyElement {
    let old_lines: Vec<_> = old.lines().collect();
    let new_lines: Vec<_> = new.lines().collect();
    let mut rows: Vec<FileDiffRow> = Vec::new();
    let (mut old_index, mut new_index) = (0usize, 0usize);
    let can_align = old_lines.len().saturating_mul(new_lines.len()) <= 100_000;
    if can_align {
        let mut lcs = vec![vec![0usize; new_lines.len() + 1]; old_lines.len() + 1];
        for i in (0..old_lines.len()).rev() {
            for j in (0..new_lines.len()).rev() {
                lcs[i][j] = if old_lines[i] == new_lines[j] {
                    lcs[i + 1][j + 1] + 1
                } else {
                    lcs[i + 1][j].max(lcs[i][j + 1])
                };
            }
        }
        while old_index < old_lines.len() || new_index < new_lines.len() {
            if old_index < old_lines.len()
                && new_index < new_lines.len()
                && old_lines[old_index] == new_lines[new_index]
            {
                rows.push((
                    Some(old_index + 1),
                    Some(new_index + 1),
                    ' ',
                    old_lines[old_index].to_string(),
                    theme.ink_tertiary,
                ));
                old_index += 1;
                new_index += 1;
            } else if old_index < old_lines.len()
                && (new_index == new_lines.len()
                    || lcs[old_index + 1][new_index] >= lcs[old_index][new_index + 1])
            {
                rows.push((
                    Some(old_index + 1),
                    None,
                    '-',
                    old_lines[old_index].to_string(),
                    theme.danger,
                ));
                old_index += 1;
            } else {
                rows.push((
                    None,
                    Some(new_index + 1),
                    '+',
                    new_lines[new_index].to_string(),
                    theme.success,
                ));
                new_index += 1;
            }
        }
    } else {
        for (index, line) in old_lines.iter().enumerate() {
            rows.push((
                Some(index + 1),
                None,
                '-',
                (*line).to_string(),
                theme.danger,
            ));
        }
        for (index, line) in new_lines.iter().enumerate() {
            rows.push((
                None,
                Some(index + 1),
                '+',
                (*line).to_string(),
                theme.success,
            ));
        }
    }
    let truncated = rows.len() > 240;
    rows.truncate(240);
    let mut body = div()
        .w_full()
        .max_h(px(metrics::PX_360))
        .flex()
        .flex_col()
        .rounded(px(metrics::RADIUS_CONTROL))
        .bg(theme.panel)
        .border_1()
        .border_color(theme.hairline)
        .py(px(metrics::SPACE_1));
    for (old_no, new_no, kind, line, color) in rows {
        let line = if line.is_empty() {
            " ".to_string()
        } else {
            line
        };
        body = body.child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .gap(px(metrics::SPACE_2))
                .px(px(metrics::SPACE_2))
                .py(px(metrics::PX_1))
                .text_size(px(typo::TINY))
                .child(
                    div()
                        .w(px(metrics::PX_56))
                        .flex_shrink_0()
                        .flex()
                        .justify_end()
                        .gap(px(metrics::SPACE_1))
                        .text_color(theme.ink_tertiary)
                        .child(old_no.map(|number| number.to_string()).unwrap_or_default())
                        .child(new_no.map(|number| number.to_string()).unwrap_or_default()),
                )
                .child(
                    div()
                        .w(px(metrics::SPACE_3))
                        .flex_shrink_0()
                        .text_color(color)
                        .child(kind.to_string()),
                )
                .child(div().flex_1().min_w_0().text_color(color).child(line)),
        );
    }
    if truncated {
        body = body.child(
            div()
                .px(px(metrics::SPACE_2))
                .text_size(px(typo::TINY))
                .text_color(theme.ink_tertiary)
                .child("Diff preview shortened to 240 lines."),
        );
    }
    body.into_any_element()
}

pub(super) fn syntax_code(source: &str, language: &str, theme: &Theme) -> AnyElement {
    let bytes = source.as_bytes();
    let mut spans = Vec::new();
    let lang = language.to_ascii_lowercase();
    let keywords: &[&str] = if matches!(
        lang.as_str(),
        "rust" | "rs" | "typescript" | "ts" | "javascript" | "js" | "json"
    ) {
        &[
            "fn", "let", "mut", "pub", "struct", "impl", "use", "mod", "return", "if", "else",
            "match", "for", "while", "const", "async", "await", "true", "false", "null", "self",
            "new", "class", "function", "import", "export", "const", "var",
        ]
    } else {
        &[
            "def", "class", "return", "if", "else", "elif", "for", "while", "import", "from", "as",
            "true", "false", "null", "none", "function", "const", "let", "var",
        ]
    };
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let byte = bytes[i];
        let end = if byte == b'/' && bytes.get(i + 1) == Some(&b'/') || byte == b'#' {
            i += if byte == b'/' { 2 } else { 1 };
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            Some((i, theme.ink_tertiary))
        } else if byte == b'\'' || byte == b'"' || byte == b'`' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if bytes[i] == byte {
                    i += 1;
                    break;
                }
                i += 1;
            }
            Some((i, theme.success))
        } else if byte.is_ascii_digit() {
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_digit()
                    || matches!(bytes[i], b'.' | b'x' | b'a'..=b'f' | b'A'..=b'F' | b'_'))
            {
                i += 1;
            }
            Some((i, theme.accent))
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &source[start..i];
            keywords.contains(&word).then_some((i, theme.accent))
        } else {
            i += 1;
            None
        };
        if let Some((end, color)) = end {
            spans.push((
                start..end,
                HighlightStyle {
                    color: Some(color),
                    ..Default::default()
                },
            ));
        }
    }
    div()
        .w_full()
        .min_w_0()
        .child(StyledText::new(source.to_string()).with_highlights(spans))
        .into_any_element()
}

pub(super) fn structured_code_view(
    source: &str,
    language: &str,
    theme: &Theme,
) -> Option<AnyElement> {
    if language.eq_ignore_ascii_case("json") {
        let value: serde_json::Value = serde_json::from_str(source).ok()?;
        let mut rows = Vec::new();
        flatten_json(&value, 0, &mut rows);
        let mut view = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .py(px(metrics::SPACE_2));
        for (indent, key, value, is_key) in rows {
            view = view.child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .gap(px(metrics::SPACE_2))
                    .pl(px((indent * 14) as f32))
                    .py(px(metrics::PX_2))
                    .text_size(px(typo::META))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(if is_key {
                                theme.accent
                            } else {
                                theme.ink_secondary
                            })
                            .child(key),
                    )
                    .child(div().flex_1().min_w_0().text_color(theme.ink).child(value)),
            );
        }
        return Some(view.into_any_element());
    }
    if matches!(language.to_ascii_lowercase().as_str(), "csv" | "tsv") {
        let delimiter = if language.eq_ignore_ascii_case("tsv") {
            '\t'
        } else {
            ','
        };
        let rows = parse_delimited(source, delimiter);
        if rows.is_empty() {
            return None;
        }
        let mut markdown: Vec<String> = rows
            .iter()
            .map(|row| {
                format!(
                    "| {} |",
                    row.iter()
                        .map(|cell| cell.replace('|', "\\|"))
                        .collect::<Vec<_>>()
                        .join(" | ")
                )
            })
            .collect();
        if let Some(header) = rows.first() {
            markdown.insert(1, format!("| {} |", vec!["---"; header.len()].join(" | ")));
        }
        return Some(table::render_table(&markdown, theme));
    }
    None
}

pub(super) fn html_preview_button(
    source: String,
    id: usize,
    theme: &Theme,
    cx: &mut Context<Shell>,
) -> AnyElement {
    div()
        .id(("preview-html", id))
        .px(px(metrics::SPACE_2))
        .h(px(metrics::SPACE_5))
        .flex()
        .items_center()
        .rounded(px(metrics::PX_6))
        .cursor_pointer()
        .hover(|style| style.bg(theme.hover_wash()))
        .text_size(px(typo::TINY))
        .text_color(theme.ink_secondary)
        .on_click(cx.listener(move |_this, _event, _window, cx| {
            let escaped = source.replace('&', "&amp;").replace('"', "&quot;");
            let document = format!(
                "<!doctype html><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline';\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><style>body{{margin:0;font:14px system-ui;background:#202020;color:#eee}}iframe{{border:0;width:100vw;height:100vh;background:white}}</style><iframe sandbox srcdoc=\"{escaped}\"></iframe>"
            );
            let file = std::env::temp_dir().join(format!(
                "aucteris-preview-{}.html",
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default().as_nanos()
            ));
            if std::fs::write(&file, document).is_ok() {
                cx.open_with_system(&file);
            }
        }))
        .child("Preview")
        .into_any_element()
}

fn flatten_json(
    value: &serde_json::Value,
    depth: usize,
    rows: &mut Vec<(usize, String, String, bool)>,
) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                match value {
                    serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                        rows.push((depth, format!("{key}:"), String::new(), true));
                        flatten_json(value, depth + 1, rows);
                    }
                    _ => rows.push((depth, format!("{key}:"), value.to_string(), true)),
                }
            }
        }
        serde_json::Value::Array(array) => {
            for (index, value) in array.iter().enumerate() {
                match value {
                    serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                        rows.push((depth, format!("[{index}]"), String::new(), true));
                        flatten_json(value, depth + 1, rows);
                    }
                    _ => rows.push((depth, format!("[{index}]"), value.to_string(), true)),
                }
            }
        }
        _ => rows.push((depth, String::new(), value.to_string(), false)),
    }
}

fn parse_delimited(source: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ch if ch == delimiter && !quoted => {
                row.push(std::mem::take(&mut cell));
            }
            '\n' if !quoted => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            '\r' if !quoted => {}
            _ => cell.push(ch),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}
