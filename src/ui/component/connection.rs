use hostaddr::HostAddr;

use crate::ui::prelude::*;

pub struct Connection<'a> {
    list_state: ListState,
    is_tcp_form_visible: bool,
    popup_input_state: PopupInputState<'a>,
    removing_tcp_device: Option<HostAddr<String>>,
}

impl<'a> Connection<'a> {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
            is_tcp_form_visible: false,
            popup_input_state: PopupInputState::new(Some("TCP device"), Some("host[:port=4403]"), ""),
            removing_tcp_device: None,
        }
    }

    fn get_hotkeys(&self, state: &State) -> Vec<Hotkey> {
        if state.active_device.is_some() {
            return vec![Hotkey::new("esc", "disconnect")];
        }

        if self.is_tcp_form_visible {
            return vec![Hotkey::new("enter", "submit"), Hotkey::new("esc", "cancel")];
        }

        vec![
            Some(Hotkey::new("↑↓", "scroll")),
            Some(Hotkey::new("enter", "connect")),
            Some(Hotkey::new("t", "add TCP")),
            Some(Hotkey::new("r", "rediscover")),
            self.list_state
                .selected
                .and_then(|i| state.aggregated_devices.get(i))
                .and_then(|d| Some(matches!(d, Device::Tcp(_))))
                .unwrap_or(false)
                .then_some(Hotkey::red("del", "delete")),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

impl<'a> Component for Connection<'a> {
    fn handle_event(
        &mut self,
        state: &State,
        event: &Event,
        emit: &impl Fn(AppEvent) -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
        if let Some(removing_hostaddr) = &self.removing_tcp_device {
            match event {
                Event::Key(KeyEvent { code, kind, .. }) if kind == &KeyEventKind::Press => match code {
                    KeyCode::Enter => {
                        emit(AppEvent::TcpDeviceRemoved(removing_hostaddr.clone()))?;
                        self.removing_tcp_device = None;

                        return Ok(true);
                    }
                    KeyCode::Esc => {
                        self.removing_tcp_device = None;
                        return Ok(true);
                    }
                    _ => {}
                },
                _ => {}
            }

            return Ok(false);
        }

        if self.is_tcp_form_visible {
            match event {
                Event::Key(KeyEvent { code, kind, .. }) if kind == &KeyEventKind::Press => match code {
                    KeyCode::Enter => {
                        match self.popup_input_state.get_value().parse::<HostAddr<String>>() {
                            Ok(addr) => {
                                emit(AppEvent::TcpDeviceSubmitted(addr))?;
                                self.is_tcp_form_visible = false;
                            }
                            Err(e) => {
                                self.popup_input_state.set_error(format!("invalid address: {}", e));
                            }
                        }

                        return Ok(true);
                    }
                    KeyCode::Esc => {
                        self.is_tcp_form_visible = false;
                        return Ok(true);
                    }
                    _ => {}
                },
                _ => {}
            }

            return self.popup_input_state.handle_event(event.clone());
        }

        if state.active_device.is_some() {
            match event {
                Event::Key(KeyEvent { code, .. }) => match code {
                    KeyCode::Esc => {
                        emit(AppEvent::DisconnectionRequested)?;
                        return Ok(true);
                    }
                    KeyCode::Tab | KeyCode::BackTab => {
                        return Ok(false);
                    }
                    _ => {}
                },
                _ => {}
            }

            return Ok(false);
        }

        match event {
            Event::Key(KeyEvent { code, .. }) => match code {
                KeyCode::Up => {
                    self.list_state.previous();
                    return Ok(true);
                }
                KeyCode::Down => {
                    self.list_state.next();
                    return Ok(true);
                }
                KeyCode::Char('r') => {
                    emit(AppEvent::DeviceRediscoverRequested)?;
                    return Ok(true);
                }
                KeyCode::Char('t') => {
                    self.popup_input_state.reset();
                    self.is_tcp_form_visible = true;

                    return Ok(true);
                }
                KeyCode::Enter => {
                    if let Some(index) = self.list_state.selected {
                        emit(AppEvent::DeviceSelected(state.aggregated_devices[index].clone()))?
                    }
                    return Ok(true);
                }
                KeyCode::Delete | KeyCode::Backspace => {
                    if let Some(index) = self.list_state.selected
                        && let Device::Tcp(hostaddr) = &state.aggregated_devices[index]
                    {
                        self.removing_tcp_device = Some(hostaddr.clone());
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

        if !state.aggregated_devices.is_empty() {
            if self.list_state.selected.is_none()
                && state.device_discovering_state == DeviceDiscoveringState::Done
                && !state.aggregated_devices.is_empty()
            {
                if let Some(active) = &state.active_device {
                    self.list_state
                        .select(state.aggregated_devices.iter().position(|d| active == d));
                } else {
                    self.list_state.select(Some(0));
                }
            }

            let list_builder = ListBuilder::new(|context| {
                let device = state.aggregated_devices.iter().nth(context.index).unwrap();

                let item = DeviceWidget {
                    device,
                    is_selected: context.is_selected,
                    centered: false,
                };

                (item, 1)
            });

            let list = ListView::new(list_builder, state.aggregated_devices.len())
                .infinite_scrolling(false)
                .scrollbar(default_scrollbar())
                .add_modifier(if state.active_device.is_some() {
                    Modifier::DIM
                } else {
                    Modifier::empty()
                });

            list.render(v[0], frame.buffer_mut(), &mut self.list_state);
        } else {
            PlaceholderWidget::dark_gray("no devices").render(v[0], frame.buffer_mut());
        }

        if self.is_tcp_form_visible {
            PopupInputWidget::new(36).render(v[0], frame.buffer_mut(), &mut self.popup_input_state);
        }

        if self.removing_tcp_device.is_some() {
            PopupConfirmWidget::new(
                "Are you sure to remove this device?",
                "confirm",
                "cancel",
                40,
                Color::Red,
            )
            .render(v[0], frame.buffer_mut());
        }

        if let Some(active_device) = &state.active_device {
            let popup_area = Rect {
                x: v[0].x,
                y: v[0].y + v[0].height / 3,
                width: v[0].width,
                height: v[0].height - v[0].height / 3,
            };

            let popup_block = Block::bordered()
                .border_type(BorderType::Thick)
                .padding(Padding::uniform(1))
                .title(" selected connection ");

            let popup_block_area = popup_block.inner(popup_area);

            frame.render_widget(Clear, popup_area);
            frame.render_widget(popup_block, popup_area);

            let conn_info: Vec<Line> = match &state.connection_state {
                ConnectionState::NotConnected => {
                    vec![Line::from(Span::from("not connected").dark_gray())]
                }
                ConnectionState::ProblemDetected { error, .. } => vec![
                    Line::from(Span::from(" connection problem ").white().on_red()),
                    Line::from(""),
                    Line::from(Span::from(error).dark_gray()),
                ],
                ConnectionState::Connecting => {
                    vec![Line::from(Span::from("connecting...").yellow())]
                }
                ConnectionState::Connected => {
                    vec![Line::from(Span::from("connected").green())]
                }
            };

            let block_v = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(conn_info.len() as u16),
                Constraint::Fill(1),
            ])
            .split(popup_block_area);

            let device_widget = DeviceWidget {
                device: active_device,
                is_selected: false,
                centered: true,
            };

            device_widget.render(block_v[1], frame.buffer_mut());

            frame.render_widget(
                Paragraph::new(conn_info)
                    .alignment(HorizontalAlignment::Center)
                    .wrap(Wrap { trim: false }),
                block_v[3],
            );
        }

        HotkeysWidget::new(&self.get_hotkeys(state)).render(v[1], frame.buffer_mut())
    }
}

struct DeviceWidget<'a> {
    device: &'a Device,
    is_selected: bool,
    centered: bool,
}

impl<'a> Widget for DeviceWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let spans = match self.device {
            Device::Ble { name, .. } => vec![Span::from(" BLE ").black().on_blue(), Span::from(" "), Span::from(name)],
            Device::Tcp(hostaddr) => vec![
                Span::from(" TCP ").black().on_green(),
                Span::from(" "),
                Span::from(hostaddr.to_string()),
            ],
            Device::Serial(address) => vec![
                Span::from(" COM ").black().on_magenta(),
                Span::from(" "),
                Span::from(address),
            ],
        };

        let mut line = Line::from(spans);

        if self.is_selected {
            line = line.reversed();
        }

        if self.centered {
            line = line.centered();
        }

        line.render(area, buf);
    }
}
