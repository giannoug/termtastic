use crate::types::{Hotkey, Node, NodeLastTelemetry, NodeTelemetry};
use crate::ui::helpers::{hops_to_spans, humanize_uptime, last_heard_to_spans};
use crate::ui::prelude::{Constraint, Layout, PlaceholderWidget, PopupConfirmWidget, TabsWidget};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    prelude::*,
    text::ToSpan,
    widgets::{Block, BorderType, Padding, Paragraph, StatefulWidget, Widget},
};
use strum::{Display, EnumCount, EnumIter, FromRepr, IntoEnumIterator};

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
    PopupCloseRequested,
    PublicKeyCopyRequested,
    NodeDeleteRequested,
}

#[derive(Debug, Clone)]
pub struct NodeInfoWidgetState {
    active_tab: NodeInfoTab,
    is_delete_node_popup_visible: bool,
}

impl Default for NodeInfoWidgetState {
    fn default() -> Self {
        Self {
            active_tab: NodeInfoTab::default(),
            is_delete_node_popup_visible: false,
        }
    }
}

impl NodeInfoWidgetState {
    pub fn handle_event(
        &mut self,
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
                    emit(NodeInfoWidgetEvent::PublicKeyCopyRequested)?;
                    return Ok(true);
                }
                (NodeInfoTab::Info, KeyCode::Delete | KeyCode::Backspace) if modifiers.is_empty() => {
                    self.is_delete_node_popup_visible = true;
                    return Ok(true);
                }
                (_, KeyCode::Esc) if modifiers.is_empty() => {
                    emit(NodeInfoWidgetEvent::PopupCloseRequested)?;
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
            _ => vec![],
        }
    }
}

pub struct NodeInfoWidget<'a> {
    maybe_node: Option<&'a Node>,
    last_telemetry: Option<&'a NodeLastTelemetry>,
    telemetry: &'a Vec<NodeTelemetry>,
    is_my_node: bool,
}

impl<'a> NodeInfoWidget<'a> {
    pub fn new(
        maybe_node: Option<&'a Node>,
        last_telemetry: Option<&'a NodeLastTelemetry>,
        telemetry: &'a Vec<NodeTelemetry>,
        is_my_node: bool,
    ) -> Self {
        Self {
            maybe_node,
            last_telemetry,
            telemetry,
            is_my_node,
        }
    }

    fn render_info(&self, node: &Node, area: Rect, buf: &mut Buffer, state: &mut NodeInfoWidgetState) {
        let v = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(area);

        let v0_h = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1), Constraint::Fill(1)]).split(v[0]);
        let v1_h = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1), Constraint::Fill(1)]).split(v[1]);
        let v2_h = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1), Constraint::Fill(1)]).split(v[2]);

        // first line
        Paragraph::new(vec![
            Line::from(Span::from("short name").dark_gray()),
            Line::from(node.short_name().to_span()),
        ])
        .render(v0_h[0], buf);

        Paragraph::new(vec![
            Line::from(Span::from("node number").dark_gray()),
            Line::from(node.key.to_span()),
        ])
        .render(v0_h[1], buf);

        Paragraph::new(vec![
            Line::from(Span::from("user ID").dark_gray()),
            Line::from(node.id().to_span()),
        ])
        .render(v0_h[2], buf);

        // second line
        Paragraph::new(vec![
            Line::from(Span::from("last heard").dark_gray()),
            Line::from(last_heard_to_spans(node, self.is_my_node)),
        ])
        .render(v1_h[0], buf);

        Paragraph::new(vec![
            Line::from(Span::from("hops").dark_gray()),
            Line::from(hops_to_spans(node, self.is_my_node)),
        ])
        .render(v1_h[1], buf);

        Paragraph::new(vec![
            Line::from(Span::from("uptime").dark_gray()),
            Line::from(
                self.last_telemetry
                    .and_then(|t| t.device_metrics)
                    .and_then(|m| m.uptime_seconds)
                    .and_then(|s| Some(humanize_uptime(s)))
                    .unwrap_or(Span::from("no data")),
            ),
        ])
        .render(v1_h[2], buf);

        // third line
        Paragraph::new(vec![
            Line::from(Span::from("device role").dark_gray()),
            Line::from(node.role().to_span()),
        ])
        .render(v2_h[0], buf);

        Paragraph::new(vec![
            Line::from(Span::from("public key").dark_gray()),
            Line::from(if let Some(user) = node.user.as_ref() {
                Span::from(format!("{}-byte", user.public_key.len())).green()
            } else {
                "none".to_span().red()
            }),
        ])
        .render(v2_h[1], buf);

        Paragraph::new(vec![
            Line::from(Span::from("status").dark_gray()),
            Line::from(if node.user.is_none() {
                Span::from("UNKNOWN").yellow()
            } else {
                Span::from("STORED").green()
            }),
        ])
        .render(v2_h[2], buf);

        // fourth line
        Paragraph::new(vec![
            Line::from(Span::from("hardware").dark_gray()),
            Line::from(node.hw_model().to_span().magenta()),
        ])
        .render(v[3], buf);

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
        telemetry: &Vec<NodeTelemetry>,
        area: Rect,
        buf: &mut Buffer,
        _state: &mut NodeInfoWidgetState,
    ) {
        Span::from(telemetry.len().to_string()).render(area, buf);
    }
}

impl<'a> StatefulWidget for NodeInfoWidget<'a> {
    type State = NodeInfoWidgetState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let block = Block::bordered()
            .border_type(BorderType::Thick)
            .padding(Padding::symmetric(2, 1))
            .title(Line::from(vec![
                Span::from(" "),
                Span::from(
                    self.maybe_node
                        .as_ref()
                        .and_then(|n| Some(n.long_name()))
                        .unwrap_or("UNKNOWN".to_owned()),
                )
                .bold(),
                Span::from(" "),
            ]));

        let block_area = block.inner(area);
        block.render(area, buf);

        let Some(node) = self.maybe_node else {
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
            NodeInfoTab::Telemetry => self.render_telemetry(self.telemetry, v[2], buf, state),
            _ => PlaceholderWidget::red("not implemented").render(v[2], buf),
        }
    }
}
