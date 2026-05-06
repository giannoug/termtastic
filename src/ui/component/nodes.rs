use chrono::{SubsecRound, TimeDelta, Utc};

use crate::ui::{
    helpers::{default_scrollbar, ColorExt},
    prelude::*,
};

pub struct Nodes<'a> {
    list_state: ListState,
    filter_input: TextArea<'a>,
    hotkeys: Vec<Hotkey>,
}

impl<'a> Nodes<'a> {
    pub fn new() -> Self {
        let mut filter_input = TextArea::default();
        filter_input.set_placeholder_text("nodes filter...");
        filter_input.set_cursor_line_style(Style::default());
        filter_input.select_all();

        Self {
            list_state: ListState::default(),
            filter_input,
            hotkeys: vec![
                Hotkey {
                    key: "↑↓".to_string(),
                    label: "scroll".to_string(),
                },
                Hotkey {
                    key: "enter".to_string(),
                    label: "node info".to_string(),
                },
                Hotkey {
                    key: "F2".to_string(),
                    label: "direct".to_string(),
                },
                Hotkey {
                    key: "F6".to_string(),
                    label: "sort by".to_string(),
                },
            ],
        }
    }
}

impl<'a> Component for Nodes<'a> {
    fn handle_event(
        &mut self,
        state: &State,
        event: &Event,
        emit: &impl Fn(AppEvent) -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
        match event {
            Event::Key(KeyEvent { code, kind, .. }) if kind == &KeyEventKind::Press => match code {
                KeyCode::Up => self.list_state.previous(),
                KeyCode::Down => self.list_state.next(),
                KeyCode::Home => {
                    self.list_state.select(Some(0));
                }
                KeyCode::End => {
                    self.list_state.select(Some(state.nodes_view.len() - 1));
                }
                KeyCode::F(6) => {
                    emit(AppEvent::NodesSortByCyclePressed)?;
                }
                KeyCode::F(2) => {
                    if let Some(node_key) = self.list_state.selected.and_then(|index| state.nodes_view.get(index)) {
                        emit(AppEvent::DirectChatRequested(*node_key))?;
                    }
                }
                _ => {
                    if self.filter_input.input(event.clone()) {
                        emit(AppEvent::NodesFilterChanged(self.filter_input.lines()[0].clone()))?;
                    }
                }
            },
            Event::Mouse(MouseEvent { kind, .. }) => match kind {
                MouseEventKind::ScrollUp => self.list_state.previous(),
                MouseEventKind::ScrollDown => self.list_state.next(),
                _ => {}
            },
            _ => {}
        }

        Ok(true)
    }

    fn render(&mut self, state: &State, frame: &mut Frame, area: Rect) {
        if self
            .list_state
            .selected
            .and_then(|i| Some(i >= state.nodes_view.len()))
            .unwrap_or(false)
        {
            self.list_state.selected = None;
        }

        if !state.nodes_view.is_empty() && self.list_state.selected.is_none() {
            self.list_state.select(Some(0));
        }

        let v = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Length(3), Constraint::Length(1)])
            .split(area);

        if !state.nodes_view.is_empty() {
            let list_builder = ListBuilder::new(|context| {
                let node = &state.nodes[&state.nodes_view[context.index as usize]];

                let item = NodeWidget {
                    node,
                    is_selected: context.is_selected,
                };

                (item, 3)
            });

            let list = ListView::new(list_builder, state.nodes_view.len())
                .infinite_scrolling(false)
                .scrollbar(default_scrollbar());

            list.render(v[0], frame.buffer_mut(), &mut self.list_state);
        } else {
            PlaceholderWidget::dark_gray("no nodes").render(v[0], frame.buffer_mut());
        }

        let v1_h = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Fill(3), Constraint::Fill(2)])
            .split(v[1]);

        let filter_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().dark_gray())
            .padding(Padding::symmetric(1, 0));

        let filter_block_area = filter_block.inner(v1_h[0]);
        filter_block.render(v1_h[0], frame.buffer_mut());

        self.filter_input.render(filter_block_area, frame.buffer_mut());

        let sort_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().magenta())
            .padding(Padding::symmetric(1, 0));

        let sort_block_area = sort_block.inner(v1_h[1]);
        sort_block.render(v1_h[1], frame.buffer_mut());

        Line::from(Span::from(state.nodes_sort_by.to_string()).magenta())
            .centered()
            .render(sort_block_area, frame.buffer_mut());

        HotkeysWidget::new(&self.hotkeys).render(v[2], frame.buffer_mut());
    }
}

struct NodeWidget<'a> {
    pub node: &'a Node,
    pub is_selected: bool,
}

impl<'a> Widget for NodeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let area = Rect {
            x: area.x,
            y: area.y,
            width: area.width - 2,
            height: area.height - 1,
        };

        let block = Block::bordered()
            .borders(Borders::LEFT)
            .border_set(if self.is_selected {
                symbols::border::THICK
            } else {
                symbols::border::PLAIN
            })
            .border_style(Style::new().fg(if self.is_selected {
                Color::Yellow
            } else {
                Color::DarkGray
            }))
            .padding(Padding::symmetric(1, 0));

        let block_area = block.inner(area);
        block.render(area, buf);

        let v = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(block_area);

        let v0_h = Layout::default()
            .direction(Direction::Horizontal)
            .flex(layout::Flex::SpaceBetween)
            .constraints([Constraint::Fill(2), Constraint::Fill(1), Constraint::Fill(1)])
            .split(v[0]);

        let v1_h = Layout::default()
            .direction(Direction::Horizontal)
            .flex(layout::Flex::SpaceBetween)
            .constraints([Constraint::Fill(2), Constraint::Fill(1), Constraint::Fill(1)])
            .split(v[1]);

        // first line
        Line::from(vec![
            self.node.to_span(),
            Span::from(" "),
            Span::from(self.node.long_name.clone()),
        ])
        .render(v0_h[0], buf);

        Line::from(match self.node.hops_away {
            _ if self.node.my => Span::from("connected").blue(),
            Some(0) => {
                Span::from(format!("* {}dB", self.node.snr)).style(Style::new().fg(self.node.snr.snr_to_color()))
            }
            Some(1) => Span::from("1 hop"),
            Some(hops) => Span::from(format!("{} hops", hops)),
            None => Span::from("unknown").dark_gray(),
        })
        .render(v0_h[1], buf);

        let last_heard_spans: Vec<Span> = match self.node.last_heard {
            Some(_) if self.node.my => vec![Span::from("now").blue()],
            Some(dt) => humanize_duration(Utc::now().round_subsecs(0) - dt),
            None => vec![Span::from("?").dark_gray()],
        };

        Line::from(last_heard_spans).right_aligned().render(v0_h[2], buf);

        // second line
        Line::from(vec![Span::from(self.node.hw_model.clone()).magenta()]).render(v1_h[0], buf);

        Line::from(vec![Span::from(self.node.role.clone()).dark_gray()]).render(v1_h[1], buf);

        Line::from(vec![Span::from(self.node.id.clone()).dark_gray()])
            .right_aligned()
            .render(v1_h[2], buf);
    }
}

fn humanize_duration<'a>(d: TimeDelta) -> Vec<Span<'a>> {
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
