use crate::ui::prelude::*;
use meshtastic::protobufs::HardwareModel;

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
        let h = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(2)]).split(area);

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::from(crate::APP_NAME).magenta().bold(),
                Span::from(" "),
                Span::from(crate::APP_VERSION).dark_gray(),
            ])),
            h[0],
        );

        let hw_model = state.device_metadata.as_ref().and_then(|metadata| {
            HardwareModel::try_from(metadata.hw_model)
                .ok()
                .and_then(|h| Some(h.as_str_name()))
        });

        let my_node_info = if let Some(my_node) = state.get_my_node()
            && !matches!(state.connection_state, ConnectionState::NotConnected)
        {
            vec![
                if let Some(hw) = hw_model {
                    Span::from(hw).magenta()
                } else {
                    Span::from("node").dark_gray()
                },
                Span::from(" "),
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
            ConnectionState::LoadingConfig => vec![
                Span::from("loading nodes ").yellow(),
                Span::from(state.nodes_stash.len().to_string()).yellow(),
                Span::from("/").yellow().dim(),
                Span::from(state.nodes_stash_cap.to_string()).yellow(),
                Span::from(" "),
                Span::from("■").fg(if state.rx { Color::Red } else { Color::DarkGray }),
            ],
            ConnectionState::Connected => vec![
                Span::from("online ").dark_gray(),
                Span::from(state.online_nodes.to_string()).green(),
                Span::from("/").green().dim(),
                Span::from(state.nodes.len().to_string()).green(),
                Span::from(" "),
                Span::from("■").fg(if state.rx { Color::Red } else { Color::DarkGray }),
            ],
        };

        frame.render_widget(
            Line::from([my_node_info.as_slice(), conn_info.as_slice()].concat()).right_aligned(),
            h[1],
        );
    }
}
