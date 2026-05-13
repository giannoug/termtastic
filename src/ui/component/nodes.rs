use crate::ui::prelude::*;

pub struct Nodes<'a> {
    list_state: ListState,
    filter_input: TextArea<'a>,
    is_filter_input_dirty: bool,
    nodeinfo: Option<u32>,
    nodeinfo_state: NodeInfoState,
    is_filter_help_visible: bool,
    filter_help_scroll_state: ScrollbarState,
}

impl<'a> Nodes<'a> {
    pub fn new() -> Self {
        let mut filter_input = TextArea::default();
        filter_input.set_placeholder_text("nodes filter...");
        filter_input.set_cursor_line_style(Style::default());

        Self {
            list_state: ListState::default(),
            filter_input,
            is_filter_input_dirty: false,
            nodeinfo: None,
            nodeinfo_state: NodeInfoState::new(),
            is_filter_help_visible: false,
            filter_help_scroll_state: ScrollbarState::default(),
        }
    }

    fn get_hotkeys(&self) -> Vec<Hotkey> {
        if self.is_filter_help_visible {
            return vec![Hotkey::new("↑↓", "scroll"), Hotkey::new("esc", "close")];
        }

        if self.nodeinfo.is_some() {
            return vec![Hotkey::new("esc", "close")];
        }

        vec![
            Hotkey::new("↑↓", "scroll"),
            Hotkey::new("F1", "help"),
            Hotkey::new("enter [F4]", "node info"),
            Hotkey::new("F2", "direct"),
            Hotkey::new("F6", "sort by"),
        ]
    }

    fn render_filter_help(&mut self, area: Rect, buf: &mut Buffer) {
        let popup_area = Rect {
            x: area.x + area.width / 2 - 50 / 2,
            y: area.y + area.height / 2 - 16 / 2,
            width: 50,
            height: 16,
        };

        let popup_block = Block::bordered()
            .border_type(BorderType::Thick)
            .padding(Padding::symmetric(2, 1))
            .title(" help ");

        let popup_block_area = popup_block.inner(popup_area);

        Clear.render(popup_area, buf);
        popup_block.render(popup_area, buf);

        let h = Layout::horizontal([Constraint::Fill(1), Constraint::Length(1), Constraint::Length(1)])
            .split(popup_block_area);

        let paragraph = Paragraph::new(vec![
            Line::from(vec![
                Span::from("The nodes filter can accept one or more "),
                Span::from("space-separated").bold(),
                Span::from(" tokens. All tokens are applied using the logical "),
                Span::from("AND").bold().blue(),
                Span::from(" operator. Special tokens begin with the "),
                Span::from("$").magenta(),
                Span::from(" symbol; a\u{00A0}list of them is provided below:"),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::from("$online     ").magenta(),
                Span::from("show only online nodes"),
            ]),
            Line::from(vec![
                Span::from("$offline    ").magenta(),
                Span::from("show only offline nodes"),
            ]),
            Line::from(vec![
                Span::from("$direct     ").magenta(),
                Span::from("show only direct nodes"),
            ]),
            Line::from(vec![
                Span::from("$remote     ").magenta(),
                Span::from("show only nodes with hops > 0"),
            ]),
            Line::from(vec![
                Span::from("$hops").magenta(),
                Span::from("X      ").cyan(),
                Span::from("show only nodes with hops == X"),
            ]),
            Line::from(vec![
                Span::from("$favorite   ").magenta(),
                Span::from("show only favorite nodes"),
            ]),
            Line::from(vec![
                Span::from("$ignored    ").magenta(),
                Span::from("show only ignored nodes"),
            ]),
            Line::from(vec![
                Span::from("$muted      ").magenta(),
                Span::from("show only muted nodes"),
            ]),
            Line::from(vec![
                Span::from("$temporary  ").magenta(),
                Span::from("show only temporary nodes"),
            ]),
        ])
        .wrap(Wrap { trim: false })
        .scroll((self.filter_help_scroll_state.get_position() as u16, 0));

        let paragraph_lines = paragraph.line_count(h[0].width);
        self.filter_help_scroll_state = self
            .filter_help_scroll_state
            .content_length(paragraph_lines.saturating_sub(popup_block_area.height as usize) + 1);

        paragraph.render(h[0], buf);

        default_scrollbar().render(h[2], buf, &mut self.filter_help_scroll_state);
    }
}

impl<'a> Component for Nodes<'a> {
    fn handle_event(
        &mut self,
        state: &State,
        event: &Event,
        emit: &impl Fn(AppEvent) -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
        if self.is_filter_help_visible {
            match event {
                Event::Key(KeyEvent { code, kind, .. }) if kind == &KeyEventKind::Press => match code {
                    KeyCode::Esc => {
                        self.is_filter_help_visible = false;
                        return Ok(true);
                    }
                    KeyCode::Up => {
                        self.filter_help_scroll_state.prev();
                        return Ok(true);
                    }
                    KeyCode::Down => {
                        self.filter_help_scroll_state.next();
                        return Ok(true);
                    }
                    _ => {}
                },
                Event::Mouse(MouseEvent { kind, .. }) => match kind {
                    MouseEventKind::ScrollUp => {
                        self.filter_help_scroll_state.prev();
                        return Ok(true);
                    }
                    MouseEventKind::ScrollDown => {
                        self.filter_help_scroll_state.next();
                        return Ok(true);
                    }
                    _ => {}
                },
                _ => {}
            }

            return Ok(false);
        }

        if let Some(node_key) = self.nodeinfo {
            if self.nodeinfo_state.handle_event(event.clone(), &mut |ev| match ev {
                NodeInfoWidgetEvent::PublicKeyCopyRequested => {
                    if let Some(node) = state.nodes.get(&node_key) {
                        emit(AppEvent::CopyToClipboardRequested(node.public_key.base64_encode()))?;
                    }

                    Ok(())
                }
                NodeInfoWidgetEvent::NodeDeleteRequested => {
                    emit(AppEvent::NodeDeleteRequested(node_key))?;
                    self.nodeinfo = None;
                    Ok(())
                }
            })? {
                return Ok(true);
            }

            if let Event::Key(KeyEvent {
                code: KeyCode::Esc,
                kind: KeyEventKind::Press,
                ..
            }) = event
            {
                self.nodeinfo = None;
            }

            return Ok(true);
        }

        match event {
            Event::Key(KeyEvent { code, kind, .. }) if kind == &KeyEventKind::Press => match code {
                KeyCode::Up => {
                    self.list_state.previous();
                    return Ok(true);
                }
                KeyCode::Down => {
                    self.list_state.next();
                    return Ok(true);
                }
                KeyCode::Home => {
                    self.list_state.select(Some(0));
                    return Ok(true);
                }
                KeyCode::End => {
                    self.list_state.select(Some(state.nodes_view.len() - 1));
                    return Ok(true);
                }
                KeyCode::F(1) => {
                    self.is_filter_help_visible = true;
                    return Ok(true);
                }
                KeyCode::F(4) | KeyCode::Enter => {
                    if let Some(node_key) = self.list_state.selected.and_then(|index| state.nodes_view.get(index)) {
                        self.nodeinfo = Some(*node_key);
                    }
                    return Ok(true);
                }
                KeyCode::F(6) => {
                    emit(AppEvent::NodesSortByCyclePressed)?;
                    return Ok(true);
                }
                KeyCode::F(2) => {
                    if let Some(node_key) = self.list_state.selected.and_then(|index| state.nodes_view.get(index)) {
                        emit(AppEvent::DirectChatRequested(*node_key))?;
                    }
                    return Ok(true);
                }
                KeyCode::Tab | KeyCode::BackTab => {
                    // Capture these events to prevent handling them by input widget
                    return Ok(false);
                }
                _ => {}
            },
            Event::Mouse(MouseEvent { kind, .. }) => match kind {
                MouseEventKind::ScrollUp => {
                    self.list_state.previous();
                    return Ok(true);
                }
                MouseEventKind::ScrollDown => {
                    self.list_state.next();
                    return Ok(true);
                }
                _ => {}
            },
            _ => {}
        }

        if self.filter_input.input(event.clone()) {
            self.is_filter_input_dirty = true;
            emit(AppEvent::NodesFilterChanged(self.filter_input.lines()[0].clone()))?;
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

        if !self.is_filter_input_dirty {
            self.filter_input.clear();
            self.filter_input.insert_str(&state.nodes_filter);
            self.is_filter_input_dirty = true;
        }

        if !state.nodes_view.is_empty() && self.list_state.selected.is_none() {
            self.list_state.select(Some(0));
        }

        let v = Layout::vertical([Constraint::Fill(1), Constraint::Length(3), Constraint::Length(1)]).split(area);
        let is_popup_visible = self.nodeinfo.is_some() || self.is_filter_help_visible;

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
                .scrollbar(default_scrollbar())
                .add_modifier(if is_popup_visible {
                    Modifier::DIM
                } else {
                    Modifier::empty()
                });

            list.render(v[0], frame.buffer_mut(), &mut self.list_state);
        } else {
            PlaceholderWidget::dark_gray("no nodes").render(v[0], frame.buffer_mut());
        }

        let v1_h = Layout::horizontal([Constraint::Fill(3), Constraint::Fill(2)]).split(v[1]);

        let filter_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().dark_gray())
            .padding(Padding::symmetric(1, 0))
            .add_modifier(if is_popup_visible {
                Modifier::DIM
            } else {
                Modifier::empty()
            });

        let filter_block_area = filter_block.inner(v1_h[0]);
        filter_block.render(v1_h[0], frame.buffer_mut());

        self.filter_input.render(filter_block_area, frame.buffer_mut());

        let sort_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().magenta())
            .padding(Padding::symmetric(1, 0))
            .add_modifier(if is_popup_visible {
                Modifier::DIM
            } else {
                Modifier::empty()
            });

        let sort_block_area = sort_block.inner(v1_h[1]);
        sort_block.render(v1_h[1], frame.buffer_mut());

        Line::from(Span::from(state.nodes_sort_by.to_string()).magenta())
            .centered()
            .render(sort_block_area, frame.buffer_mut());

        // NodeInfo popup
        if let Some(node_key) = self.nodeinfo {
            let popup_area = Rect {
                x: v[0].x + v[0].width / 2 - 70 / 2,
                y: v[0].y + v[0].height / 2 - 20 / 2,
                width: 70,
                height: 20,
            };

            Clear.render(popup_area, frame.buffer_mut());

            NodeInfoWidget::new(state.nodes.get(&node_key)).render(
                popup_area,
                frame.buffer_mut(),
                &mut self.nodeinfo_state,
            );
        }

        // filter help popup
        if self.is_filter_help_visible {
            self.render_filter_help(v[0], frame.buffer_mut());
        }

        HotkeysWidget::new(&self.get_hotkeys()).render(v[2], frame.buffer_mut());
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

        let v = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(block_area);

        let v0_h = Layout::horizontal([Constraint::Fill(2), Constraint::Fill(1), Constraint::Fill(1)])
            .flex(Flex::SpaceBetween)
            .split(v[0]);

        let v1_h = Layout::horizontal([Constraint::Fill(2), Constraint::Fill(1), Constraint::Fill(1)])
            .flex(Flex::SpaceBetween)
            .split(v[1]);

        // first line
        Line::from(vec![
            short_name_to_span(self.node),
            Span::from(" "),
            Span::from(self.node.long_name()),
        ])
        .render(v0_h[0], buf);

        Line::from(hops_to_spans(self.node)).render(v0_h[1], buf);

        Line::from(last_heard_to_spans(self.node))
            .right_aligned()
            .render(v0_h[2], buf);

        // second line
        Line::from(vec![Span::from(self.node.hw_model()).magenta()]).render(v1_h[0], buf);

        Line::from(vec![Span::from(self.node.role()).dark_gray()]).render(v1_h[1], buf);

        Line::from(vec![Span::from(self.node.id.clone()).dark_gray()])
            .right_aligned()
            .render(v1_h[2], buf);
    }
}
