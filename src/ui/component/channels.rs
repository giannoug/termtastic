use std::{collections::VecDeque, ops::Index};

use crate::ui::{helpers::default_scrollbar, prelude::*};
use chrono::Local;
use meshtastic::protobufs::config::lo_ra_config::ModemPreset;

pub struct Channels {
    list_state: ListState,
    hotkeys: Vec<Hotkey>,
}

impl Channels {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
            hotkeys: vec![Hotkey::new("↑↓", "scroll"), Hotkey::new("enter", "open")],
        }
    }
}

impl<'a> Component for Channels {
    fn handle_event(
        &mut self,
        state: &State,
        event: &Event,
        emit: &impl Fn(AppEvent) -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
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
                KeyCode::Enter => {
                    if let Some(i) = self.list_state.selected {
                        let channel = state
                            .channels
                            .values()
                            .filter(|ch| !ch.role.is_disabled())
                            .nth(i)
                            .unwrap();

                        emit(AppEvent::ChannelSelected(channel.key))?;
                    }

                    return Ok(true);
                }
                KeyCode::Tab | KeyCode::BackTab => {
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

        Ok(false)
    }

    fn render(&mut self, state: &State, frame: &mut Frame, area: Rect) {
        let v = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);

        let channels: Vec<&Channel> = state.channels.values().filter(|ch| !ch.role.is_disabled()).collect();

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
            let radio_preset_name = state
                .device_config
                .lora
                .as_ref()
                .and_then(|lora| ModemPreset::try_from(lora.modem_preset).ok())
                .and_then(|preset| Some(preset.as_channel_name()));

            let list_builder = ListBuilder::new(|context| {
                let channel = channels.index(context.index);
                let messages = state.messages.get(&channel.key).unwrap_or(&empty_messages_vec);

                let last_message = messages.iter().last();
                let last_message_node = last_message.and_then(|message| state.nodes.get(&message.from));

                let item = ChannelWidget {
                    channel,
                    radio_preset_name: &radio_preset_name,
                    direct_node: if channel.role.is_direct() {
                        state.nodes.get(&channel.key)
                    } else {
                        None
                    },
                    last_message,
                    last_message_node,
                    is_selected: context.is_selected,
                };

                (item, 4)
            });

            let list = ListView::new(list_builder, channels.len())
                .infinite_scrolling(false)
                .scrollbar(default_scrollbar());

            list.render(v[0], frame.buffer_mut(), &mut self.list_state);
        } else {
            PlaceholderWidget::dark_gray("no channels").render(v[0], frame.buffer_mut());
        }

        HotkeysWidget::new(&self.hotkeys).render(v[1], frame.buffer_mut());
    }
}

struct ChannelWidget<'a> {
    pub channel: &'a Channel,
    pub radio_preset_name: &'a Option<String>,
    pub direct_node: Option<&'a Node>,
    pub last_message: Option<&'a Message>,
    pub last_message_node: Option<&'a Node>,
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
        let security_span = match self.channel.psk.len() {
            0 => Span::from("[non-encrypted]").red(),
            1 => Span::from("[weak]").yellow(),
            _ => Span::from("[encrypted]").green(),
        };

        let name_span = match (&self.channel.role, self.direct_node) {
            (ChannelRole::Primary, _) => vec![
                Span::from(format!("#{}", self.channel.key)).dark_gray(),
                Span::from(" "),
                Span::from(if !self.channel.name.is_empty() {
                    &self.channel.name
                } else if let Some(preset_name) = self.radio_preset_name.as_ref() {
                    preset_name
                } else {
                    "Primary"
                }),
                Span::from(" ").dark_gray(),
                security_span,
            ],
            (ChannelRole::Secondary, _) => vec![
                Span::from(format!("#{}", self.channel.key)).dark_gray(),
                Span::from(" "),
                Span::from(if !self.channel.name.is_empty() {
                    &self.channel.name
                } else {
                    "Secondary"
                }),
                Span::from(" ").dark_gray(),
                security_span,
            ],
            (ChannelRole::Direct, Some(node)) => {
                vec![short_name_to_span(node), Span::from(" "), Span::from(node.long_name())]
            }
            (ChannelRole::Direct, None) => {
                vec![Span::from(format!("!{:x}", self.channel.key))]
            }
            (ChannelRole::Disabled, _) => {
                vec![Span::from("Disabled")]
            }
        };

        Line::from(name_span).render(v0_h[0], buf);

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
                    short_name_to_span(&UNKNOWN_NODE),
                    Span::from(" "),
                    Span::from(message.text.clone()).dark_gray(),
                ]
            }
            (_, Some(node), Some(message)) => {
                vec![
                    short_name_to_span(node),
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
