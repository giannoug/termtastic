use crate::types::{Hotkey, Node, TelemetryItem, TracerouteItem};
use crate::ui::helpers::{
    Base64EncoderExt, ListStateExt, SnrColorExt, default_scrollbar, hops_to_spans, humanize_last_heard,
    humanize_uptime, last_heard_to_spans, short_name_to_span,
};
use crate::ui::widget::{PlaceholderWidget, PopupConfirmWidget, TabsWidget};
use chrono::Utc;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    prelude::*,
    text::ToSpan,
    widgets::{Block, BorderType, Padding, Paragraph},
};
use strum::{Display, EnumCount, EnumIter, FromRepr, IntoEnumIterator};
use tui_widget_list::{ListBuilder, ListState, ListView};

#[derive(Debug, Default, Clone, Copy, PartialEq, FromRepr, Display, EnumIter, EnumCount)]
enum NodeInfoTab {
    #[default]
    #[strum(to_string = "info")]
    Info,
    #[strum(to_string = "traceroutes")]
    Traceroutes,
    #[strum(to_string = "position")]
    Position,
    #[strum(to_string = "telemetry")]
    Telemetry,
}

impl NodeInfoTab {
    pub fn prev(self) -> Self {
        let current_index: usize = self as usize;
        let (previous_index, overflowed) = current_index.overflowing_sub(1);

        Self::from_repr(if overflowed {
            NodeInfoTab::COUNT - 1
        } else {
            previous_index
        })
        .unwrap_or(self)
    }

    pub fn next(self) -> Self {
        let current_index = self as usize;
        let next_index = current_index.saturating_add(1);

        Self::from_repr(if next_index > NodeInfoTab::COUNT - 1 {
            0
        } else {
            next_index
        })
        .unwrap_or(self)
    }
}

pub enum NodeInfoWidgetEvent {
    CloseRequested,
    CopyToClipboardRequested(String),
    NodeDeleteRequested,
    TracerouteRequested,
}

#[derive(Debug, Clone)]
pub struct NodeInfoContext<'a> {
    pub maybe_node: Option<&'a Node>,
    pub telemetry: &'a Vec<TelemetryItem>,
    pub traceroute: &'a Vec<TracerouteItem>,
    pub is_traceroute_pending: bool,
    pub uptime: Option<u32>,
    pub is_my_node: bool,
}

#[derive(Debug, Clone)]
pub struct NodeInfoWidgetState {
    active_tab: NodeInfoTab,
    telemetry_list_state: ListState,
    traceroute_list_state: ListState,
    is_delete_node_popup_visible: bool,
}

impl Default for NodeInfoWidgetState {
    fn default() -> Self {
        Self {
            active_tab: NodeInfoTab::default(),
            telemetry_list_state: ListState::default(),
            traceroute_list_state: ListState::default(),
            is_delete_node_popup_visible: false,
        }
    }
}

impl NodeInfoWidgetState {
    pub fn handle_event(
        &mut self,
        context: NodeInfoContext,
        event: Event,
        emit: &mut impl FnMut(NodeInfoWidgetEvent) -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
        if self.is_delete_node_popup_visible {
            match event {
                Event::Key(KeyEvent {
                    code,
                    kind: KeyEventKind::Press,
                    modifiers,
                    ..
                }) if modifiers.is_empty() => match code {
                    KeyCode::Enter => {
                        emit(NodeInfoWidgetEvent::NodeDeleteRequested)?;
                        self.is_delete_node_popup_visible = false;
                    }
                    KeyCode::Esc => {
                        self.is_delete_node_popup_visible = false;
                    }
                    _ => {}
                },
                _ => {}
            }

            return Ok(true);
        }

        if self.active_tab == NodeInfoTab::Telemetry && self.telemetry_list_state.handle_navigation_events(&event, None)
        {
            return Ok(true);
        }

        if self.active_tab == NodeInfoTab::Traceroutes
            && self.traceroute_list_state.handle_navigation_events(&event, None)
        {
            return Ok(true);
        }

        match event {
            Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Press,
                modifiers,
                ..
            }) => match (self.active_tab, code) {
                (_, KeyCode::Tab) if modifiers.is_empty() => {
                    self.active_tab = self.active_tab.next();
                    return Ok(true);
                }
                (_, KeyCode::BackTab) => {
                    self.active_tab = self.active_tab.prev();
                    return Ok(true);
                }
                (NodeInfoTab::Info, KeyCode::Char('k')) if modifiers.is_empty() => {
                    if let Some(user) = context.maybe_node.and_then(|n| n.user.as_ref()) {
                        emit(NodeInfoWidgetEvent::CopyToClipboardRequested(
                            user.public_key.base64_encode(),
                        ))?;
                        return Ok(true);
                    }
                }
                (NodeInfoTab::Info, KeyCode::Delete | KeyCode::Backspace) if modifiers.is_empty() => {
                    self.is_delete_node_popup_visible = true;
                    return Ok(true);
                }
                (NodeInfoTab::Telemetry, KeyCode::Char('c')) if modifiers.is_empty() => {
                    if let Some(item) = self
                        .telemetry_list_state
                        .selected
                        .and_then(|i| context.telemetry.get(i))
                    {
                        match item {
                            TelemetryItem::Group { json, .. } => {
                                emit(NodeInfoWidgetEvent::CopyToClipboardRequested(json.to_owned()))?;
                            }
                            TelemetryItem::Item { value: Some(v), .. } => {
                                emit(NodeInfoWidgetEvent::CopyToClipboardRequested(v.to_owned()))?;
                            }
                            _ => {}
                        };

                        return Ok(true);
                    }
                }
                (NodeInfoTab::Traceroutes, KeyCode::Char('r')) if modifiers.is_empty() => {
                    emit(NodeInfoWidgetEvent::TracerouteRequested)?;
                    return Ok(true);
                }
                (NodeInfoTab::Traceroutes, KeyCode::Char('c')) if modifiers.is_empty() => {
                    if let Some(TracerouteItem::Group { json, .. }) = self
                        .traceroute_list_state
                        .selected
                        .and_then(|i| context.traceroute.get(i))
                    {
                        emit(NodeInfoWidgetEvent::CopyToClipboardRequested(json.to_owned()))?;
                        return Ok(true);
                    }
                }
                (_, KeyCode::Esc) if modifiers.is_empty() => {
                    emit(NodeInfoWidgetEvent::CloseRequested)?;
                    return Ok(true);
                }
                _ => {}
            },
            _ => {}
        }

        Ok(false)
    }

    pub fn get_hotkeys(&self, is_my_node: bool) -> Vec<Hotkey> {
        match &self.active_tab {
            NodeInfoTab::Info => vec![
                Some(Hotkey::new("esc", "close")),
                Some(Hotkey::new("k", "copy public key")),
                (!is_my_node).then_some(Hotkey::new("delete", "remove")),
            ]
            .into_iter()
            .flatten()
            .collect(),
            NodeInfoTab::Telemetry => vec![Hotkey::new("esc", "close"), Hotkey::new("c", "copy")],
            NodeInfoTab::Traceroutes => vec![
                Hotkey::new("esc", "close"),
                Hotkey::new("r", "run traceroute"),
                Hotkey::new("c", "copy"),
            ],
            _ => vec![],
        }
    }
}

pub struct NodeInfoWidget<'a> {
    context: NodeInfoContext<'a>,
}

impl<'a> NodeInfoWidget<'a> {
    pub fn new(context: NodeInfoContext<'a>) -> Self {
        Self { context }
    }

    fn render_info(&self, node: &Node, area: Rect, buf: &mut Buffer, state: &mut NodeInfoWidgetState) {
        let v = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(area);

        // first line
        ThreeColumnInfoWidget {
            first: Some(InfoWidget::new("short name", node.short_name().to_span())),
            second: Some(InfoWidget::new("node number", node.key.to_span())),
            third: Some(InfoWidget::new("user ID", node.id().to_span())),
        }
        .render(v[0], buf);

        // second line
        ThreeColumnInfoWidget {
            first: Some(InfoWidget::new(
                "last heard",
                last_heard_to_spans(node, self.context.is_my_node),
            )),
            second: Some(InfoWidget::new("hops", hops_to_spans(node, self.context.is_my_node))),
            third: Some(InfoWidget::new(
                "uptime",
                self.context
                    .uptime
                    .and_then(|s| Some(Span::from(humanize_uptime(s))))
                    .unwrap_or(Span::from("no data").dark_gray()),
            )),
        }
        .render(v[1], buf);

        // third line
        ThreeColumnInfoWidget {
            first: Some(InfoWidget::new("device role", node.role().to_span())),
            second: Some(InfoWidget::new(
                "public key",
                if let Some(user) = node.user.as_ref() {
                    Span::from(format!("{}-byte", user.public_key.len())).green()
                } else {
                    "none".to_span().red()
                },
            )),
            third: Some(InfoWidget::new(
                "status",
                if node.user.is_none() {
                    Span::from("UNKNOWN").yellow()
                } else {
                    Span::from("STORED").green()
                },
            )),
        }
        .render(v[2], buf);

        // fourth line
        InfoWidget::new("hardware", node.hw_model().to_span().magenta()).render(v[3], buf);

        // delete popup
        if state.is_delete_node_popup_visible {
            PopupConfirmWidget::new(
                "This node will be removed from your list until your node receives data from it again.",
                "confirm",
                "cancel",
                40,
                Color::Red,
            )
            .render(area, buf);
        }
    }

    fn render_telemetry(
        &self,
        telemetry: &Vec<TelemetryItem>,
        area: Rect,
        buf: &mut Buffer,
        state: &mut NodeInfoWidgetState,
    ) {
        if telemetry.is_empty() {
            PlaceholderWidget::dark_gray("no telemetry collected yet").render(area, buf);
            return;
        };

        state.telemetry_list_state.fix_selection(telemetry.len());

        let list_builder = ListBuilder::new(|context| {
            let widget = TelemetryItemWidget {
                item: &telemetry[context.index],
                is_selected: context.is_selected,
            };

            (widget, 1)
        });

        let list = ListView::new(list_builder, telemetry.len())
            .infinite_scrolling(false)
            .scrollbar(default_scrollbar());

        list.render(area, buf, &mut state.telemetry_list_state);
    }

    fn render_traceroutes(
        &self,
        traceroute: &Vec<TracerouteItem>,
        is_pending: bool,
        area: Rect,
        buf: &mut Buffer,
        state: &mut NodeInfoWidgetState,
    ) {
        if traceroute.is_empty() {
            if is_pending {
                PlaceholderWidget::dark_gray("tracing route\u{2026}").render(area, buf);
            } else {
                PlaceholderWidget::dark_gray("press 'r' to run a traceroute").render(area, buf);
            }
            return;
        };

        let v = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(area);

        state.traceroute_list_state.fix_selection(traceroute.len());

        let list_builder = ListBuilder::new(|context| {
            let widget = TracerouteItemWidget {
                item: &traceroute[context.index],
                is_selected: context.is_selected,
            };

            (widget, 1)
        });

        let list = ListView::new(list_builder, traceroute.len())
            .infinite_scrolling(false)
            .scrollbar(default_scrollbar());

        list.render(v[0], buf, &mut state.traceroute_list_state);

        if is_pending {
            Line::from(Span::from("tracing route\u{2026}").dark_gray()).render(v[1], buf);
        }
    }
}

impl<'a> StatefulWidget for NodeInfoWidget<'a> {
    type State = NodeInfoWidgetState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let title = match self.context.maybe_node {
            Some(node) => Line::from(vec![
                Span::from(" "),
                short_name_to_span(node, self.context.is_my_node),
                Span::from(" "),
                Span::from(node.long_name()).bold(),
                Span::from(" "),
            ]),
            None => Line::from(Span::from("Node not found").dark_gray()),
        };

        let block = Block::bordered()
            .border_type(BorderType::Thick)
            .padding(Padding::symmetric(2, 1))
            .title(title);

        let block_area = block.inner(area);
        block.render(area, buf);

        let Some(node) = self.context.maybe_node else {
            PlaceholderWidget::dark_gray("node not found").render(block_area, buf);
            return;
        };

        let v = Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Fill(1)]).split(block_area);

        // tabs
        TabsWidget::new(
            NodeInfoTab::iter().map(|t| (t as usize, t.to_string())).collect(),
            state.active_tab as usize,
        )
        .render(v[0], buf);

        match &state.active_tab {
            NodeInfoTab::Info => self.render_info(node, v[2], buf, state),
            NodeInfoTab::Telemetry => self.render_telemetry(self.context.telemetry, v[2], buf, state),
            NodeInfoTab::Traceroutes => self.render_traceroutes(
                self.context.traceroute,
                self.context.is_traceroute_pending,
                v[2],
                buf,
                state,
            ),
            _ => PlaceholderWidget::red("not implemented").render(v[2], buf),
        }
    }
}

#[derive(Clone)]
struct ThreeColumnInfoWidget<'a> {
    first: Option<InfoWidget<'a>>,
    second: Option<InfoWidget<'a>>,
    third: Option<InfoWidget<'a>>,
}

impl<'a> Widget for ThreeColumnInfoWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let h = Layout::horizontal([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(area);

        if let Some(first) = self.first {
            first.render(h[0], buf);
        }

        if let Some(second) = self.second {
            second.render(h[1], buf);
        }

        if let Some(third) = self.third {
            third.render(h[2], buf);
        }
    }
}

#[derive(Clone)]
struct InfoWidget<'a> {
    pub title: &'a str,
    pub value: Line<'a>,
}

impl<'a> InfoWidget<'a> {
    pub fn new(title: &'a str, value: impl Into<Line<'a>>) -> Self {
        Self {
            title,
            value: value.into(),
        }
    }
}

impl<'a> Widget for InfoWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        Paragraph::new(vec![Line::from(Span::from(self.title).dark_gray()), self.value]).render(area, buf);
    }
}

struct TelemetryItemWidget<'a> {
    item: &'a TelemetryItem,
    is_selected: bool,
}

impl<'a> Widget for TelemetryItemWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.item {
            TelemetryItem::Group { title, datetime, .. } => {
                let h =
                    Layout::horizontal([Constraint::Fill(3), Constraint::Fill(2), Constraint::Length(2)]).split(area);

                Line::from(vec![Span::from(title).bold().add_modifier(if self.is_selected {
                    Modifier::UNDERLINED
                } else {
                    Modifier::empty()
                })])
                .magenta()
                .render(h[0], buf);

                Line::from(humanize_last_heard(Utc::now().signed_duration_since(datetime)))
                    .right_aligned()
                    .render(h[1], buf);
            }
            TelemetryItem::Item {
                title, formatted_value, ..
            } => {
                let v = Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).split(area);

                Line::from(vec![
                    Span::from("  "),
                    Span::from(format!("{}:", title)).add_modifier(if self.is_selected {
                        Modifier::UNDERLINED | Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ])
                .render(v[0], buf);

                formatted_value
                    .as_ref()
                    .and_then(|v| Some(Span::from(v)))
                    .unwrap_or(Span::from("no data").dark_gray())
                    .add_modifier(if self.is_selected {
                        Modifier::UNDERLINED | Modifier::BOLD
                    } else {
                        Modifier::empty()
                    })
                    .render(v[1], buf);
            }
        }
    }
}

struct TracerouteItemWidget<'a> {
    item: &'a TracerouteItem,
    is_selected: bool,
}

impl<'a> Widget for TracerouteItemWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.item {
            TracerouteItem::Group { title, datetime, .. } => {
                let h =
                    Layout::horizontal([Constraint::Fill(3), Constraint::Fill(2), Constraint::Length(2)]).split(area);

                Line::from(vec![Span::from(title).bold().add_modifier(if self.is_selected {
                    Modifier::UNDERLINED
                } else {
                    Modifier::empty()
                })])
                .magenta()
                .render(h[0], buf);

                Line::from(humanize_last_heard(Utc::now().signed_duration_since(datetime)))
                    .right_aligned()
                    .render(h[1], buf);
            }
            TracerouteItem::Hop { title, snr } => {
                let v = Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).split(area);

                Line::from(vec![
                    Span::from("  "),
                    Span::from(title).add_modifier(if self.is_selected {
                        Modifier::UNDERLINED | Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ])
                .render(v[0], buf);

                let snr_span = match snr {
                    Some(snr) => Span::from(format!("{:.2} dB", snr)).fg(snr.snr_to_color()),
                    None => Span::from("no data").dark_gray(),
                };

                Line::from(snr_span.add_modifier(if self.is_selected {
                    Modifier::UNDERLINED | Modifier::BOLD
                } else {
                    Modifier::empty()
                }))
                .right_aligned()
                .render(v[1], buf);
            }
        }
    }
}
