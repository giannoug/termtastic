use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span, ToSpan};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, BorderType, Padding, StatefulWidget, Widget},
};
use tui_widget_list::{ListBuilder, ListState, ListView};

use crate::types::{MessageReaction, Node};
use crate::ui::helpers::default_scrollbar;
use crate::ui::prelude::{Borders, Constraint, Layout, PlaceholderWidget};

pub struct ReactionsViewerState {
    list_state: ListState,
}

impl ReactionsViewerState {
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

pub struct ReactionsViewerItem<'a> {
    pub reaction: &'a MessageReaction,
    pub node: &'a Node,
}

pub struct ReactionsViewerWidget<'a> {
    reactions: Vec<ReactionsViewerItem<'a>>,
}

impl<'a> ReactionsViewerWidget<'a> {
    pub fn new(reactions: Vec<ReactionsViewerItem<'a>>) -> Self {
        Self { reactions }
    }
}

impl<'a> StatefulWidget for ReactionsViewerWidget<'a> {
    type State = ReactionsViewerState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let block = Block::bordered()
            .border_type(BorderType::Thick)
            .padding(Padding::new(0, 1, 1, 0))
            .title(format!(" reactions: {} ", self.reactions.len()));

        let block_area = block.inner(area);
        block.render(area, buf);

        // placeholder if no reactions
        if self.reactions.is_empty() {
            PlaceholderWidget::dark_gray("no reactions").render(block_area, buf);
            return;
        }

        // list
        if let Some(selected) = state.list_state.selected
            && selected > self.reactions.len() - 1
        {
            state.list_state.select(None);
        }

        if state.list_state.selected.is_none() && !self.reactions.is_empty() {
            state.list_state.select(Some(0));
        }

        const ITEM_HEIGHT: usize = 3;
        let is_scrollable = self.reactions.len() * ITEM_HEIGHT > block_area.height as usize;

        let list_builder = ListBuilder::new(|context| {
            let reaction = &self.reactions[context.index];

            let item = ReactionWidget {
                item: reaction,
                is_selected: context.is_selected,
                is_scrollable,
            };

            (item, ITEM_HEIGHT as u16)
        });

        let list = ListView::new(list_builder, self.reactions.len())
            .scrollbar(default_scrollbar())
            .infinite_scrolling(false);

        list.render(block_area, buf, &mut state.list_state);
    }
}

struct ReactionWidget<'a> {
    pub item: &'a ReactionsViewerItem<'a>,
    pub is_selected: bool,
    pub is_scrollable: bool,
}

impl<'a> Widget for ReactionWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let area = Rect::new(area.x, area.y, area.width, area.height - 1);

        let block = Block::new()
            .padding(Padding::new(1, if self.is_scrollable { 3 } else { 0 }, 0, 0))
            .borders(Borders::LEFT)
            .border_type(BorderType::QuadrantInside)
            .border_style(Style::new().fg(if self.is_selected { Color::Yellow } else { Color::Black }));

        let block_area = block.inner(area);
        block.render(area, buf);

        let v = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(block_area);
        let v0_h = Layout::horizontal([Constraint::Fill(1), Constraint::Length(1), Constraint::Length(2)]).split(v[0]);
        let v1_h = Layout::horizontal([Constraint::Fill(1), Constraint::Length(1), Constraint::Length(5)]).split(v[1]);

        // first line
        Line::from(vec![
            self.item.node.short_name_to_span(),
            " ".to_span(),
            self.item.node.long_name().to_span(),
        ])
        .render(v0_h[0], buf);

        Span::from(&self.item.reaction.emoji).render(v0_h[2], buf);

        // second line
        Line::from(if self.item.node.my {
            vec!["my node".to_span().dark_gray()]
        } else {
            self.item.reaction.hops_to_spans()
        })
        .render(v1_h[0], buf);

        Span::from(
            self.item
                .reaction
                .datetime
                .with_timezone(&chrono::Local)
                .format("%H:%M")
                .to_string(),
        )
        .dark_gray()
        .render(v1_h[2], buf);
    }
}
