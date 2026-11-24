use super::super::table;
use super::code::{html_preview_button, structured_code_view, syntax_code};
use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use gpui::{
    AnyElement, ClipboardItem, Context, FontWeight, HighlightStyle, IntoElement, ParentElement,
    Styled, StyledText, div, prelude::*, px,
};

/// Render the small, common Markdown subset used by assistant prose without
/// exposing formatting markers in the transcript.
pub(in crate::presentation::ui::chat) fn markdown_line(source: &str, theme: &Theme) -> AnyElement {
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

pub(super) fn markdown_view(
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
                    .gap(px(metrics::PX_2))
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
                    .gap(px(metrics::PX_2))
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
                        cx.write_to_clipboard(ClipboardItem::new_string(clipboard_text.clone()));
                    }))
                    .child("Copy");
                let toggle = div()
                    .id(("toggle-code", code_id))
                    .px(px(metrics::SPACE_2))
                    .h(px(metrics::SPACE_5))
                    .flex()
                    .items_center()
                    .rounded(px(metrics::PX_6))
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
                        .my(px(metrics::SPACE_1))
                        .rounded(px(metrics::SPACE_2))
                        .bg(theme.inset)
                        .child(
                            div()
                                .px(px(metrics::PX_10))
                                .h(px(metrics::PX_32))
                                .flex()
                                .items_center()
                                .gap(px(metrics::PX_6))
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
                                .px(px(metrics::SPACE_3))
                                .py(px(metrics::PX_9))
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
        if table::looks_like_table_header(line, lines.peek().copied()) {
            flush_paragraph(&mut paragraph, &mut blocks);
            flush_list(&mut list_items, &mut blocks);
            let mut table_lines = vec![line.to_string()];
            if let Some(separator) = lines.next() {
                table_lines.push(separator.to_string());
            }
            while lines.peek().is_some_and(|row| table::is_table_row(row)) {
                if let Some(row) = lines.next() {
                    table_lines.push(row.to_string());
                }
            }
            blocks.push(table::render_table(&table_lines, theme));
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
                    .gap(px(metrics::SPACE_2))
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
                    .mt(px(metrics::SPACE_1))
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
                .px(px(metrics::SPACE_3))
                .py(px(metrics::PX_9))
                .rounded(px(metrics::SPACE_2))
                .bg(theme.inset)
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(metrics::PX_6))
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
                                    .px(px(metrics::SPACE_2))
                                    .h(px(metrics::SPACE_5))
                                    .flex()
                                    .items_center()
                                    .rounded(px(metrics::PX_6))
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
