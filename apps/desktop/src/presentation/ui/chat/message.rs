//! Rendering one bot message: author plate, parts stream, undo buttons.

use gpui::{
    Animation, AnimationExt, AnyElement, ClipboardItem, Context, FontWeight, HighlightStyle,
    IntoElement, ParentElement, Styled, StyledText, div, img, prelude::*, px,
};
use std::time::Duration;

use crate::domain::model::{Author, Message, Part, is_internal_tool};
use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets;

fn pulsing_icon(icon: AnyElement, id: (&'static str, usize)) -> AnyElement {
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

fn public_tool_label(label: &str) -> String {
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

fn public_tool_detail(label: &str, detail: Option<&str>) -> Option<String> {
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

fn file_diff_view(old: &str, new: &str, theme: &Theme) -> AnyElement {
    let old_lines: Vec<_> = old.lines().collect();
    let new_lines: Vec<_> = new.lines().collect();
    let mut rows: Vec<(Option<usize>, Option<usize>, char, String, gpui::Hsla)> = Vec::new();
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
        .max_h(px(360.0))
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
                .py(px(1.0))
                .text_size(px(typo::TINY))
                .child(
                    div()
                        .w(px(56.0))
                        .flex_shrink_0()
                        .flex()
                        .justify_end()
                        .gap(px(4.0))
                        .text_color(theme.ink_tertiary)
                        .child(old_no.map(|number| number.to_string()).unwrap_or_default())
                        .child(new_no.map(|number| number.to_string()).unwrap_or_default()),
                )
                .child(
                    div()
                        .w(px(12.0))
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

fn syntax_code(source: &str, language: &str, theme: &Theme) -> AnyElement {
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

fn structured_code_view(source: &str, language: &str, theme: &Theme) -> Option<AnyElement> {
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
                    .py(px(2.0))
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
        return Some(super::table::render_table(&markdown, theme));
    }
    None
}

fn html_preview_button(
    source: String,
    id: usize,
    theme: &Theme,
    cx: &mut Context<Shell>,
) -> AnyElement {
    div()
        .id(("preview-html", id))
        .px(px(8.0))
        .h(px(24.0))
        .flex()
        .items_center()
        .rounded(px(6.0))
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
                        rows.push((depth, format!("{key}:").into(), String::new(), true));
                        flatten_json(value, depth + 1, rows);
                    }
                    _ => rows.push((depth, format!("{key}:").into(), value.to_string(), true)),
                }
            }
        }
        serde_json::Value::Array(array) => {
            for (index, value) in array.iter().enumerate() {
                match value {
                    serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                        rows.push((depth, format!("[{index}]").into(), String::new(), true));
                        flatten_json(value, depth + 1, rows);
                    }
                    _ => rows.push((depth, format!("[{index}]").into(), value.to_string(), true)),
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

/// Render the small, common Markdown subset used by assistant prose without
/// exposing formatting markers in the transcript.
pub(super) fn markdown_line(source: &str, theme: &Theme) -> AnyElement {
    let mut text = String::new();
    let mut highlights = Vec::new();
    let mut rest = source;
    let mut emphasis: Option<(usize, &'static str)> = None;

    while !rest.is_empty() {
        let marker = if rest.starts_with("**") {
            Some(("**", "bold"))
        } else if rest.starts_with('`') {
            Some(("`", "code"))
        } else {
            None
        };
        if let Some((marker, kind)) = marker {
            if let Some((start, active_kind)) = emphasis
                && active_kind == kind
            {
                let style = if kind == "bold" {
                    HighlightStyle {
                        font_weight: Some(FontWeight::BOLD),
                        ..Default::default()
                    }
                } else {
                    HighlightStyle {
                        color: Some(theme.accent),
                        background_color: Some(theme.raised),
                        ..Default::default()
                    }
                };
                if start < text.len() {
                    highlights.push((start..text.len(), style));
                }
                emphasis = None;
                rest = &rest[marker.len()..];
                continue;
            }
            if emphasis.is_none() {
                emphasis = Some((text.len(), kind));
                rest = &rest[marker.len()..];
                continue;
            }
        }
        if rest.starts_with('[')
            && let Some(label_end) = rest.find("](")
            && let Some(url_end) = rest[label_end + 2..].find(')')
        {
            text.push_str(&rest[1..label_end]);
            rest = &rest[label_end + 2 + url_end + 1..];
            continue;
        }
        let ch = rest.chars().next().expect("non-empty remainder");
        text.push(ch);
        rest = &rest[ch.len_utf8()..];
    }

    // Incomplete markers can arrive during streaming; keep their content
    // visible and avoid leaving a trailing formatting delimiter behind.
    if let Some((start, kind)) = emphasis {
        let delimiter_len = if kind == "bold" { 2 } else { 1 };
        text.insert_str(start, if kind == "bold" { "**" } else { "`" });
        for (range, _) in &mut highlights {
            if range.start >= start {
                range.start += delimiter_len;
            }
            if range.end >= start {
                range.end += delimiter_len;
            }
        }
    }

    let rendered = StyledText::new(text).with_highlights(highlights);
    div()
        .w_full()
        .min_w_0()
        .whitespace_normal()
        .child(rendered)
        .into_any_element()
}

fn markdown_view(
    source: &str,
    theme: &Theme,
    code_id_base: usize,
    expanded_code_blocks: &std::collections::HashSet<usize>,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let mut blocks = Vec::new();
    let mut in_code = false;
    let mut code_lines = Vec::new();
    let mut code_language = String::new();
    let mut code_index = 0usize;
    let mut paragraph = Vec::new();
    let mut list_items = Vec::new();

    let flush_paragraph = |paragraph: &mut Vec<AnyElement>, blocks: &mut Vec<AnyElement>| {
        if !paragraph.is_empty() {
            blocks.push(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .children(std::mem::take(paragraph))
                    .into_any_element(),
            );
        }
    };
    let flush_list = |list_items: &mut Vec<AnyElement>, blocks: &mut Vec<AnyElement>| {
        if !list_items.is_empty() {
            blocks.push(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .children(std::mem::take(list_items))
                    .into_any_element(),
            );
        }
    };

    let mut lines = source.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("```") {
            if in_code {
                let code = code_lines.join("\n");
                let code_id = code_id_base.wrapping_add(code_index);
                code_index += 1;
                let expanded = expanded_code_blocks.contains(&code_id);
                let lines: Vec<_> = code.lines().collect();
                let is_long = lines.len() > 12;
                let shown = if is_long && !expanded {
                    lines[..12].join("\n")
                } else {
                    code.clone()
                };
                let clipboard_text = code.clone();
                let copy = div()
                    .id(("copy-code", code_id))
                    .px(px(8.0))
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.hover_wash()))
                    .text_size(px(typo::TINY))
                    .text_color(theme.ink_secondary)
                    .on_click(cx.listener(move |_this, _event, _window, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(clipboard_text.clone()));
                    }))
                    .child("Copy");
                let toggle = div()
                    .id(("toggle-code", code_id))
                    .px(px(8.0))
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.hover_wash()))
                    .text_size(px(typo::TINY))
                    .text_color(theme.ink_secondary)
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        if !this.expanded_code_blocks.remove(&code_id) {
                            this.expanded_code_blocks.insert(code_id);
                        }
                        cx.notify();
                    }))
                    .child(if expanded { "Collapse" } else { "Show all" });
                let language_label = if code_language.is_empty() {
                    "Code".to_string()
                } else {
                    code_language.clone()
                };
                let code_view = structured_code_view(&shown, &code_language, theme)
                    .unwrap_or_else(|| syntax_code(&shown, &code_language, theme));
                blocks.push(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .my(px(4.0))
                        .rounded(px(8.0))
                        .bg(theme.inset)
                        .child(
                            div()
                                .px(px(10.0))
                                .h(px(32.0))
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(typo::TINY))
                                        .text_color(theme.ink_tertiary)
                                        .child(language_label),
                                )
                                .when(is_long, |this| this.child(toggle))
                                .when(
                                    code_language
                                        .split_whitespace()
                                        .next()
                                        .is_some_and(|lang| lang.eq_ignore_ascii_case("html")),
                                    |this| {
                                        this.child(html_preview_button(
                                            code.clone(),
                                            code_id,
                                            theme,
                                            cx,
                                        ))
                                    },
                                )
                                .child(copy),
                        )
                        .child(
                            div()
                                .w_full()
                                .w_full()
                                .min_w_0()
                                .px(px(12.0))
                                .py(px(9.0))
                                .font_family(theme.font_mono.clone())
                                .text_size(px(typo::META))
                                .line_height(px(typo::BODY))
                                .text_color(theme.ink_secondary)
                                .child(code_view),
                        )
                        .into_any_element(),
                );
                code_lines.clear();
                code_language.clear();
                in_code = false;
            } else {
                flush_paragraph(&mut paragraph, &mut blocks);
                flush_list(&mut list_items, &mut blocks);
                code_language = line.trim_start_matches('`').trim().to_string();
                in_code = true;
            }
            continue;
        }
        if in_code {
            code_lines.push(line.to_string());
            continue;
        }
        if super::table::looks_like_table_header(line, lines.peek().copied()) {
            flush_paragraph(&mut paragraph, &mut blocks);
            flush_list(&mut list_items, &mut blocks);
            let mut table_lines = vec![line.to_string()];
            if let Some(separator) = lines.next() {
                table_lines.push(separator.to_string());
            }
            while lines
                .peek()
                .is_some_and(|row| super::table::is_table_row(row))
            {
                if let Some(row) = lines.next() {
                    table_lines.push(row.to_string());
                }
            }
            blocks.push(super::table::render_table(&table_lines, theme));
            continue;
        }
        if line.trim().is_empty() {
            flush_paragraph(&mut paragraph, &mut blocks);
            flush_list(&mut list_items, &mut blocks);
            continue;
        }

        let trimmed = line.trim_start();
        let heading_level = trimmed.chars().take_while(|ch| *ch == '#').count();
        let (content, heading) = if heading_level > 0 && trimmed[heading_level..].starts_with(' ') {
            (trimmed[heading_level..].trim_start(), true)
        } else {
            (line, false)
        };
        let list_prefix = if content.starts_with("- [ ] ") || content.starts_with("* [ ] ") {
            Some((6, "☐"))
        } else if content.starts_with("- [x] ")
            || content.starts_with("- [X] ")
            || content.starts_with("* [x] ")
            || content.starts_with("* [X] ")
        {
            Some((6, "☑"))
        } else if content.starts_with("- ") || content.starts_with("* ") {
            Some((2, "•"))
        } else {
            let digits = content.chars().take_while(char::is_ascii_digit).count();
            (digits > 0 && content[digits..].starts_with(". ")).then_some((digits + 2, "•"))
        };
        if let Some((prefix_len, bullet)) = list_prefix {
            flush_paragraph(&mut paragraph, &mut blocks);
            list_items.push(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(theme.ink_tertiary)
                            .child(bullet),
                    )
                    .child(markdown_line(&content[prefix_len..], theme))
                    .into_any_element(),
            );
        } else if heading {
            flush_paragraph(&mut paragraph, &mut blocks);
            flush_list(&mut list_items, &mut blocks);
            blocks.push(
                div()
                    .mt(px(4.0))
                    .text_size(px(typo::BODY))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(markdown_line(content, theme))
                    .into_any_element(),
            );
        } else {
            flush_list(&mut list_items, &mut blocks);
            paragraph.push(markdown_line(line, theme));
        }
    }
    if in_code {
        let code = code_lines.join("\n");
        let code_id = code_id_base.wrapping_add(code_index);
        let expanded = expanded_code_blocks.contains(&code_id);
        let lines: Vec<_> = code.lines().collect();
        let is_long = lines.len() > 12;
        let shown = if is_long && !expanded {
            lines[..12].join("\n")
        } else {
            code.clone()
        };
        let clipboard_text = code.clone();
        let code_view = structured_code_view(&shown, &code_language, theme)
            .unwrap_or_else(|| syntax_code(&shown, &code_language, theme));
        blocks.push(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .px(px(12.0))
                .py(px(9.0))
                .rounded(px(8.0))
                .bg(theme.inset)
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(typo::TINY))
                                .text_color(theme.ink_tertiary)
                                .child(if code_language.is_empty() {
                                    "Code".to_string()
                                } else {
                                    code_language.clone()
                                }),
                        )
                        .when(is_long, |this| {
                            this.child(
                                div()
                                    .id(("toggle-code", code_id))
                                    .px(px(8.0))
                                    .h(px(24.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(6.0))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme.hover_wash()))
                                    .text_size(px(typo::TINY))
                                    .text_color(theme.ink_secondary)
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        if !this.expanded_code_blocks.remove(&code_id) {
                                            this.expanded_code_blocks.insert(code_id);
                                        }
                                        cx.notify();
                                    }))
                                    .child(if expanded { "Collapse" } else { "Show all" }),
                            )
                        })
                        .when(
                            code_language
                                .split_whitespace()
                                .next()
                                .is_some_and(|lang| lang.eq_ignore_ascii_case("html")),
                            |this| {
                                this.child(html_preview_button(code.clone(), code_id, theme, cx))
                            },
                        )
                        .child(
                            div()
                                .id(("copy-code", code_id))
                                .px(px(8.0))
                                .h(px(24.0))
                                .flex()
                                .items_center()
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .hover(|style| style.bg(theme.hover_wash()))
                                .text_size(px(typo::TINY))
                                .text_color(theme.ink_secondary)
                                .on_click(cx.listener(move |_this, _event, _window, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        clipboard_text.clone(),
                                    ));
                                }))
                                .child("Copy"),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .font_family(theme.font_mono.clone())
                        .text_size(px(typo::META))
                        .line_height(px(typo::BODY))
                        .text_color(theme.ink_secondary)
                        .child(code_view),
                )
                .into_any_element(),
        );
    }
    flush_paragraph(&mut paragraph, &mut blocks);
    flush_list(&mut list_items, &mut blocks);
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(metrics::SPACE_2))
        .children(blocks)
        .into_any_element()
}

fn activity_summary(
    message: &Message,
    is_active: bool,
    status: Option<&str>,
    fallback_index: usize,
) -> Option<(String, &'static str, Option<u64>, usize)> {
    if !message.streaming
        && let Some(duration_ms) = message.duration_ms
    {
        return Some((
            format!("Durée d’exécution : {}", human_duration(duration_ms)),
            "clock",
            None,
            fallback_index,
        ));
    }
    if is_active && matches!(message.parts.last(), Some(Part::Text(_))) {
        return Some((
            "Writing response".into(),
            "edit",
            message
                .started_at
                .map(|start| start.elapsed().as_millis() as u64),
            fallback_index,
        ));
    }
    if is_active && let Some(status) = status {
        let status = status.to_ascii_lowercase();
        let current_phase = if status.contains("think") {
            Some(("Reasoning", "sparkle"))
        } else if status.contains("writ") || status.contains("stream") {
            Some(("Writing response", "edit"))
        } else if status.contains("approval") {
            Some(("Waiting for approval", "check"))
        } else if status.contains("answer") {
            Some(("Waiting for your answer", "mail"))
        } else {
            None
        };
        if let Some((label, icon)) = current_phase {
            return Some((
                label.into(),
                icon,
                message
                    .started_at
                    .map(|start| start.elapsed().as_millis() as u64),
                fallback_index,
            ));
        }
    }
    for (index, part) in message.parts.iter().enumerate().rev() {
        match part {
            Part::Thought { .. } => {
                return Some(("Reasoning".into(), "sparkle", None, index));
            }
            Part::Tool {
                icon,
                label,
                detail,
                duration_ms,
                started_at,
                ..
            } if !is_internal_tool(label, detail.as_deref()) => {
                let label = public_tool_label(label);
                let detail = public_tool_detail(label.as_str(), detail.as_deref());
                let title = detail.map_or(label.clone(), |detail| format!("{label} · {detail}"));
                let elapsed = duration_ms
                    .or_else(|| started_at.map(|start| start.elapsed().as_millis() as u64));
                return Some((title, icon, elapsed, index));
            }
            Part::Files { path, .. } => {
                let file = std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(path);
                return Some((format!("Edited {file}"), "file", None, index));
            }
            Part::Note { .. } => return Some(("Updated workspace".into(), "sparkle", None, index)),
            Part::Approval { .. } => {
                return Some(("Waiting for approval".into(), "check", None, index));
            }
            Part::Question { .. } => {
                return Some(("Waiting for your answer".into(), "mail", None, index));
            }
            _ => {}
        }
    }
    if is_active {
        return Some(("Working".into(), "sparkle", None, fallback_index));
    }
    None
}

fn human_duration(duration_ms: u64) -> String {
    let seconds = duration_ms / 1000;
    if seconds >= 3600 {
        format!("{}h {}min", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}min {}s", seconds / 60, seconds % 60)
    } else {
        format!("{:.1}s", duration_ms as f64 / 1000.0)
    }
}

fn activity_row(
    summary: (String, &'static str, Option<u64>, usize),
    expanded: bool,
    live: bool,
    theme: &Theme,
    cx: &mut Context<Shell>,
    chat_index: usize,
    message_index: usize,
) -> AnyElement {
    let (label, icon, duration_ms, event_index) = summary;
    let label_salt = label.bytes().fold(0usize, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(byte as usize)
    });
    let motion_id = chat_index * 1_000_000 + message_index * 1_000 + event_index + label_salt;
    let mark = widgets::icon(
        icon,
        metrics::ICON_SM,
        if live {
            theme.accent
        } else {
            theme.ink_secondary
        },
    );
    let mark = if live {
        pulsing_icon(mark, ("activity-pulse", motion_id))
    } else {
        mark
    };
    let chat = chat_index;
    let message = message_index;
    let label_view = div()
        .min_w_0()
        .truncate()
        .text_size(px(typo::META))
        .text_color(theme.ink_secondary)
        .child(label)
        .with_animation(
            ("activity-swap", motion_id),
            Animation::new(Duration::from_millis(180)),
            |element, progress| element.opacity(0.65 + progress * 0.35),
        );
    div()
        .id(("activity-toggle", message_index))
        .w_full()
        .min_w_0()
        .h(px(metrics::CONTROL_MD))
        .mt(px(metrics::SPACE_1))
        .flex()
        .items_center()
        .gap(px(metrics::SPACE_2))
        .cursor_pointer()
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.dispatch(
                crate::presentation::ui::action::UiAction::ToggleActivity { chat, message },
                cx,
            );
        }))
        .child(mark)
        .child(
            div()
                .min_w_0()
                .max_w(px(metrics::BUBBLE_MAX_WIDTH - 72.0))
                .child(label_view),
        )
        .child(widgets::icon(
            if expanded {
                "chevron-down"
            } else {
                "chevron-right"
            },
            9.0,
            theme.ink_tertiary,
        ))
        .child(div().flex_1())
        .when_some(duration_ms, |this, duration_ms| {
            this.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(typo::TINY))
                    .text_color(theme.ink_tertiary)
                    .child(format!("{:.1}s", duration_ms as f64 / 1000.0)),
            )
        })
        .into_any_element()
}

impl Shell {
    pub(super) fn message_view(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        chat_index: usize,
        message_index: usize,
        message: &Message,
    ) -> AnyElement {
        match message.author {
            Author::You => {
                let attachments: Vec<AnyElement> = message
                    .parts
                    .iter()
                    .enumerate()
                    .filter_map(|(attachment_index, part)| {
                        let Part::Attachment { path } = part else {
                            return None;
                        };
                        let path_for_open = path.clone();
                        let name = std::path::Path::new(path)
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or(path)
                            .to_string();
                        let extension = std::path::Path::new(path)
                            .extension()
                            .and_then(|value| value.to_str())
                            .unwrap_or("")
                            .to_ascii_lowercase();
                        let is_image = matches!(
                            extension.as_str(),
                            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"
                        );
                        let file_body = if is_image {
                            img(std::path::PathBuf::from(path.clone()))
                                .max_w(px(320.0))
                                .max_h(px(220.0))
                                .rounded(px(metrics::RADIUS_CONTROL))
                                .into_any_element()
                        } else {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(metrics::SPACE_2))
                                .child(widgets::icon("file", metrics::ICON_MD, theme.ink_secondary))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .text_size(px(typo::META))
                                                .text_color(theme.ink)
                                                .child(name.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(typo::TINY))
                                                .text_color(theme.ink_secondary)
                                                .child(if extension == "pdf" {
                                                    "PDF document"
                                                } else {
                                                    "Attached file"
                                                }),
                                        ),
                                )
                                .into_any_element()
                        };
                        Some(
                            div()
                                .w_full()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap(px(metrics::SPACE_2))
                                .p(px(metrics::SPACE_2))
                                .rounded(px(metrics::RADIUS_CONTROL))
                                .bg(theme.card)
                                .border_1()
                                .border_color(theme.hairline)
                                .child(file_body)
                                .child(
                                    div()
                                        .id((
                                            "open-source-attachment",
                                            message_index * 128 + attachment_index,
                                        ))
                                        .ml_auto()
                                        .px(px(metrics::SPACE_2))
                                        .h(px(metrics::CONTROL_XS))
                                        .flex()
                                        .items_center()
                                        .rounded_full()
                                        .cursor_pointer()
                                        .hover(|style| style.bg(theme.hover_wash()))
                                        .text_size(px(typo::TINY))
                                        .text_color(theme.ink_secondary)
                                        .on_click(cx.listener(move |_this, _event, _window, cx| {
                                            cx.open_with_system(std::path::Path::new(
                                                &path_for_open,
                                            ))
                                        }))
                                        .child("Open original"),
                                )
                                .into_any_element(),
                        )
                    })
                    .collect();
                div()
                    .w_full()
                    .flex()
                    .justify_end()
                    .child(
                        div()
                            .max_w(px(metrics::BUBBLE_MAX_WIDTH))
                            .min_w_0()
                            .px(px(metrics::SPACE_3))
                            .py(px(metrics::SPACE_2))
                            .rounded(px(metrics::RADIUS_BUBBLE))
                            .bg(theme.user_plate)
                            .text_size(px(typo::BUBBLE))
                            .line_height(px(typo::BUBBLE_LINE))
                            .text_color(theme.on_user_plate)
                            .when(!message.text().is_empty(), |this| {
                                this.child(message.text())
                            })
                            .children(attachments),
                    )
                    .into_any_element()
            }
            Author::System => div()
                .w_full()
                .flex()
                .justify_center()
                .child(
                    div()
                        .text_size(px(typo::META))
                        .text_color(theme.ink_secondary)
                        .child(message.text()),
                )
                .into_any_element(),
            Author::Bot => {
                let has_approval = message
                    .parts
                    .iter()
                    .any(|part| matches!(part, Part::Approval { .. }));
                let is_active_run = message.streaming
                    && self.run_chat.is_some_and(|run_chat| {
                        self.workspace
                            .chats
                            .get(chat_index)
                            .is_some_and(|chat| chat.id == run_chat)
                    });
                let mut response_body: Vec<AnyElement> = Vec::new();
                let mut activity_details: Vec<AnyElement> = Vec::new();
                for (part_index, part) in message.parts.iter().enumerate() {
                    let is_activity = matches!(
                        part,
                        Part::Thought { .. }
                            | Part::Tool { .. }
                            | Part::Files { .. }
                            | Part::Note { .. }
                    );
                    if is_activity && !message.activity_expanded {
                        continue;
                    }
                    if matches!(part, Part::Tool { label, detail, .. } if is_internal_tool(label, detail.as_deref()))
                    {
                        continue;
                    }
                    let rendered = match part {
                        Part::Text(text) => div()
                            .w_full()
                            .min_w_0()
                            .when(has_approval, |this| {
                                this.max_w(px(metrics::BUBBLE_MAX_WIDTH))
                                    .px(px(metrics::SPACE_3))
                                    .py(px(metrics::SPACE_2))
                                    .rounded(px(metrics::RADIUS_BUBBLE))
                                    .bg(theme.card)
                            })
                            .text_size(px(typo::BUBBLE))
                            .line_height(px(typo::BUBBLE_LINE))
                            .text_color(theme.ink)
                            .child(markdown_view(
                                text,
                                theme,
                                chat_index
                                    .wrapping_mul(1_000_000)
                                    .wrapping_add(message_index.wrapping_mul(1_000)),
                                &self.expanded_code_blocks,
                                cx,
                            ))
                            .into_any_element(),
                        Part::Thought { text, collapsed } => {
                            let chat_for_click = chat_index;
                            let message_for_click = message_index;
                            let part_for_click = part_index;
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(metrics::SPACE_2))
                                .py(px(metrics::SPACE_1))
                                .child(
                                    div()
                                        .id(("thought-toggle", message_index * 64 + part_index))
                                        .h(px(metrics::CONTROL_SM))
                                        .flex()
                                        .items_center()
                                        .gap(px(metrics::SPACE_2))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.dispatch(
                                                crate::presentation::ui::action::UiAction::ToggleThought {
                                                    chat: chat_for_click,
                                                    message: message_for_click,
                                                    part: part_for_click,
                                                },
                                                cx,
                                            );
                                        }))
                                        .child(
                                            div()
                                                .text_size(px(typo::META))
                                                .font_weight(FontWeight::MEDIUM)
                                        .text_color(theme.ink_secondary)
                                                .child("Reasoning"),
                                        )
                                        .child(widgets::icon(
                                            if *collapsed { "chevron-right" } else { "chevron-down" },
                                            9.0,
                                            theme.ink_tertiary,
                                        )),
                                )
                                .when(!*collapsed, |this| {
                                    this.child(
                                        div()
                                            .pl(px(23.0))
                                            .text_size(px(typo::BODY))
                                            .line_height(px(typo::BUBBLE_LINE))
                                            .text_color(theme.ink_secondary)
                                            .child(text.clone()),
                                    )
                                })
                                .into_any_element()
                        }
                        Part::Tool {
                            icon,
                            label,
                            detail,
                            done,
                            failed,
                            result,
                            duration_ms,
                            started_at,
                            ..
                        } => {
                            let display_label = public_tool_label(label);
                            let display_detail = public_tool_detail(label, detail.as_deref());
                            let elapsed_ms = duration_ms.or_else(|| {
                                started_at.map(|start| start.elapsed().as_millis() as u64)
                            });
                            let elapsed =
                                elapsed_ms.map(|ms| format!("{:.1}s", ms as f64 / 1000.0));
                            let state_color = if *failed {
                                theme.danger
                            } else {
                                theme.ink_tertiary
                            };
                            let motion_id =
                                chat_index * 1_000_000 + message_index * 1_000 + part_index;
                            let output_id = 9_000_000_000usize
                                .wrapping_add(chat_index * 1_000_000)
                                .wrapping_add(message_index * 1_000)
                                .wrapping_add(part_index);
                            let output_expanded = self.expanded_code_blocks.contains(&output_id);
                            let tool_mark = widgets::icon(icon, metrics::ICON_SM, state_color);
                            let tool_mark = if !*done {
                                pulsing_icon(tool_mark, ("tool-pulse", motion_id))
                            } else {
                                tool_mark
                            };
                            let row = div()
                                .id(("tool-output", motion_id))
                                .w_full()
                                .min_w_0()
                                .h(px(metrics::CONTROL_MD))
                                .py(px(metrics::SPACE_1))
                                .flex()
                                .items_center()
                                .gap(px(metrics::SPACE_2))
                                .when(result.is_some(), |this| {
                                    this.cursor_pointer().on_click(cx.listener(
                                        move |this, _event, _window, cx| {
                                            if !this.expanded_code_blocks.remove(&output_id) {
                                                this.expanded_code_blocks.insert(output_id);
                                            }
                                            cx.notify();
                                        },
                                    ))
                                })
                                .child(tool_mark)
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .items_center()
                                        .gap(px(metrics::SPACE_2))
                                        .child(
                                            div()
                                                .min_w_0()
                                                .truncate()
                                                .text_size(px(typo::META))
                                                .text_color(if *done {
                                                    theme.ink_secondary
                                                } else {
                                                    theme.ink
                                                })
                                                .child(display_label),
                                        )
                                        .when_some(display_detail, |this, detail| {
                                            this.child(
                                                div()
                                                    .truncate()
                                                    .text_size(px(typo::TINY))
                                                    .text_color(theme.ink_tertiary)
                                                    .child(format!("· {detail}")),
                                            )
                                        }),
                                )
                                .when_some(elapsed, |this, elapsed| {
                                    this.child(
                                        div()
                                            .flex_shrink_0()
                                            .text_size(px(typo::TINY))
                                            .text_color(state_color)
                                            .child(elapsed),
                                    )
                                });
                            let mut tool_view =
                                div().w_full().min_w_0().flex().flex_col().child(row);
                            if output_expanded && let Some(output) = result.as_ref() {
                                let output_for_copy = output.clone();
                                let copy_output = div()
                                    .id(("copy-tool-output", motion_id))
                                    .px(px(8.0))
                                    .h(px(metrics::CONTROL_XS))
                                    .flex()
                                    .items_center()
                                    .rounded_full()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme.hover_wash()))
                                    .text_size(px(typo::TINY))
                                    .text_color(theme.ink_secondary)
                                    .on_click(cx.listener(move |_this, _event, _window, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            output_for_copy.clone(),
                                        ));
                                    }))
                                    .child("Copy");
                                tool_view = tool_view.child(
                                    div()
                                        .w_full()
                                        .ml(px(metrics::ICON_SM + metrics::SPACE_2))
                                        .mb(px(metrics::SPACE_2))
                                        .p(px(metrics::SPACE_2))
                                        .rounded(px(metrics::RADIUS_CONTROL))
                                        .bg(if *failed {
                                            theme.danger.opacity(0.10)
                                        } else {
                                            theme.inset
                                        })
                                        .flex()
                                        .flex_col()
                                        .gap(px(metrics::SPACE_1))
                                        .child(
                                            div()
                                                .w_full()
                                                .flex()
                                                .items_center()
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .text_size(px(typo::TINY))
                                                        .text_color(if *failed {
                                                            theme.danger
                                                        } else {
                                                            theme.ink_tertiary
                                                        })
                                                        .child(if *failed {
                                                            "Error output"
                                                        } else {
                                                            "Output"
                                                        }),
                                                )
                                                .child(copy_output),
                                        )
                                        .child(
                                            div()
                                                .w_full()
                                                .min_w_0()
                                                .font_family(theme.font_mono.clone())
                                                .text_size(px(typo::TINY))
                                                .line_height(px(typo::BODY))
                                                .text_color(if *failed {
                                                    theme.danger
                                                } else {
                                                    theme.ink_secondary
                                                })
                                                .child(output.clone()),
                                        ),
                                );
                            }
                            tool_view.into_any_element()
                        }
                        Part::Files {
                            path,
                            added,
                            removed,
                            old_text,
                            new_text,
                            diff_expanded,
                            undo_error,
                            undone,
                        } => {
                            let file_name = std::path::Path::new(path)
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or(path)
                                .to_string();
                            let open_path = path.clone();
                            let open = div()
                                .id(("open-changed-file", message_index * 32 + part_index))
                                .px(px(metrics::SPACE_2))
                                .h(px(metrics::CONTROL_XS))
                                .flex()
                                .items_center()
                                .rounded_full()
                                .cursor_pointer()
                                .hover(|style| style.bg(theme.hover_wash()))
                                .text_size(px(typo::TINY))
                                .text_color(theme.ink_secondary)
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.dispatch(
                                        crate::presentation::ui::action::UiAction::OpenFile {
                                            chat: chat_index,
                                            path: open_path.clone(),
                                        },
                                        cx,
                                    );
                                }))
                                .child("Open");
                            let toggle = div()
                                .id(("toggle-file-diff", message_index * 32 + part_index))
                                .px(px(metrics::SPACE_2))
                                .h(px(metrics::CONTROL_XS))
                                .flex()
                                .items_center()
                                .rounded_full()
                                .cursor_pointer()
                                .hover(|style| style.bg(theme.hover_wash()))
                                .text_size(px(typo::TINY))
                                .text_color(theme.ink_secondary)
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.dispatch(
                                        crate::presentation::ui::action::UiAction::ToggleFileDiff {
                                            chat: chat_index,
                                            message: message_index,
                                            part: part_index,
                                        },
                                        cx,
                                    );
                                }))
                                .child(if *diff_expanded {
                                    "Hide diff"
                                } else {
                                    "Preview"
                                });
                            div()
                                .w_full()
                                .flex()
                                .flex_col()
                                .gap(px(metrics::SPACE_1))
                                .py(px(metrics::SPACE_1))
                                .child(
                                    div()
                                        .w_full()
                                        .min_w_0()
                                        .flex()
                                        .items_center()
                                        .gap(px(metrics::SPACE_2))
                                        .child(widgets::icon("file", 13.0, theme.ink_tertiary))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .flex()
                                                .flex_col()
                                                .child(
                                                    div()
                                                        .truncate()
                                                        .text_size(px(typo::META))
                                                        .text_color(if *undone {
                                                            theme.ink_tertiary
                                                        } else {
                                                            theme.ink
                                                        })
                                                        .child(file_name),
                                                )
                                                .child(
                                                    div()
                                                        .truncate()
                                                        .text_size(px(typo::TINY))
                                                        .text_color(theme.ink_tertiary)
                                                        .child(path.clone()),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(typo::TINY))
                                                .text_color(theme.success)
                                                .child(format!("+{added}")),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(typo::TINY))
                                                .text_color(theme.danger)
                                                .child(format!("−{removed}")),
                                        )
                                        .when(!*undone, |this| {
                                            this.child(toggle).child(open).when(
                                                new_text.is_some(),
                                                |this| {
                                                    this.child(self.undo_button(
                                                        theme,
                                                        cx,
                                                        chat_index,
                                                        message_index,
                                                        part_index,
                                                    ))
                                                },
                                            )
                                        })
                                        .when(*undone, |this| {
                                            this.child(
                                                div()
                                                    .text_size(px(typo::TINY))
                                                    .text_color(theme.ink_tertiary)
                                                    .child("Undone"),
                                            )
                                        }),
                                )
                                .when(*diff_expanded, |this| {
                                    let preview = match (old_text.as_deref(), new_text.as_deref()) {
                                        (Some(old), Some(new)) => file_diff_view(old, new, theme),
                                        (None, Some(new)) => file_diff_view("", new, theme),
                                        _ => div()
                                            .text_size(px(typo::TINY))
                                            .text_color(theme.ink_tertiary)
                                            .child("Diff preview unavailable for this large file.")
                                            .into_any_element(),
                                    };
                                    this.child(preview)
                                })
                                .when_some(undo_error.as_ref(), |this, error| {
                                    this.child(
                                        div()
                                            .text_size(px(typo::TINY))
                                            .text_color(theme.danger)
                                            .child(error.clone()),
                                    )
                                })
                                .into_any_element()
                        }
                        Part::Note { text, undone } => div()
                            .flex()
                            .items_center()
                            .gap(px(metrics::SPACE_2))
                            .h(px(metrics::CONTROL_MD))
                            .py(px(metrics::SPACE_1))
                            .child(widgets::icon("sparkle", 13.0, theme.ink_tertiary))
                            .child(
                                div()
                                    .flex_1()
                                    .truncate()
                                    .text_size(px(typo::META))
                                    .text_color(if *undone {
                                        theme.ink_tertiary
                                    } else {
                                        theme.ink_secondary
                                    })
                                    .child(text.clone()),
                            )
                            .when(!*undone, |this| {
                                this.child(self.undo_button(
                                    theme,
                                    cx,
                                    chat_index,
                                    message_index,
                                    part_index,
                                ))
                            })
                            .when(*undone, |this| {
                                this.child(
                                    div()
                                        .text_size(px(typo::TINY))
                                        .text_color(theme.ink_tertiary)
                                        .child("Undone"),
                                )
                            })
                            .into_any_element(),
                        Part::Approval { .. } => {
                            self.card_view(theme, cx, chat_index, message_index, part_index, part)
                        }
                        Part::Question { .. } => {
                            self.card_view(theme, cx, chat_index, message_index, part_index, part)
                        }
                        Part::Attachment { .. } => div().into_any_element(),
                    };
                    if is_activity {
                        activity_details.push(rendered);
                    } else {
                        response_body.push(rendered);
                    }
                }

                let mut body: Vec<AnyElement> = Vec::new();
                if let Some(summary) = activity_summary(
                    message,
                    is_active_run,
                    self.status.as_deref(),
                    message.parts.len(),
                ) {
                    body.push(activity_row(
                        summary,
                        message.activity_expanded,
                        is_active_run,
                        theme,
                        cx,
                        chat_index,
                        message_index,
                    ));
                }
                if message.activity_expanded && !activity_details.is_empty() {
                    body.push(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(metrics::SPACE_1))
                            .children(activity_details)
                            .into_any_element(),
                    );
                }
                body.extend(response_body);
                if message.streaming {
                    body.push(
                        div()
                            .mt(px(metrics::SPACE_HALF))
                            .w(px(metrics::CARET_WIDTH))
                            .h(px(typo::BUBBLE))
                            .rounded(px(2.0))
                            .bg(theme.ink_secondary.opacity(0.55))
                            .into_any_element(),
                    );
                }
                if message.stopped {
                    body.push(
                        div()
                            .text_size(px(typo::META))
                            .text_color(theme.ink_tertiary)
                            .child("Stopped on request")
                            .into_any_element(),
                    );
                }
                div()
                    .w_full()
                    .flex()
                    .justify_start()
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .max_w(px(metrics::BUBBLE_MAX_WIDTH))
                            .flex()
                            .flex_col()
                            .gap(px(metrics::SPACE_2))
                            .children(body),
                    )
                    .into_any_element()
            }
        }
    }

    fn undo_button(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        chat_index: usize,
        message_index: usize,
        part_index: usize,
    ) -> AnyElement {
        let hover = theme.hover_wash();
        let color = theme.ink_secondary;
        div()
            .id(("undo", message_index * 32 + part_index))
            .flex_shrink_0()
            .h(px(metrics::CONTROL_XS))
            .px(px(metrics::SPACE_2))
            .flex()
            .items_center()
            .rounded_full()
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .text_size(px(typo::TINY))
            .text_color(color)
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.dispatch(
                    crate::presentation::ui::action::UiAction::UndoPart {
                        chat: chat_index,
                        message: message_index,
                        part: part_index,
                    },
                    cx,
                )
            }))
            .child("Undo")
            .into_any_element()
    }
}
