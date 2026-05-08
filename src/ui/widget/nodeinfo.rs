use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span, ToSpan};
use ratatui::widgets::Paragraph;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, BorderType, Padding, StatefulWidget, Widget},
};
use tui_widget_list::ListState;

use crate::types::Node;
use crate::ui::prelude::{Constraint, Layout, PlaceholderWidget};

pub struct NodeInfoState {
    list_state: ListState,
}

impl NodeInfoState {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
        }
    }

    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(KeyEvent { code, kind, .. }) if kind == KeyEventKind::Press => match code {
                KeyCode::Up => {
                    self.list_state.previous();
                }
                KeyCode::Down => {
                    self.list_state.next();
                }
                _ => {}
            },
            Event::Mouse(MouseEvent { kind, .. }) => match kind {
                MouseEventKind::ScrollUp => {
                    self.list_state.previous();
                }
                MouseEventKind::ScrollDown => {
                    self.list_state.next();
                }
                _ => {}
            },
            _ => {}
        }
    }
}

pub struct NodeInfoWidget<'a> {
    node: Option<&'a Node>,
}

impl<'a> NodeInfoWidget<'a> {
    pub fn new(node: Option<&'a Node>) -> Self {
        Self { node }
    }
}

impl<'a> StatefulWidget for NodeInfoWidget<'a> {
    type State = NodeInfoState;

    fn render(self, area: Rect, buf: &mut Buffer, _state: &mut Self::State) {
        let block = Block::bordered()
            .border_type(BorderType::Thick)
            .padding(Padding::symmetric(2, 1))
            .title(format!(
                " {} ",
                self.node
                    .and_then(|n| Some(n.long_name()))
                    .unwrap_or("Node Info".to_owned()),
            ));

        let block_area = block.inner(area);
        block.render(area, buf);

        let Some(node) = self.node else {
            PlaceholderWidget::dark_gray("node not found").render(block_area, buf);
            return;
        };

        let v = Layout::vertical(Constraint::from_lengths([2, 1, 2, 1, 2, 1, 2])).split(block_area);
        let v0_h = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1), Constraint::Fill(1)]).split(v[0]);
        let v2_h = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1), Constraint::Fill(1)]).split(v[2]);
        let v4_h = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1), Constraint::Fill(1)]).split(v[4]);

        // first line
        Paragraph::new(vec![
            Line::from(Span::from("Short Name").dark_gray()),
            Line::from(node.short_name_to_span()),
        ])
        .render(v0_h[0], buf);

        Paragraph::new(vec![
            Line::from(Span::from("Node Number").dark_gray()),
            Line::from(node.key.to_span()),
        ])
        .render(v0_h[1], buf);

        Paragraph::new(vec![
            Line::from(Span::from("User ID").dark_gray()),
            Line::from(node.id.to_span()),
        ])
        .render(v0_h[2], buf);

        // second line
        Paragraph::new(vec![
            Line::from(Span::from("Last Heard").dark_gray()),
            Line::from(node.last_heard_to_spans()),
        ])
        .render(v2_h[0], buf);

        Paragraph::new(vec![
            Line::from(Span::from("Hops").dark_gray()),
            Line::from(node.hops_to_spans()),
        ])
        .render(v2_h[1], buf);

        Paragraph::new(vec![
            Line::from(Span::from("Uptime").dark_gray()),
            Line::from("Unknown".to_span()),
        ])
        .render(v2_h[2], buf);

        // third line
        Paragraph::new(vec![
            Line::from(Span::from("Device Role").dark_gray()),
            Line::from(node.role().to_span()),
        ])
        .render(v4_h[0], buf);

        Paragraph::new(vec![
            Line::from(Span::from("Public Key").dark_gray()),
            Line::from(format!("{}-byte", node.public_key.len())),
        ])
        .render(v4_h[1], buf);

        Paragraph::new(vec![
            Line::from(Span::from("Hardware").dark_gray()),
            Line::from(node.hw_model().to_span().magenta()),
        ])
        .render(v4_h[2], buf);

        Line::from("─".repeat(v[5].width as usize).to_span().dark_gray()).render(v[5], buf);
    }
}
