use std::net::IpAddr;
use std::time::{Duration, Instant};

use hostaddr::HostAddr;
use meshtastic::protobufs::from_radio;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::{join, time};
use tokio_graceful_shutdown::SubsystemHandle;

use crate::state::State;
use crate::types::{AppEvent, Channel, ConnectionState, Device, Toast};
use crate::{
    meshtastic::types::{CommandToMeshtastic, MeshtasticEvent},
    state::StateAction,
};

const CONNECTION_CHECK_INTERVAL_MILLIS: u64 = 250;
const RECONNECTION_BACKOFF_BASE_MILLIS: u64 = 1_000;
const RECONNECTION_BACKOFF_MAX_MILLIS: u64 = 30_000;
const BLE_DISCOVERY_TIMEOUT_SECS: u64 = 3;
const MDNS_DISCOVERY_TIMEOUT_SECS: u64 = 3;
const MDNS_MESHTASTIC_DOMAIN: &'static str = "_meshtastic._tcp.local.";

pub struct ConnectionService {
    app_event_tx: broadcast::Sender<AppEvent>,
    app_event_rx: broadcast::Receiver<AppEvent>,
    state_rx: watch::Receiver<State>,
    state_action_tx: mpsc::UnboundedSender<StateAction>,
    meshtastic_command_tx: mpsc::UnboundedSender<CommandToMeshtastic>,
    meshtastic_event_rx: broadcast::Receiver<MeshtasticEvent>,
    mdns_daemon: Option<mdns_sd::ServiceDaemon>,
}

impl ConnectionService {
    pub fn new(
        app_event_tx: broadcast::Sender<AppEvent>,
        app_event_rx: broadcast::Receiver<AppEvent>,
        state_rx: watch::Receiver<State>,
        state_action_tx: mpsc::UnboundedSender<StateAction>,
        meshtastic_command_tx: mpsc::UnboundedSender<CommandToMeshtastic>,
        meshtastic_event_rx: broadcast::Receiver<MeshtasticEvent>,
    ) -> Self {
        Self {
            app_event_tx,
            app_event_rx,
            state_rx,
            state_action_tx,
            meshtastic_command_tx,
            meshtastic_event_rx,
            mdns_daemon: None,
        }
    }

    pub async fn run(mut self, subsys: &mut SubsystemHandle) -> anyhow::Result<()> {
        let mut connection_check_interval = time::interval(Duration::from_millis(CONNECTION_CHECK_INTERVAL_MILLIS));

        self.mdns_daemon = mdns_sd::ServiceDaemon::new()
            .map_err(|e| tracing::error!("can't start mDNS daemon: {}", e))
            .ok();

        loop {
            tokio::select! {
                event = self.app_event_rx.recv() => self.handle_app_event(event).await?,
                event = self.meshtastic_event_rx.recv() => self.handle_meshtastic_event(event)?,
                _ = connection_check_interval.tick() => self.check_connection()?,
                _ = subsys.on_shutdown_requested() => {
                    tracing::info!("shutdown");

                    if let Some(daemon) = self.mdns_daemon.take() {
                        let _ = daemon.shutdown();
                    }

                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_app_event(&mut self, event: Result<AppEvent, broadcast::error::RecvError>) -> anyhow::Result<()> {
        match event {
            Ok(app_event) => match app_event {
                AppEvent::DeviceSelected(hardware) => {
                    self.state_action_tx.send(StateAction::DeviceActiveSet(hardware))?;
                }
                AppEvent::DisconnectionRequested => {
                    self.meshtastic_command_tx.send(CommandToMeshtastic::Disconnect)?;
                }
                AppEvent::DeviceRediscoverRequested => {
                    self.state_action_tx.send(StateAction::DeviceDiscoveringStart)?;

                    self.state_action_tx
                        .send(StateAction::Toast(Toast::normal("discovering...")))?;

                    match self.discover_devices().await {
                        Ok(_) => {
                            self.state_action_tx.send(StateAction::DeviceDiscoveringDone)?;

                            self.state_action_tx
                                .send(StateAction::Toast(Toast::normal("discovery done")))?;
                        }
                        Err(e) => {
                            tracing::error!("device discovering failed: {}", e);

                            self.state_action_tx
                                .send(StateAction::DeviceDiscoveringFail(e.to_string()))?;

                            self.state_action_tx
                                .send(StateAction::Toast(Toast::error("discovery failed")))?;
                        }
                    };
                }
                AppEvent::DeviceRebootRequested => {
                    let state = &self.state_rx.borrow();

                    self.meshtastic_command_tx.send(CommandToMeshtastic::Reboot {
                        my_node_num: state.my_node_key.expect("should be Some"),
                        secs: 3,
                    })?;
                }
                AppEvent::DeviceShutdownRequested => {
                    let state = &self.state_rx.borrow();

                    self.meshtastic_command_tx.send(CommandToMeshtastic::Shutdown {
                        my_node_num: state.my_node_key.expect("should be Some"),
                        secs: 3,
                    })?;
                }
                AppEvent::DeviceSubmitted(device) => {
                    self.state_action_tx.send(StateAction::DevicesAdd(device))?;
                }
                AppEvent::DeviceRemoveRequested(device) => {
                    self.state_action_tx.send(StateAction::DevicesRemove(device))?;
                }
                _ => {}
            },
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("broadcast receiver lagged by {} events", n);
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_meshtastic_event(
        &self,
        event: Result<MeshtasticEvent, broadcast::error::RecvError>,
    ) -> anyhow::Result<()> {
        match event {
            Ok(meshtastic_event) => match meshtastic_event {
                MeshtasticEvent::Connected => {
                    tracing::info!("successfully connected");

                    self.state_action_tx
                        .send(StateAction::Toast(Toast::normal("loading data...")))?;
                }
                MeshtasticEvent::ConnectionError(e) => {
                    self.state_action_tx.send(StateAction::ConnectionFail(e))?;
                }
                MeshtasticEvent::Disconnected => {
                    tracing::info!("disconnected");

                    self.state_action_tx.send(StateAction::ConnectionStop)?;

                    self.state_action_tx
                        .send(StateAction::Toast(Toast::normal("disconnected")))?;
                }
                MeshtasticEvent::IncomingPacket(packet) => {
                    match &packet {
                        from_radio::PayloadVariant::MyInfo(my_info) => {
                            self.state_action_tx
                                .send(StateAction::MyNodeKeySet(my_info.my_node_num))?;

                            self.app_event_tx.send(AppEvent::DbLoadRequested(my_info.my_node_num))?;
                        }
                        from_radio::PayloadVariant::Channel(ch) => {
                            self.state_action_tx
                                .send(StateAction::ChannelSet(ch.index as u32, Channel::from(ch)))?;
                        }
                        from_radio::PayloadVariant::ConfigCompleteId(_) => {
                            let state = &self.state_rx.borrow();

                            self.state_action_tx.send(StateAction::ConnectionSuccess)?;

                            self.state_action_tx
                                .send(StateAction::Toast(Toast::success("connected")))?;

                            self.meshtastic_command_tx
                                .send(CommandToMeshtastic::LoadCannedMessages {
                                    my_node_num: state.my_node_key.expect("should be Some"),
                                })?;
                        }
                        from_radio::PayloadVariant::Rebooted(true) => {
                            self.state_action_tx
                                .send(StateAction::Toast(Toast::success("device has been rebooted")))?;
                        }
                        _ => {}
                    }

                    self.state_action_tx.send(StateAction::RxTrigger)?;

                    if let from_radio::PayloadVariant::Packet(p) = packet {
                        let state = &self.state_rx.borrow();
                        let from = state.nodes.get(&p.from).and_then(|n| Some(n.short_name()));
                        let to = state.nodes.get(&p.to).and_then(|n| Some(n.short_name()));

                        tracing::debug!("PACKET from=\"{:?}\" to=\"{:?}\": {:?}", from, to, p);
                    } else {
                        tracing::debug!("PACKET {:?}", packet);
                    }
                }
                _ => {}
            },
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("broadcast receiver lagged by {} events", n);
            }
            _ => {}
        }

        Ok(())
    }

    fn check_connection(&self) -> anyhow::Result<()> {
        let state = &self.state_rx.borrow();

        match (&state.active_device, &state.connection_state) {
            (Some(device), ConnectionState::ProblemDetected { since, .. }) => {
                let backoff_duration = Duration::from_millis(
                    (RECONNECTION_BACKOFF_BASE_MILLIS * 2_u64.saturating_pow(state.connection_attempt))
                        .min(RECONNECTION_BACKOFF_MAX_MILLIS),
                );

                let time_left = backoff_duration.saturating_sub(Instant::now().duration_since(*since));

                self.state_action_tx
                    .send(StateAction::ReconnectionBackoffSet(time_left))?;

                if time_left.is_zero() {
                    self.connect(device)?;
                }
            }
            (Some(device), ConnectionState::NotConnected) => self.connect(device)?,
            _ => {}
        }

        Ok(())
    }

    fn connect(&self, device: &Device) -> anyhow::Result<()> {
        self.state_action_tx.send(StateAction::ConnectionStart)?;

        match device {
            Device::Tcp(address) => self
                .meshtastic_command_tx
                .send(CommandToMeshtastic::ConnectViaTcp(address.clone()))?,
            Device::Ble(address, name) => self
                .meshtastic_command_tx
                .send(CommandToMeshtastic::ConnectViaBle(address.clone(), name.clone()))?,
            Device::Serial(address) => self
                .meshtastic_command_tx
                .send(CommandToMeshtastic::ConnectViaSerial(address.to_owned()))?,
        };

        Ok(())
    }

    async fn discover_devices(&self) -> anyhow::Result<()> {
        // Serial
        match meshtastic::utils::stream::available_serial_ports() {
            Ok(ports) => {
                let serial_devices = ports.iter().map(|port| Device::Serial(port.to_owned()));

                for device in serial_devices.into_iter() {
                    self.state_action_tx.send(StateAction::DevicesDiscoveredAdd(device))?;
                }
            }
            Err(e) => {
                tracing::error!("can't fetch serial ports: {}", e);
            }
        };

        let _ = join!(self.discover_ble_devices(), self.discover_tcp_devices());

        Ok(())
    }

    async fn discover_ble_devices(&self) -> anyhow::Result<()> {
        match meshtastic::utils::stream::available_ble_devices(Duration::from_secs(BLE_DISCOVERY_TIMEOUT_SECS)).await {
            Ok(devices) => {
                let ble_devices = devices
                    .iter()
                    .map(|device| Device::Ble(device.mac_address, device.name.clone()));

                for device in ble_devices.into_iter() {
                    self.state_action_tx.send(StateAction::DevicesDiscoveredAdd(device))?;
                }
            }
            Err(e) => {
                tracing::error!("can't fetch BLE devices: {}", e);
            }
        };

        Ok(())
    }

    async fn discover_tcp_devices(&self) -> anyhow::Result<()> {
        if let Some(mdns_daemon) = &self.mdns_daemon {
            match mdns_daemon.browse(MDNS_MESHTASTIC_DOMAIN) {
                Ok(receiver) => {
                    let now = Instant::now();

                    while let Ok(event) = receiver.recv() {
                        match event {
                            mdns_sd::ServiceEvent::ServiceResolved(info) => {
                                for addr in info.addresses.iter() {
                                    match addr {
                                        mdns_sd::ScopedIp::V4(ipv4) => {
                                            self.state_action_tx.send(StateAction::DevicesDiscoveredAdd(
                                                Device::Tcp(
                                                    HostAddr::from_ip_addr(IpAddr::from(*ipv4.addr()))
                                                        .with_port(info.port),
                                                ),
                                            ))?;
                                        }
                                        mdns_sd::ScopedIp::V6(ipv6) => {
                                            self.state_action_tx.send(StateAction::DevicesDiscoveredAdd(
                                                Device::Tcp(
                                                    HostAddr::from_ip_addr(IpAddr::from(*ipv6.addr()))
                                                        .with_port(info.port),
                                                ),
                                            ))?;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            _ => {
                                if now.elapsed().as_secs() > MDNS_DISCOVERY_TIMEOUT_SECS {
                                    tracing::debug!("mDNS timeout, stopping discovery");
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("mDNS browse error: {}", e);
                }
            };
        }

        Ok(())
    }
}
