use crate::types::{Hotkey, Node};
use crate::ui::helpers::{hops_to_spans, last_heard_to_spans};
use crate::ui::prelude::{Constraint, Layout, PlaceholderWidget, PopupConfirmWidget, TabsWidget};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    prelude::*,
    text::ToSpan,
    widgets::{Block, BorderType, Padding, Paragraph, StatefulWidget, Widget},
};
use strum::{Display, EnumCount, EnumIter, FromRepr, IntoEnumIterator};

#[derive(Debug, Default, Clone, Copy, PartialEq, FromRepr, Display, EnumIter, EnumCount)]
enum Tab {
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

impl Tab {
    pub fn prev(self) -> Self {
        let current_index: usize = self as usize;
        let (previous_index, overflowed) = current_index.overflowing_sub(1);

        Self::from_repr(if overflowed { Tab::COUNT - 1 } else { previous_index }).unwrap_or(self)
    }

    pub fn next(self) -> Self {
        let current_index = self as usize;
        let next_index = current_index.saturating_add(1);

        Self::from_repr(if next_index > Tab::COUNT - 1 { 0 } else { next_index }).unwrap_or(self)
    }
}

pub enum NodeInfoWidgetEvent {
    PopupCloseRequested,
    PublicKeyCopyRequested,
    NodeDeleteRequested,
}

#[derive(Debug, Clone)]
pub struct NodeInfoState {
    active_tab: Tab,
    is_delete_node_popup_visible: bool,
}

impl Default for NodeInfoState {
    fn default() -> Self {
        Self {
            active_tab: Tab::default(),
            is_delete_node_popup_visible: false,
        }
    }
}

impl NodeInfoState {
    pub fn handle_event(
        &mut self,
        event: Event,
        emit: &mut impl FnMut(NodeInfoWidgetEvent) -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
        match event {
            Event::Key(KeyEvent { code, kind, .. }) if kind == KeyEventKind::Press => match (self.active_tab, code) {
                (_, KeyCode::Tab) => {
                    self.active_tab = self.active_tab.next();
                    return Ok(true);
                }
                (_, KeyCode::BackTab) => {
                    self.active_tab = self.active_tab.prev();
                    return Ok(true);
                }
                (Tab::Info, KeyCode::Char('k')) => {
                    emit(NodeInfoWidgetEvent::PublicKeyCopyRequested)?;
                    return Ok(true);
                }
                (Tab::Info, KeyCode::Delete | KeyCode::Backspace) => {
                    self.is_delete_node_popup_visible = true;
                    return Ok(true);
                }
                (Tab::Info, KeyCode::Esc) if self.is_delete_node_popup_visible => {
                    self.is_delete_node_popup_visible = false;
                    return Ok(true);
                }
                (Tab::Info, KeyCode::Enter) if self.is_delete_node_popup_visible => {
                    emit(NodeInfoWidgetEvent::NodeDeleteRequested)?;
                    self.is_delete_node_popup_visible = false;
                    return Ok(true);
                }
                (_, KeyCode::Esc) => {
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
            Tab::Info => vec![
                Some(Hotkey::new("esc", "close")),
                Some(Hotkey::new("k", "copy public key")),
                is_my_node.then_some(Hotkey::new("del", "remove")),
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
    is_my_node: bool,
}

impl<'a> NodeInfoWidget<'a> {
    pub fn new(maybe_node: Option<&'a Node>, is_my_node: bool) -> Self {
        Self { maybe_node, is_my_node }
    }

    fn render_info(&self, node: &Node, area: Rect, buf: &mut Buffer, state: &mut NodeInfoState) {
        let v = Layout::vertical([Constraint::Length(9), Constraint::Length(2)]).split(area);
        let v0_h = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1), Constraint::Fill(1)]).split(v[0]);

        // first column
        Paragraph::new(vec![
            Line::from(Span::from("short name").dark_gray()),
            Line::from(node.short_name().to_span()),
            Line::default(),
            Line::from(Span::from("last heard").dark_gray()),
            Line::from(last_heard_to_spans(node, self.is_my_node)),
            Line::default(),
            Line::from(Span::from("device role").dark_gray()),
            Line::from(node.role().to_span()),
        ])
        .render(v0_h[0], buf);

        // second column
        Paragraph::new(vec![
            Line::from(Span::from("node number").dark_gray()),
            Line::from(node.key.to_span()),
            Line::default(),
            Line::from(Span::from("hops").dark_gray()),
            Line::from(hops_to_spans(node, self.is_my_node)),
            Line::default(),
            Line::from(Span::from("public key").dark_gray()),
            Line::from(if !node.public_key.is_empty() {
                Span::from(format!("{}-byte", node.public_key.len())).green()
            } else {
                "none".to_span().red()
            }),
        ])
        .render(v0_h[1], buf);

        // third column
        Paragraph::new(vec![
            Line::from(Span::from("user ID").dark_gray()),
            Line::from(node.id.to_span()),
            Line::default(),
            Line::from(Span::from("uptime").dark_gray()),
            Line::from("no data".to_span()),
            Line::default(),
            Line::from(Span::from("status").dark_gray()),
            Line::from(if node.user.is_none() {
                Span::from("UNKNOWN").yellow()
            } else {
                Span::from("STORED").green()
            }),
        ])
        .render(v0_h[2], buf);

        // long line
        Paragraph::new(vec![
            Line::from(Span::from("hardware").dark_gray()),
            Line::from(node.hw_model().to_span().magenta()),
        ])
        .render(v[1], buf);

        // delete popup
        if state.is_delete_node_popup_visible {
            PopupConfirmWidget::new(
                "This node will be removed from your list until your node receives data from it again.",
                "confirm",
                "cancel",
                40,
                Color::Red,
            )
            .render(v[0], buf);
        }
    }
}

impl<'a> StatefulWidget for NodeInfoWidget<'a> {
    type State = NodeInfoState;

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
            Tab::iter().map(|t| (t as usize, t.to_string())).collect(),
            state.active_tab as usize,
        )
        .render(v[0], buf);

        match &state.active_tab {
            Tab::Info => self.render_info(node, v[2], buf, state),
            _ => PlaceholderWidget::red("not implemented").render(v[2], buf),
        }
    }
}
