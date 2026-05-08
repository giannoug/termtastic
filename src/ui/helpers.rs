use chrono::TimeDelta;
use ratatui::{
    style::{Color, Style, Stylize},
    symbols::scrollbar::Set as ScrollbarSet,
    text::{Line, Span},
    widgets::{Scrollbar, ScrollbarOrientation},
};
use regex::{Regex, RegexBuilder};
use std::sync::LazyLock;

static LINK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"[^:/?#\s]+://[^\s]+(?:[\s,.!?;:)\]}>]|$)")
        .multi_line(true)
        .unicode(true)
        .build()
        .unwrap()
});

pub trait ColorExt {
    fn snr_to_color(&self) -> Color;
}

impl ColorExt for f32 {
    fn snr_to_color(&self) -> Color {
        match self {
            ..=-10.0 => Color::Red,
            -10.0..=-7.0 => Color::Yellow,
            -7.0.. => Color::Green,
            _ => Color::DarkGray,
        }
    }
}

#[allow(dead_code)]
pub trait LinkExt {
    fn str_to_hyperlinked_lines(value: &str) -> Vec<Line<'_>>;
}

#[allow(dead_code)]
pub fn str_to_hyperlinked_lines(value: &str) -> Vec<Line<'_>> {
    let mut result = Vec::new();

    for line in value.split('\n') {
        let mut spans = Vec::new();
        let mut last_end = 0;

        for mat in LINK_REGEX.find_iter(line) {
            let start = mat.start();
            let end = mat.end();

            if start > last_end {
                spans.push(Span::raw(&line[last_end..start]).style(Style::new()));
            }

            spans.push(Span::from(&line[start..end]).underlined().magenta());
            last_end = end;
        }

        if last_end < line.len() {
            spans.push(Span::from(&line[last_end..]));
        }

        result.push(Line::from(spans));
    }

    result
}

pub fn default_scrollbar() -> Scrollbar<'static> {
    Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .symbols(ScrollbarSet {
            begin: "┬",
            thumb: "█",
            track: "│",
            end: "┴",
        })
        .style(Style::new().dark_gray())
}

pub fn pad_center(s: &str, width: usize) -> String {
    let w = unicode_width::UnicodeWidthStr::width(s);
    if w >= width {
        return s.to_string();
    }

    let padding = width - w;
    let left = padding / 2;
    let right = padding - left;

    format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
}

pub fn humanize_duration<'a>(d: TimeDelta) -> Vec<Span<'a>> {
    if d.num_seconds() < 60 {
        return vec![Span::from("now").green()];
    }

    if d.num_minutes() < 60 {
        return vec![
            Span::from(format!("{}m", d.num_minutes())),
            Span::from(" ago").dark_gray(),
        ];
    }

    if d.num_hours() < 24 {
        let remaining_minutes = d.num_minutes() % 60;

        return vec![
            Span::from(format!("{}h {}m", d.num_hours(), remaining_minutes)),
            Span::from(" ago").dark_gray(),
        ];
    }

    let remaining_hours = d.num_hours() % 24;

    vec![
        Span::from(format!("{}d {}h", d.num_days(), remaining_hours)),
        Span::from(" ago").dark_gray(),
    ]
}
