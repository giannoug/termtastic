use std::{
    collections::{HashMap, VecDeque},
    iter,
    ops::RangeInclusive,
    sync::LazyLock,
};

use itertools::Itertools;
use ordermap::OrderMap;
use tracing_unwrap::OptionExt;
use tui_widget_list::ScrollDirection;

use crate::ui::prelude::*;

const INPUT_VALUE_MAX_LENGTH: usize = 200;
const VALID_INPUT_LENGTH: RangeInclusive<usize> = 1..=INPUT_VALUE_MAX_LENGTH;

static EMPTY_MESSAGES_VEC: LazyLock<VecDeque<Message>> = LazyLock::new(|| VecDeque::default());

pub struct Messenger<'a> {
    list_states: HashMap<u32, ListState>,
    input_widgets: HashMap<u32, TextArea<'a>>,
    follow_chat: HashMap<u32, bool>,
    replying_to: HashMap<u32, (Node, u32)>,
    is_emoji_selector_visible: bool,
    emoji_selector_state: EmojiSelectorState<'a>,
    is_reaction_viewer_visible: bool,
    reactions_viewer_state: ReactionViewerState,
}

impl<'a> Messenger<'a> {
    pub fn new() -> Self {
        Self {
            list_states: HashMap::default(),
            input_widgets: HashMap::default(),
            follow_chat: HashMap::default(),
            replying_to: HashMap::default(),
            emoji_selector_state: EmojiSelectorState::new(),
            is_emoji_selector_visible: false,
            reactions_viewer_state: ReactionViewerState::new(),
            is_reaction_viewer_visible: false,
        }
    }
}

impl<'a> Component for Messenger<'a> {
    fn get_hotkeys(&self, state: &State) -> Vec<Hotkey> {
        if self.is_reaction_viewer_visible {
            return vec![Hotkey::new("↑↓", "scroll"), Hotkey::new("esc", "close")];
        }

        if self.is_emoji_selector_visible {
            return vec![
                Hotkey::new("↑↓", "scroll"),
                Hotkey::new("enter", "insert"),
                Hotkey::new("esc", "close"),
            ];
        }

        if state.nodeinfo_popup.is_some() {
            return vec![Hotkey::new("esc", "close")];
        }

        let active_channel_key = state.active_channel_key.expect_or_log("channel should be selected");

        let is_input_contains_single_emoji = self
            .input_widgets
            .get(&active_channel_key)
            .and_then(|input| emoji::lookup_by_glyph::lookup(&input.lines()[0]))
            .is_some();

        let has_valid_input_value = self
            .input_widgets
            .get(&active_channel_key)
            .and_then(|input| Some(VALID_INPUT_LENGTH.contains(&input.lines()[0].len())))
            .unwrap_or(false);

        if self.replying_to.contains_key(&active_channel_key) {
            return vec![
                is_input_contains_single_emoji.then_some(Hotkey::new("enter", "send reaction")),
                (!is_input_contains_single_emoji && has_valid_input_value)
                    .then_some(Hotkey::new("enter", "send reply")),
                Some(Hotkey::new("esc", "cancel reply")),
            ]
            .into_iter()
            .flatten()
            .collect();
        }

        let is_message_selected = self
            .list_states
            .get(&active_channel_key)
            .and_then(|s| Some(s.selected.is_some()))
            .unwrap_or(false);

        Vec::from([
            Some(Hotkey::new("↑↓", "scroll")),
            (is_message_selected).then_some(Hotkey::new("F2", "reply")),
            (is_message_selected).then_some(Hotkey::new("F4", "node info")),
            Some(Hotkey::new("F5", "emoji")),
            is_message_selected.then_some(Hotkey::new("F7", "reactions")),
            has_valid_input_value.then_some(Hotkey::new("enter", "send")),
            Some(Hotkey::new("esc", "switch channel")),
        ])
        .into_iter()
        .flatten()
        .collect()
    }

    fn handle_event(
        &mut self,
        state: &State,
        event: &Event,
        emit: &impl Fn(AppEvent) -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
        let active_channel_key = state.active_channel_key.expect_or_log("channel should be selected");

        let list_state = self
            .list_states
            .entry(active_channel_key)
            .or_insert_with(|| ListState::default());

        let input_widget = self
            .input_widgets
            .entry(active_channel_key)
            .or_insert_with(|| new_input_widget());

        let is_replying_to = self.replying_to.contains_key(&active_channel_key);

        let messages = state.messages.get(&active_channel_key).unwrap_or(&EMPTY_MESSAGES_VEC);

        if self.is_reaction_viewer_visible {
            match event {
                Event::Key(KeyEvent { code, kind, .. }) => match code {
                    KeyCode::Esc if kind == &KeyEventKind::Press => {
                        self.is_reaction_viewer_visible = false;
                        return Ok(true);
                    }
                    _ => {}
                },
                _ => {}
            };

            return self.reactions_viewer_state.handle_event(event.clone());
        }

        if self.is_emoji_selector_visible {
            match event {
                Event::Key(KeyEvent { code, kind, .. }) => match code {
                    KeyCode::Enter if kind == &KeyEventKind::Press => {
                        if let Some(emoji) = self.emoji_selector_state.get_value() {
                            input_widget.insert_str(emoji.glyph);
                            self.is_emoji_selector_visible = false;
                            return Ok(true);
                        }
                    }
                    KeyCode::Esc if kind == &KeyEventKind::Press => {
                        self.is_emoji_selector_visible = false;
                        self.emoji_selector_state.reset();
                        return Ok(true);
                    }
                    _ => {}
                },
                _ => {}
            };

            return self.emoji_selector_state.handle_event(event.clone());
        }

        if is_replying_to {
            match event {
                Event::Key(KeyEvent { code, kind, .. }) => match code {
                    KeyCode::F(5) if kind == &KeyEventKind::Press => {
                        self.is_emoji_selector_visible = true;
                        return Ok(true);
                    }
                    KeyCode::Enter if kind == &KeyEventKind::Press => {
                        if input_widget.lines()[0].len() <= INPUT_VALUE_MAX_LENGTH
                            && let Some((_, message_id)) = self.replying_to.remove(&active_channel_key)
                        {
                            if let Some(emoji) = emoji::lookup_by_glyph::lookup(&input_widget.lines()[0]) {
                                emit(AppEvent::ChatReactionSubmitted {
                                    emoji,
                                    reply_message_id: Some(message_id),
                                })?;
                            } else {
                                emit(AppEvent::ChatMessageSubmitted {
                                    text: input_widget.lines()[0].clone(),
                                    reply_message_id: Some(message_id),
                                })?;
                            }

                            input_widget.clear();

                            return Ok(true);
                        }
                    }
                    KeyCode::Esc if kind == &KeyEventKind::Press => {
                        self.replying_to.remove(&active_channel_key);
                        return Ok(true);
                    }
                    _ => {}
                },
                _ => {}
            };

            input_widget.input(event.clone());

            return Ok(true);
        }

        match event {
            Event::Key(KeyEvent { code, kind, .. }) => match code {
                KeyCode::Up if kind == &KeyEventKind::Press => {
                    self.follow_chat.insert(active_channel_key, false);
                    list_state.previous();
                    return Ok(true);
                }
                KeyCode::Down if kind == &KeyEventKind::Press => {
                    list_state.next();

                    if let Some(index) = list_state.selected {
                        self.follow_chat.insert(active_channel_key, index == messages.len() - 1);
                    }

                    return Ok(true);
                }
                KeyCode::Esc if kind == &KeyEventKind::Press => {
                    emit(AppEvent::SwitchChannelRequested)?;
                    return Ok(true);
                }
                KeyCode::Enter if kind == &KeyEventKind::Press => {
                    if input_widget.lines()[0].len() <= INPUT_VALUE_MAX_LENGTH {
                        emit(AppEvent::ChatMessageSubmitted {
                            text: input_widget.lines()[0].clone(),
                            reply_message_id: None,
                        })?;

                        input_widget.clear();
                    }

                    return Ok(true);
                }
                KeyCode::F(2) if kind == &KeyEventKind::Press => {
                    if let Some(message) = list_state.selected.and_then(|i| messages.get(i)) {
                        let node = state.nodes.get(&message.from).unwrap_or(&UNKNOWN_NODE);
                        self.replying_to.insert(active_channel_key, (node.clone(), message.id));
                    }

                    return Ok(true);
                }
                KeyCode::F(4) if kind == &KeyEventKind::Press => {
                    if let Some(node_key) = list_state
                        .selected
                        .and_then(|i| messages.get(i))
                        .and_then(|message| Some(message.from))
                    {
                        emit(AppEvent::NodeInfoPopupRequested(node_key))?;
                    }

                    return Ok(true);
                }
                KeyCode::F(5) if kind == &KeyEventKind::Press => {
                    self.is_emoji_selector_visible = true;
                    return Ok(true);
                }
                KeyCode::F(7) if kind == &KeyEventKind::Press => {
                    if list_state.selected.and_then(|i| messages.get(i)).is_some() {
                        self.follow_chat.insert(active_channel_key, false);
                        self.is_reaction_viewer_visible = true;
                    }

                    return Ok(true);
                }
                KeyCode::Tab | KeyCode::BackTab => {
                    // Capture these events to prevent handling by input widget
                    return Ok(false);
                }
                _ => {}
            },
            Event::Mouse(MouseEvent { kind, .. }) => match kind {
                MouseEventKind::ScrollUp => {
                    self.follow_chat.insert(active_channel_key, false);
                    list_state.previous();

                    return Ok(false);
                }
                MouseEventKind::ScrollDown => {
                    list_state.next();

                    if let Some(index) = list_state.selected {
                        self.follow_chat.insert(active_channel_key, index == messages.len() - 1);
                    }

                    return Ok(false);
                }
                _ => {}
            },
            _ => {}
        }

        input_widget.input(event.clone());

        Ok(true)
    }

    fn render(&mut self, state: &State, frame: &mut Frame, area: Rect) {
        let active_channel = state.get_active_channel().expect_or_log("channel should be selected");

        let list_state = self
            .list_states
            .entry(active_channel.key)
            .or_insert_with(|| ListState::default());

        let input_widget = self
            .input_widgets
            .entry(active_channel.key)
            .or_insert_with(|| new_input_widget());

        let replying_to = self.replying_to.get(&active_channel.key);

        let messages = state.messages.get(&active_channel.key).unwrap_or(&EMPTY_MESSAGES_VEC);

        let follow_chat = self.follow_chat.entry(active_channel.key).or_insert(true);
        if *follow_chat && !messages.is_empty() {
            list_state.select(Some(messages.len() - 1));
        }

        let v = Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).split(area);
        let is_any_popup_visible =
            state.nodeinfo_popup.is_some() || self.is_emoji_selector_visible || self.is_reaction_viewer_visible;

        // list
        if !messages.is_empty() {
            let list_builder = ListBuilder::new(|context| {
                let message = &messages[context.index as usize];
                let replied_message = if message.reply_message_id > 0 {
                    messages
                        .iter()
                        .find(|m| m.id == message.reply_message_id)
                        .and_then(|m| Some((state.nodes.get(&m.from).unwrap_or(&UNKNOWN_NODE), m)))
                } else {
                    None
                };
                let node = state.nodes.get(&message.from).unwrap_or(&UNKNOWN_NODE);

                let item = MessageWidget {
                    node: &node,
                    message,
                    replied_message,
                    is_selected: context.is_selected,
                    is_highlighted: replying_to
                        .and_then(|(_, msg_key)| Some(message.id == *msg_key))
                        .unwrap_or(false),
                };

                let mut height = item.height(area.width);

                if context.index < messages.len() - 1 {
                    height += 1;
                }

                (item, height)
            });

            let list = ListView::new(list_builder, messages.len())
                .infinite_scrolling(false)
                .scroll_direction(ScrollDirection::Backward)
                .scrollbar(default_scrollbar())
                .add_modifier(if is_any_popup_visible {
                    Modifier::DIM
                } else {
                    Modifier::empty()
                });

            list.render(v[0], frame.buffer_mut(), list_state);
        } else {
            PlaceholderWidget::dark_gray("no messages").render(v[0], frame.buffer_mut());
        }

        // input
        let input_block = Block::bordered()
            .padding(Padding::symmetric(1, 0))
            .border_type(BorderType::Rounded)
            .border_style(Style::new().dark_gray())
            .add_modifier(if is_any_popup_visible {
                Modifier::DIM
            } else {
                Modifier::empty()
            });

        let input_block_area = input_block.inner(v[1]);

        let channel_name_spans = match (&active_channel.role, replying_to) {
            (ChannelRole::Primary | ChannelRole::Secondary, None) => channel_name_to_spans(active_channel, state)
                .iter()
                .chain(iter::once(&Span::from(" ←").dark_gray()))
                .cloned()
                .collect(),
            (ChannelRole::Direct, None) => vec![
                short_name_to_span(state.nodes.get(&active_channel.key).unwrap_or(&UNKNOWN_NODE)),
                Span::from(" ←").dark_gray(),
            ],
            (_, Some((node, _))) => vec![
                Span::from("reply to ").cyan(),
                short_name_to_span(node),
                Span::from(" ←").dark_gray(),
            ],
            _ => unreachable!(),
        };

        let channel_line = Line::from(channel_name_spans);

        let input_block_area_h = Layout::horizontal([
            Constraint::Length(channel_line.width() as u16),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(8),
        ])
        .split(input_block_area);

        input_block.render(v[1], frame.buffer_mut());
        channel_line.render(input_block_area_h[0], frame.buffer_mut());
        frame.render_widget(&*input_widget, input_block_area_h[2]);

        let input_value_len = input_widget.lines()[0].len();

        Line::from(
            Span::from(format!(" {}/{}", input_value_len, INPUT_VALUE_MAX_LENGTH)).style(Style::new().fg(
                if input_value_len > INPUT_VALUE_MAX_LENGTH {
                    Color::Red
                } else {
                    Color::DarkGray
                },
            )),
        )
        .right_aligned()
        .render(input_block_area_h[3], frame.buffer_mut());

        // reaction viewer
        if self.is_reaction_viewer_visible
            && let Some(message) = list_state.selected.and_then(|i| messages.get(i))
        {
            let popup_area = Rect {
                x: v[0].x + v[0].width / 2 - 40 / 2,
                y: v[0].y + v[0].height / 2 - 15 / 2,
                width: 40,
                height: 15,
            };

            Clear.render(popup_area, frame.buffer_mut());

            self.is_reaction_viewer_visible = true;

            let reaction_items: Vec<ReactionViewerItem> = message
                .reactions
                .iter()
                .map(|reaction| {
                    let node = state.nodes.get(&reaction.node_key).unwrap_or(&UNKNOWN_NODE);

                    ReactionViewerItem { reaction, node }
                })
                .collect();

            ReactionViewerWidget::new(reaction_items).render(
                popup_area,
                frame.buffer_mut(),
                &mut self.reactions_viewer_state,
            );
        }

        // emoji selector
        if self.is_emoji_selector_visible {
            let popup_area = Rect {
                x: v[0].x + v[0].width / 2 - 40 / 2,
                y: v[0].y + v[0].height / 2 - 14 / 2,
                width: 40,
                height: 14,
            };

            Clear.render(popup_area, frame.buffer_mut());

            EmojiSelectorWidget::new().render(popup_area, frame.buffer_mut(), &mut self.emoji_selector_state);
        }
    }
}

fn new_input_widget() -> TextArea<'static> {
    let mut input = TextArea::default();
    input.set_placeholder_text("type message...");
    input.set_cursor_line_style(Style::default());

    input
}

struct MessageWidget<'a> {
    pub node: &'a Node,
    pub message: &'a Message,
    pub replied_message: Option<(&'a Node, &'a Message)>,
    pub is_selected: bool,
    pub is_highlighted: bool,
}

impl MessageWidget<'_> {
    pub fn get_text_paragraph(&self) -> Paragraph<'_> {
        let reply_line = self.replied_message.and_then(|(_, m)| {
            #[allow(unstable_name_collisions)]
            let spans: Vec<Span<'_>> = m
                .text
                .split('\n')
                .map(|line| Span::from(line))
                .intersperse(Span::from(" "))
                .collect();

            Some(
                Line::from(
                    iter::once("“".to_span())
                        .chain(spans)
                        .chain(iter::once("”".to_span()))
                        .collect::<Vec<Span<'_>>>(),
                )
                .magenta(),
            )
        });

        let text_lines: Vec<Line<'_>> = self.message.text.split('\n').map(Line::from).collect();

        Paragraph::new(reply_line.into_iter().chain(text_lines).collect::<Vec<Line<'_>>>()).wrap(Wrap { trim: false })
    }

    pub fn height(&self, width: u16) -> u16 {
        1 + self.get_text_paragraph().line_count(width - 2) as u16 + !self.message.reactions.is_empty() as u16
    }
}

impl<'a> Widget for MessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let text_paragraph = self.get_text_paragraph();
        let text_height = text_paragraph.line_count(area.width - 2) as u16;

        let area = Rect {
            x: area.x,
            y: area.y,
            width: area.width - 2,
            height: 1 + text_height + !self.message.reactions.is_empty() as u16,
        };

        let block = Block::bordered()
            .borders(Borders::LEFT)
            .border_set(if self.is_selected {
                symbols::border::THICK
            } else {
                symbols::border::PLAIN
            })
            .border_style(Style::new().fg(if self.is_highlighted {
                Color::Cyan
            } else if self.is_selected {
                Color::Yellow
            } else {
                Color::DarkGray
            }))
            .padding(Padding::symmetric(1, 0));

        let block_area = block.inner(area);
        block.render(area, buf);

        let v = Layout::vertical(if self.message.reactions.is_empty() {
            vec![Constraint::Length(1), Constraint::Length(text_height)]
        } else {
            vec![
                Constraint::Length(1),
                Constraint::Length(text_height),
                Constraint::Length(1),
            ]
        })
        .split(block_area);

        // first line
        let v0_h = Layout::horizontal([Constraint::Fill(4), Constraint::Fill(2), Constraint::Fill(1)])
            .flex(Flex::SpaceBetween)
            .split(v[0]);

        if let Some((rep_node, _)) = self.replied_message {
            Line::from(vec![
                short_name_to_span(self.node),
                " → ".to_span().dark_gray(),
                short_name_to_span(rep_node).on_magenta(),
            ])
            .render(v0_h[0], buf);
        } else {
            Line::from(vec![
                short_name_to_span(self.node),
                " ".to_span(),
                self.node.long_name().to_span().bold(),
            ])
            .render(v0_h[0], buf);
        }

        if !self.node.my {
            Line::from(hops_to_spans(self.message)).render(v0_h[1], buf);
        } else {
            routing_error_to_span(self.message.error).render(v0_h[1], buf);
        }

        Line::from(
            Span::from(
                self.message
                    .datetime
                    .with_timezone(&chrono::Local)
                    .format("%H:%M")
                    .to_string(),
            )
            .dark_gray(),
        )
        .right_aligned()
        .render(v0_h[2], buf);

        // second line
        text_paragraph.render(v[1], buf);

        // third line
        if !self.message.reactions.is_empty() {
            Line::from(
                self.message
                    .reactions
                    .iter()
                    .sorted_by_key(|r| r.datetime)
                    .fold(OrderMap::new(), |mut acc, r| {
                        *acc.entry(&r.emoji).or_insert(0) += 1;
                        acc
                    })
                    .iter()
                    .map(|(emoji, nodes_count)| {
                        if *nodes_count > 1 {
                            vec![emoji.to_span(), Span::from(format!(":{} ", nodes_count)).dark_gray()]
                        } else {
                            vec![emoji.to_span(), " ".to_span()]
                        }
                    })
                    .flatten()
                    .collect::<Vec<Span>>(),
            )
            .render(v[2], buf);
        }
    }
}
