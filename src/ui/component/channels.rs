use std::{collections::VecDeque, ops::Index};

use crate::ui::{helpers::default_scrollbar, prelude::*};
use chrono::Local;

pub struct Channels {
    list_state: ListState,
    channel_purge_key: Option<u32>,
}

impl Channels {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
            channel_purge_key: None,
        }
    }

    fn channels<'a>(&self, state: &'a State) -> impl Iterator<Item = &'a Channel> {
        state.channels.values().filter(|ch| !ch.role.is_disabled())
    }
}

impl<'a> Component for Channels {
    fn get_hotkeys(&self, _state: &State) -> Vec<Hotkey> {
        vec![
            Hotkey::new("↑↓", "scroll"),
            Hotkey::new("enter", "open"),
            Hotkey::new("del", "purge chat"),
        ]
    }

    fn handle_event(
        &mut self,
        state: &State,
        event: &Event,
        emit: &impl Fn(AppEvent) -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
        if let Some(channel_key) = &self.channel_purge_key {
            match event {
                Event::Key(KeyEvent { code, kind, .. }) if kind == &KeyEventKind::Press => match code {
                    KeyCode::Enter => {
                        emit(AppEvent::ChannelPurgeRequested(*channel_key))?;
                        self.channel_purge_key = None;

                        return Ok(true);
                    }
                    KeyCode::Esc => {
                        self.channel_purge_key = None;
                        return Ok(true);
                    }
                    _ => {}
                },
                _ => {}
            }

            return Ok(false);
        }

        if self
            .list_state
            .handle_navigation_events(event, self.channels(state).count())
        {
            return Ok(true);
        }

        match event {
            Event::Key(KeyEvent { code, kind, .. }) if kind == &KeyEventKind::Press => match code {
                KeyCode::Enter => {
                    if let Some(channel) = self.list_state.selected.and_then(|i| self.channels(state).nth(i)) {
                        emit(AppEvent::ChannelSelected(channel.key))?;
                    }

                    return Ok(true);
                }
                KeyCode::Delete | KeyCode::Backspace => {
                    if let Some(channel) = self.list_state.selected.and_then(|i| self.channels(state).nth(i)) {
                        self.channel_purge_key = Some(channel.key);
                    }

                    return Ok(true);
                }
                KeyCode::Tab | KeyCode::BackTab => {
                    return Ok(false);
                }
                _ => {}
            },
            _ => {}
        }

        Ok(false)
    }

    fn render(&mut self, state: &State, frame: &mut Frame, area: Rect) {
        let channels: Vec<&Channel> = self.channels(state).collect();

        if !channels.is_empty() {
            if let Some(selected) = self.list_state.selected
                && selected > channels.len() - 1
            {
                self.list_state.select(None);
            }

            if self.list_state.selected.is_none() && !channels.is_empty() {
                self.list_state.select(Some(0));
            }

            let empty_messages_vec: VecDeque<Message> = VecDeque::default();

            let list_builder = ListBuilder::new(|context| {
                let channel = channels.index(context.index);
                let messages = state.messages.get(&channel.key).unwrap_or(&empty_messages_vec);

                let last_message = messages.iter().last();
                let last_message_node = last_message.and_then(|message| state.nodes.get(&message.from));

                let item = ChannelWidget {
                    channel,
                    channel_name_spans: channel_name_to_spans(channel, state),
                    last_message,
                    last_message_node,
                    is_last_message_node_my: last_message_node
                        .and_then(|node| Some(state.my_node_key == Some(node.key)))
                        .unwrap_or(false),
                    is_selected: context.is_selected,
                };

                (item, 4)
            });

            let list = ListView::new(list_builder, channels.len())
                .infinite_scrolling(false)
                .scrollbar(default_scrollbar());

            list.render(area, frame.buffer_mut(), &mut self.list_state);
        } else {
            PlaceholderWidget::dark_gray("no channels").render(area, frame.buffer_mut());
        }

        if self.channel_purge_key.is_some() {
            PopupConfirmWidget::new(
                "Are you sure to delete the channel chat?",
                "confirm",
                "cancel",
                40,
                Color::Red,
            )
            .render(area, frame.buffer_mut());
        }
    }
}

struct ChannelWidget<'a> {
    pub channel: &'a Channel,
    pub channel_name_spans: Vec<Span<'a>>,
    pub last_message: Option<&'a Message>,
    pub last_message_node: Option<&'a Node>,
    pub is_last_message_node_my: bool,
    pub is_selected: bool,
}

impl<'a> Widget for ChannelWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let area = Rect {
            x: area.x,
            y: area.y,
            width: area.width - 2,
            height: area.height,
        };

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(if self.is_selected {
                Color::Yellow
            } else {
                Color::DarkGray
            }))
            .padding(Padding::symmetric(1, 0));

        let block_area = block.inner(area);
        block.render(area, buf);

        let v = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(block_area);
        let v0_h = Layout::horizontal([Constraint::Fill(3), Constraint::Fill(1), Constraint::Fill(1)]).split(v[0]);

        // first line
        Line::from(self.channel_name_spans).render(v0_h[0], buf);

        let type_span = match &self.channel.role {
            ChannelRole::Primary => Span::from("PRIMARY"),
            ChannelRole::Secondary => Span::from("SECONDARY"),
            ChannelRole::Direct => Span::from("DIRECT"),
            ChannelRole::Disabled => Span::from("DISABLED"),
        };

        Line::from(type_span).magenta().render(v0_h[1], buf);

        Line::from(if let Some(message) = self.last_message {
            Span::from(message.datetime.with_timezone(&Local).format("%H:%M").to_string())
        } else {
            Span::from("no messages").dark_gray()
        })
        .right_aligned()
        .render(v0_h[2], buf);

        // second line
        let second_line_spans = match (&self.channel.role, self.last_message_node, self.last_message) {
            (ChannelRole::Direct, _, Some(message)) => {
                vec![Span::from(message.text.clone()).dark_gray()]
            }
            (_, None, Some(message)) => {
                vec![
                    short_name_to_span(&UNKNOWN_NODE, false),
                    Span::from(" "),
                    Span::from(message.text.clone()).dark_gray(),
                ]
            }
            (_, Some(node), Some(message)) => {
                vec![
                    short_name_to_span(node, self.is_last_message_node_my),
                    Span::from(" "),
                    Span::from(message.text.clone()).dark_gray(),
                ]
            }
            (_, _, None) => {
                vec![]
            }
        };

        Line::from(second_line_spans).render(v[1], buf);
    }
}
