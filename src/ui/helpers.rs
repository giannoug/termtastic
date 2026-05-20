use crate::state::State;
use crate::types::{Channel, ChannelRole, HopsSnrRssiAware, Node};
use base64::prelude::BASE64_STANDARD;
use base64::{DecodeError, Engine};
use chrono::{SubsecRound, TimeDelta, Utc};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind};
use itertools::Itertools;
use meshtastic::protobufs::config;
use meshtastic::protobufs::routing;
use ratatui::{
    style::{Color, Style, Stylize},
    symbols::scrollbar::Set as ScrollbarSet,
    text::{Line, Span},
    widgets::{Scrollbar, ScrollbarOrientation},
};
use regex::{Regex, RegexBuilder};
use std::sync::LazyLock;
use tui_widget_list::ListState;

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

impl ModemPresetExt for config::lo_ra_config::ModemPreset {
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

pub fn channel_name_to_spans<'a>(channel: &'a Channel, state: &'a State) -> Vec<Span<'a>> {
    let maybe_direct_node = (channel.role == ChannelRole::Direct)
        .then_some(state.nodes.get(&channel.key))
        .unwrap_or(None);

    let radio_preset_name = state
        .device_config
        .lora
        .as_ref()
        .and_then(|lora| config::lo_ra_config::ModemPreset::try_from(lora.modem_preset).ok())
        .and_then(|preset| Some(preset.as_channel_name()));

    match (&channel.role, &maybe_direct_node) {
        (ChannelRole::Primary, _) => vec![
            Span::from(format!("#{}", &channel.key)).dark_gray(),
            Span::from(" "),
            Span::from(if !channel.name.is_empty() {
                channel.name.clone()
            } else if let Some(preset_name) = radio_preset_name {
                preset_name.clone()
            } else {
                "Primary".to_owned()
            })
            .fg(channel.psk.len().psk_len_to_color()),
        ],
        (ChannelRole::Secondary, _) => vec![
            Span::from(format!("#{}", channel.key)).dark_gray(),
            Span::from(" "),
            Span::from(if !channel.name.is_empty() {
                &channel.name
            } else {
                "Secondary"
            })
            .fg(channel.psk.len().psk_len_to_color()),
        ],
        (ChannelRole::Direct, Some(node)) => {
            vec![
                short_name_to_span(node, state.my_node_key == Some(node.key)),
                Span::from(" "),
                Span::from(node.long_name()),
            ]
        }
        (ChannelRole::Direct, None) => {
            vec![Span::from(format!("!{:x}", channel.key))]
        }
        (ChannelRole::Disabled, _) => {
            vec![Span::from("Disabled")]
        }
    }
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

pub fn hops_to_spans<'a>(provider: &impl HopsSnrRssiAware, my: bool) -> Vec<Span<'a>> {
    if my {
        return vec![Span::from("my node").blue()];
    }

    match (provider.hops(), provider.rssi()) {
        (Some(0), None) => {
            vec![Span::from(format!("{:.2}dB", provider.snr())).style(Style::new().fg(provider.snr().snr_to_color()))]
        }
        (Some(0), Some(rssi)) => vec![
            Span::from(format!("{:.2}dB", provider.snr())).style(Style::new().fg(provider.snr().snr_to_color())),
            Span::from("/").dark_gray(),
            Span::from(format!("{}dbm", rssi)).fg(rssi.rssi_to_color()),
        ],
        (Some(1), _) => vec![Span::from("1 hop")],
        (Some(hops), _) => vec![Span::from(format!("{} hops", hops))],
        _ => vec![Span::from("unknown").dark_gray()],
    }
}

pub fn short_name_to_span(node: &Node, my: bool) -> Span<'_> {
    Span::from(pad_center(&node.short_name(), 6))
        .black()
        .patch_style(if my {
            Style::new().white().on_blue()
        } else if node.user.is_none() {
            Style::new().on_yellow()
        } else if node.id() == "?" {
            Style::new().on_red()
        } else {
            Style::new().on_green()
        })
}

pub fn last_heard_to_spans(node: &Node, my: bool) -> Vec<Span<'_>> {
    match node.last_heard {
        Some(_) if my => vec![Span::from("now").blue()],
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

pub trait ListStateExt {
    fn handle_navigation_events(&mut self, event: &Event, items_count: usize) -> bool;

    fn fix_selection(&mut self, items_count: usize);
}

impl ListStateExt for ListState {
    fn handle_navigation_events(&mut self, event: &Event, items_count: usize) -> bool {
        match event {
            Event::Key(KeyEvent { code, kind, .. }) if kind == &KeyEventKind::Press => match code {
                KeyCode::Home => {
                    self.select(Some(0));
                    return true;
                }
                KeyCode::End => {
                    self.select(Some(items_count - 1));
                    return true;
                }
                KeyCode::Up => {
                    self.previous();
                    return true;
                }
                KeyCode::Down => {
                    self.next();
                    return true;
                }
                _ => {}
            },
            Event::Mouse(MouseEvent { kind, .. }) => match kind {
                MouseEventKind::ScrollUp => {
                    self.previous();
                    return true;
                }
                MouseEventKind::ScrollDown => {
                    self.next();
                    return true;
                }
                _ => {}
            },
            _ => {}
        }

        false
    }

    fn fix_selection(&mut self, items_count: usize) {
        if self.selected.and_then(|i| Some(i >= items_count)).unwrap_or(false) {
            self.selected = None;
        }

        if self.selected.is_none() && items_count > 0 {
            self.select(Some(0));
        }
    }
}
