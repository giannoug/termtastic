use crate::types::{HopsSnrRssiAware, Node};
use crate::ui::prelude::text;
use base64::prelude::BASE64_STANDARD;
use base64::{DecodeError, Engine};
use chrono::{SubsecRound, TimeDelta, Utc};
use itertools::Itertools;
use meshtastic::protobufs::routing;
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

pub trait SnrColorExt {
    fn snr_to_color(&self) -> Color;
}

impl SnrColorExt for f32 {
    fn snr_to_color(&self) -> Color {
        match self {
            ..=-10.0 => Color::Red,
            -10.0..=-7.0 => Color::Yellow,
            -7.0.. => Color::Green,
            _ => Color::DarkGray,
        }
    }
}

pub trait RssiColorExt {
    fn rssi_to_color(&self) -> Color;
}

impl RssiColorExt for i32 {
    fn rssi_to_color(&self) -> Color {
        match self {
            ..-100 => Color::Red,
            -100..-85 => Color::Yellow,
            -85.. => Color::Green,
        }
    }
}

pub trait PskLenColorExt {
    fn psk_len_to_color(&self) -> Color;
}

impl PskLenColorExt for usize {
    fn psk_len_to_color(&self) -> Color {
        match self {
            0 => Color::Red,
            1 => Color::Yellow,
            _ => Color::Green,
        }
    }
}

pub trait Base64EncoderExt {
    fn base64_encode(&self) -> String;
}

impl Base64EncoderExt for Vec<u8> {
    fn base64_encode(&self) -> String {
        BASE64_STANDARD.encode(&self)
    }
}

pub trait Base64DecoderExt {
    fn base64_decode(&self) -> Result<Vec<u8>, DecodeError>;
}

impl Base64DecoderExt for &str {
    fn base64_decode(&self) -> Result<Vec<u8>, DecodeError> {
        BASE64_STANDARD.decode(self)
    }
}

impl Base64DecoderExt for String {
    fn base64_decode(&self) -> Result<Vec<u8>, DecodeError> {
        BASE64_STANDARD.decode(self)
    }
}

pub trait ModemPresetExt {
    fn as_channel_name(&self) -> String;
}

impl ModemPresetExt for meshtastic::protobufs::config::lo_ra_config::ModemPreset {
    fn as_channel_name(&self) -> String {
        self.as_str_name()
            .to_lowercase()
            .split('_')
            .map(|w| {
                let mut chars = w.chars();

                match chars.next() {
                    None => String::new(),
                    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .join("")
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

pub fn humanize_duration<'a>(delta: TimeDelta) -> Vec<Span<'a>> {
    if delta.num_seconds() < 60 {
        return vec![Span::from("now").green()];
    }

    if delta.num_minutes() < 60 {
        return vec![
            Span::from(format!("{}m", delta.num_minutes())),
            Span::from(" ago").dark_gray(),
        ];
    }

    if delta.num_hours() < 48 {
        return vec![
            Span::from(format!("{}h", delta.num_hours())),
            Span::from(" ago").dark_gray(),
        ];
    }

    vec![
        Span::from(format!("{}d", delta.num_days())),
        Span::from(" ago").dark_gray(),
    ]
}

pub fn hops_to_spans<'a>(provider: &impl HopsSnrRssiAware) -> Vec<Span<'a>> {
    match provider.hops() {
        _ if provider.my() => vec![Span::from("connected").blue()],
        Some(0) => vec![
            Some(Span::from(format!("{:.2}dB", provider.snr())).style(Style::new().fg(provider.snr().snr_to_color()))),
            provider
                .rssi()
                .and_then(|rssi| Some(Span::from(format!(" RSSI {}", rssi)).fg(rssi.rssi_to_color()))),
        ]
        .iter()
        .flatten()
        .cloned()
        .collect(),
        Some(1) => vec![Span::from("1 hop")],
        Some(hops) => vec![Span::from(format!("{} hops", hops))],
        None => vec![Span::from("unknown").dark_gray()],
    }
}

pub fn short_name_to_span(node: &Node) -> text::Span<'_> {
    Span::from(pad_center(&node.short_name(), 6))
        .black()
        .patch_style(if node.my {
            Style::new().white().on_blue()
        } else if node.user.is_none() {
            Style::new().on_yellow()
        } else if node.id == "?" {
            Style::new().on_red()
        } else {
            Style::new().on_green()
        })
}

pub fn last_heard_to_spans(node: &Node) -> Vec<Span<'_>> {
    match node.last_heard {
        Some(_) if node.my => vec![Span::from("now").blue()],
        Some(dt) => humanize_duration(Utc::now().round_subsecs(0) - dt),
        None => vec![Span::from("?").dark_gray()],
    }
}

pub fn routing_error_to_span<'a>(error: Option<routing::Error>) -> Span<'a> {
    match error {
        Some(routing::Error::None) => Span::from("acked").green(),
        Some(e) => Span::from(e.as_str_name()).red(),
        None => Span::from("sent").dark_gray(),
    }
}
