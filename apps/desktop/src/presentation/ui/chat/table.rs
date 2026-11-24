//! Dedicated renderer for Markdown tables in the chat transcript.

use gpui::{
    AnyElement, FontWeight, IntoElement, ParentElement, Styled, TextAlign, div, prelude::*, px,
    relative,
};

use crate::presentation::theme::{Theme, metrics, text as typo};

use super::message::markdown_line;

#[derive(Clone, Copy)]
enum Alignment {
    Left,
    Center,
    Right,
}

impl Alignment {
    fn text_align(self) -> TextAlign {
        match self {
            Self::Left => TextAlign::Left,
            Self::Center => TextAlign::Center,
            Self::Right => TextAlign::Right,
        }
    }
}

pub(super) fn looks_like_table_header(line: &str, next: Option<&str>) -> bool {
    line.contains('|') && next.is_some_and(is_separator_row)
}

pub(super) fn is_table_row(line: &str) -> bool {
    line.contains('|') && !line.trim().is_empty()
}

fn is_separator_row(line: &str) -> bool {
    let cells = split_row(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let cell = cell.trim().trim_matches(':');
            cell.len() >= 3 && cell.chars().all(|ch| ch == '-')
        })
}

fn split_row(line: &str) -> Vec<String> {
    let line = line.trim();
    let line = line.strip_prefix('|').unwrap_or(line);
    let line = line.strip_suffix('|').unwrap_or(line);
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if chars.peek() == Some(&'|') => {
                chars.next();
                cell.push('|');
            }
            '|' => {
                cells.push(cell.trim().to_string());
                cell.clear();
            }
            _ => cell.push(ch),
        }
    }
    cells.push(cell.trim().to_string());
    cells
}

pub(super) fn render_table(lines: &[String], theme: &Theme) -> AnyElement {
    let Some(header_line) = lines.first() else {
        return div().into_any_element();
    };
    let headers = split_row(header_line);
    let columns = headers.len();
    if columns == 0 {
        return div().into_any_element();
    }
    let separators = lines.get(1).map(|line| split_row(line)).unwrap_or_default();
    let alignments = (0..columns)
        .map(|index| alignment(separators.get(index).map(String::as_str).unwrap_or("---")))
        .collect::<Vec<_>>();
    let mut rows = lines
        .iter()
        .skip(2)
        .map(|line| {
            let mut cells = split_row(line);
            cells.resize(columns, String::new());
            cells.truncate(columns);
            cells
        })
        .collect::<Vec<_>>();
    rows.retain(|row| row.iter().any(|cell| !cell.is_empty()));

    let weights = (0..columns)
        .map(|column| {
            let max_chars = headers
                .get(column)
                .into_iter()
                .chain(rows.iter().filter_map(|row| row.get(column)))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(1);
            (max_chars as f32).sqrt().clamp(0.85, 2.5)
        })
        .collect::<Vec<_>>();

    let header = render_row(&headers, &weights, &alignments, true, theme);
    let body = rows
        .iter()
        .map(|row| render_row(row, &weights, &alignments, false, theme));
    div()
        .w_full()
        .min_w_0()
        .my(px(metrics::SPACE_2))
        .overflow_hidden()
        .rounded(px(metrics::PX_9))
        .border_1()
        .border_color(theme.hairline)
        .bg(theme.card)
        .flex()
        .flex_col()
        .child(header)
        .children(body)
        .into_any_element()
}

fn alignment(separator: &str) -> Alignment {
    let separator = separator.trim();
    match (separator.starts_with(':'), separator.ends_with(':')) {
        (true, true) => Alignment::Center,
        (false, true) => Alignment::Right,
        _ => Alignment::Left,
    }
}

fn render_row(
    cells: &[String],
    weights: &[f32],
    alignments: &[Alignment],
    is_header: bool,
    theme: &Theme,
) -> AnyElement {
    let mut row = div()
        .w_full()
        .min_w_0()
        .flex()
        .items_stretch()
        .when(is_header, |row| row.bg(theme.raised));
    for (column, cell) in cells.iter().enumerate() {
        let weight = weights.get(column).copied().unwrap_or(1.0);
        let align = alignments
            .get(column)
            .copied()
            .unwrap_or(Alignment::Left)
            .text_align();
        let mut cell_view = div()
            .flex_grow(weight)
            .flex_basis(relative(0.0))
            .min_w_0()
            .px(px(metrics::PX_10))
            .py(px(if is_header { 9.0 } else { 8.0 }))
            .text_size(px(typo::META))
            .text_align(align)
            .text_color(if is_header {
                theme.ink
            } else {
                theme.ink_secondary
            });
        if is_header {
            cell_view = cell_view.font_weight(FontWeight::SEMIBOLD);
        }
        row = row.child(cell_view.child(markdown_line(cell, theme)));
    }
    row.border_b_1()
        .border_color(theme.hairline)
        .into_any_element()
}
