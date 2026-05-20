use crate::ui::prelude::*;

pub struct Header {}

impl Header {
    pub fn new() -> Self {
        Self {}
    }
}

impl Component for Header {
    fn get_hotkeys(&self, _state: &State) -> Vec<Hotkey> {
        Vec::default()
    }

    fn handle_event(
        &mut self,
        _state: &State,
        _event: &Event,
        _emit: &impl Fn(AppEvent) -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
        Ok(false)
    }

    fn render(&mut self, state: &State, frame: &mut Frame, area: Rect) {
        let v = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)])
            .flex(Flex::SpaceBetween)
            .split(area);

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::from(crate::APP_NAME).magenta().bold(),
                Span::from(" "),
                Span::from(crate::APP_VERSION).dark_gray(),
            ])),
            v[0],
        );

        let my_node_info = if let Some(my_node) = state.get_my_node()
            && !matches!(state.connection_state, ConnectionState::NotConnected)
        {
            vec![
                Span::from("node ").dark_gray(),
                short_name_to_span(my_node, true),
                Span::from("  "),
            ]
        } else {
            vec![]
        };

        let conn_info = match &state.connection_state {
            ConnectionState::NotConnected => vec![Span::from("not connected").dark_gray()],
            ConnectionState::ProblemDetected { .. } => {
                vec![
                    Span::from(format!(
                        "reconnecting in {} sec...",
                        state
                            .reconnection_backoff
                            .and_then(|b| Some(b.as_secs().to_string()))
                            .unwrap_or("?".to_owned())
                    ))
                    .red(),
                ]
            }
            ConnectionState::Connecting => vec![Span::from("connecting...").yellow()],
            ConnectionState::Connected => vec![
                Span::from("online ").dark_gray(),
                Span::from(format!("{}/{} ", state.online_nodes, state.nodes.len())).green(),
                Span::from("■").fg(if state.rx { Color::Red } else { Color::DarkGray }),
            ],
        };

        frame.render_widget(
            Line::from([my_node_info.as_slice(), conn_info.as_slice()].concat()).right_aligned(),
            v[1],
        );
    }
}
