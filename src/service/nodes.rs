use std::time::Duration;

use chrono::{DateTime, Utc};
use meshtastic::{
    Message as _,
    protobufs::{AdminMessage, MeshPacket, PortNum, Telemetry, User, admin_message, from_radio, mesh_packet},
};
use tokio::{
    sync::{broadcast, mpsc, watch},
    time,
};
use tokio_graceful_shutdown::SubsystemHandle;

use crate::state::State;
use crate::types::{NodeTelemetry, TelemetryItem, Toast};
use crate::ui::helpers::humanize_uptime;
use crate::{
    meshtastic::types::{CommandToMeshtastic, MeshtasticEvent},
    state::StateAction,
    types::{AppEvent, Node},
};

pub const ONLINE_NODE_THRESHOLD_SECS: i64 = 7200;
const UPDATE_ONLINE_NODES_INTERVAL_SECS: u64 = 2;

pub struct NodesService {
    app_event_tx: broadcast::Sender<AppEvent>,
    app_event_rx: broadcast::Receiver<AppEvent>,
    state_rx: watch::Receiver<State>,
    state_action_tx: mpsc::UnboundedSender<StateAction>,
    meshtastic_command_tx: mpsc::UnboundedSender<CommandToMeshtastic>,
    meshtastic_event_rx: broadcast::Receiver<MeshtasticEvent>,
    local_my_node_num: Option<u32>,
}

impl NodesService {
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
            local_my_node_num: None,
        }
    }

    pub async fn run(mut self, subsys: &mut SubsystemHandle) -> anyhow::Result<()> {
        let mut online_nodes_interval = time::interval(Duration::from_secs(UPDATE_ONLINE_NODES_INTERVAL_SECS));

        loop {
            tokio::select! {
                event = self.app_event_rx.recv() => self.handle_app_event(event)?,
                event = self.meshtastic_event_rx.recv() => self.handle_meshtastic_event(event)?,
                _ = online_nodes_interval.tick() => self.update_online_nodes()?,
                _ = subsys.on_shutdown_requested() => {
                    tracing::info!("shutdown");
                    break;
                }
            }
        }

        Ok(())
    }

    fn handle_app_event(&self, event: Result<AppEvent, broadcast::error::RecvError>) -> anyhow::Result<()> {
        let state = &self.state_rx.borrow();

        match event {
            Ok(app_event) => match app_event {
                AppEvent::DirectChatRequested(node_key) => {
                    self.state_action_tx.send(StateAction::DirectChatStart(node_key))?;
                }
                AppEvent::NodesSortByPrevRequested => {
                    self.state_action_tx
                        .send(StateAction::NodesSortBySet(state.nodes_sort_by.prev()))?;
                }
                AppEvent::NodesSortByNextRequested => {
                    self.state_action_tx
                        .send(StateAction::NodesSortBySet(state.nodes_sort_by.next()))?;
                }
                AppEvent::NodesFilterChanged(filter) => {
                    self.state_action_tx.send(StateAction::NodesFilterSet(filter))?;
                }
                AppEvent::NodeInfoBroadcastRequested => {
                    let my_node = state.get_my_node().expect("should be Some");

                    self.meshtastic_command_tx
                        .send(CommandToMeshtastic::BroadcastNodeInfo {
                            channel_id: 0,
                            user: my_node.try_into()?,
                        })?;
                }
                AppEvent::NodeInfoPopupOpenRequested(node_key) => {
                    self.state_action_tx.send(StateAction::NodeInfoSet(node_key))?;
                }
                AppEvent::NodeInfoPopupCloseRequested => {
                    self.state_action_tx.send(StateAction::NodeInfoUnset)?;
                }
                AppEvent::NodeDeleteRequested(node_num) => {
                    let my_node_num = state.my_node_key.expect("should be Some");

                    self.meshtastic_command_tx
                        .send(CommandToMeshtastic::DeleteNode { node_num, my_node_num })?;
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
        &mut self,
        event: Result<MeshtasticEvent, broadcast::error::RecvError>,
    ) -> anyhow::Result<()> {
        match event {
            Ok(meshtastic_event) => match meshtastic_event {
                MeshtasticEvent::IncomingPacket(packet) => self.handle_meshtastic_packet(packet)?,
                MeshtasticEvent::NodeInfoBroadcastSent => self
                    .state_action_tx
                    .send(StateAction::Toast(Toast::success("NodeInfo broadcast sent")))?,
                MeshtasticEvent::NodeInfoBroadcastFailed(e) => {
                    tracing::error!("NodeInfo broadcast failed: {:?}", e);

                    self.state_action_tx
                        .send(StateAction::Toast(Toast::error("NodeInfo broadcast failed")))?;
                }
                MeshtasticEvent::NodeRemoveAccepted => self
                    .state_action_tx
                    .send(StateAction::Toast(Toast::success("node removed")))?,
                MeshtasticEvent::NodeRemoveFailed(e) => {
                    tracing::error!("node remove failed: {:?}", e);

                    self.state_action_tx
                        .send(StateAction::Toast(Toast::error("node remove failed")))?;
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

    fn handle_meshtastic_packet(&mut self, packet: from_radio::PayloadVariant) -> anyhow::Result<()> {
        match packet {
            from_radio::PayloadVariant::MyInfo(my_info) => {
                self.local_my_node_num = Some(my_info.my_node_num);

                self.state_action_tx
                    .send(StateAction::MyNodeKeySet(my_info.my_node_num))?;
            }
            from_radio::PayloadVariant::NodeInfo(node_info) => {
                match Node::try_from(&node_info) {
                    Ok(node) => {
                        self.state_action_tx.send(StateAction::NodeInit(node))?;
                        self.update_online_nodes()?;
                    }
                    Err(e) => {
                        tracing::debug!(node_key = node_info.num, "can't convert NodeInfo into Node: {}", e);
                    }
                };
            }
            from_radio::PayloadVariant::Packet(mesh_packet) => {
                self.state_action_tx
                    .send(StateAction::NodeInit((&mesh_packet).into()))?;

                match &mesh_packet.payload_variant {
                    Some(mesh_packet::PayloadVariant::Decoded(data)) => match data.portnum() {
                        PortNum::NodeinfoApp => match User::decode(&*data.payload) {
                            Ok(user) => {
                                match Node::try_from((&mesh_packet, &user)) {
                                    Ok(node) => self.state_action_tx.send(StateAction::NodeUpdate(node))?,
                                    Err(e) => {
                                        tracing::debug!(
                                            node_key = mesh_packet.from,
                                            "can't convert NodeInfo into Node: {:?}",
                                            e
                                        );
                                    }
                                };
                            }
                            Err(e) => {
                                tracing::debug!("can't decode NodeinfoApp payload: {:?}", e);
                            }
                        },
                        PortNum::AdminApp => match AdminMessage::decode(&*data.payload) {
                            Ok(admin_message) => match admin_message.payload_variant {
                                Some(admin_message::PayloadVariant::SetOwner(user)) => {
                                    self.state_action_tx.send(StateAction::NodeOwnerSet((&user).into()))?;
                                }
                                Some(admin_message::PayloadVariant::RemoveByNodenum(node_num)) => {
                                    self.state_action_tx.send(StateAction::NodeDelete(node_num))?;
                                }
                                _ => {}
                            },
                            Err(e) => {
                                tracing::debug!("can't decode AdminMessage payload: {:?}", e);
                            }
                        },
                        PortNum::TelemetryApp => match Telemetry::decode(&*data.payload) {
                            Ok(Telemetry {
                                time,
                                variant: Some(data),
                            }) => {
                                let node_telemetry = NodeTelemetry {
                                    node_key: mesh_packet.from,
                                    datetime: DateTime::from_timestamp(time as i64, 0).unwrap_or_else(|| Utc::now()),
                                    variant: data.clone(),
                                };

                                self.app_event_tx
                                    .send(AppEvent::TelemetryArrived(node_telemetry.clone()))?;

                                self.state_action_tx
                                    .send(StateAction::NodeLastTelemetrySet(node_telemetry))?;
                            }
                            Ok(_) => {
                                tracing::debug!("TelemetryApp with empty variant");
                            }
                            Err(e) => {
                                tracing::debug!("can't decode TelemetryApp payload: {:?}", e);
                            }
                        },
                        _ => {}
                    },
                    _ => {}
                }

                self.send_node_update_last_heard(&mesh_packet)?;
            }
            _ => {}
        }

        Ok(())
    }

    fn send_node_update_last_heard(&self, packet: &MeshPacket) -> anyhow::Result<()> {
        self.state_action_tx.send(StateAction::NodeUpdateLastHeard {
            node_key: packet.from,
            hops: packet.hop_start.saturating_sub(packet.hop_limit),
            snr: packet.rx_snr,
            rssi: packet.rx_rssi,
        })?;

        self.update_online_nodes()?;

        Ok(())
    }

    fn update_online_nodes(&self) -> anyhow::Result<()> {
        let state = &self.state_rx.borrow();
        let now = Utc::now();

        let count = state.nodes.iter().fold(0, |mut counter, (_, node)| {
            if let Some(last_heard) = node.last_heard
                && (now - last_heard).num_seconds() < ONLINE_NODE_THRESHOLD_SECS
            {
                counter += 1;
            }

            counter
        });

        self.state_action_tx.send(StateAction::NodesOnlineSet(count))?;

        Ok(())
    }
}

macro_rules! power_metrics {
    ($items:expr, $power_metrics:expr, $($ch:literal), * $(,)?) => {
        $(
            paste::paste! {
                $items.push(TelemetryItem::item(
                    concat!("ch", $ch, "_voltage"),
                    $power_metrics.[<ch $ch _voltage>],
                    $power_metrics.[<ch $ch _voltage>].map(|v| format!("{:.1}V", v)),
                ));

                $items.push(TelemetryItem::item(
                    concat!("ch", $ch, " current"),
                    $power_metrics.[<ch $ch _current>],
                    $power_metrics.[<ch $ch _current>].and_then(|v| Some(format!("{:.1}A", v))),
                ));
            }
        )*
    };
}

pub fn update_nodeinfo_telemetry(state: &mut State) -> bool {
    let Some(node_key) = state.nodeinfo else {
        return false;
    };

    let Some(last_telemetry) = &state.nodes_last_telemetry.get(&node_key) else {
        return false;
    };

    let mut items: Vec<TelemetryItem> = Vec::new();

    // device metrics
    if let Some(device_metrics) = &last_telemetry.device_metrics {
        items.push(TelemetryItem::group(
            "Device Metrics",
            serde_json::to_string_pretty(device_metrics).unwrap_or("serialize failed".to_owned()),
        ));

        items.push(TelemetryItem::item(
            "battery level",
            device_metrics.battery_level,
            device_metrics.battery_level.and_then(|v| Some(format!("{}%", v))),
        ));

        items.push(TelemetryItem::item(
            "voltage",
            device_metrics.voltage,
            device_metrics.voltage.and_then(|v| Some(format!("{:.1}V", v))),
        ));

        items.push(TelemetryItem::item(
            "air util tx",
            device_metrics.air_util_tx,
            device_metrics.air_util_tx.and_then(|v| Some(format!("{:.2}%", v))),
        ));

        items.push(TelemetryItem::item(
            "channel util",
            device_metrics.channel_utilization,
            device_metrics
                .channel_utilization
                .and_then(|v| Some(format!("{:.2}%", v))),
        ));

        items.push(TelemetryItem::item(
            "uptime",
            device_metrics.uptime_seconds,
            device_metrics.uptime_seconds.and_then(|v| Some(humanize_uptime(v))),
        ));
    }

    // environment metrics
    if let Some(environment_metrics) = &last_telemetry.environment_metrics {
        items.push(TelemetryItem::group(
            "Environment Metrics",
            serde_json::to_string_pretty(environment_metrics).unwrap_or("serialize failed".to_owned()),
        ));

        items.push(TelemetryItem::item(
            "temperature",
            environment_metrics.temperature,
            environment_metrics
                .temperature
                .and_then(|v| Some(format!("{:.1}°C", v))),
        ));

        items.push(TelemetryItem::item(
            "relative humidity",
            environment_metrics.relative_humidity,
            environment_metrics
                .relative_humidity
                .and_then(|v| Some(format!("{:.1}%", v))),
        ));

        items.push(TelemetryItem::item(
            "barometric pressure",
            environment_metrics.barometric_pressure,
            environment_metrics
                .barometric_pressure
                .and_then(|v| Some(format!("{:.1}hPA", v))),
        ));

        items.push(TelemetryItem::item(
            "gas resistance",
            environment_metrics.gas_resistance,
            environment_metrics
                .gas_resistance
                .and_then(|v| Some(format!("{:.1}MOhm", v))),
        ));

        items.push(TelemetryItem::item(
            "current",
            environment_metrics.current,
            environment_metrics.current.and_then(|v| Some(format!("{:.1}A", v))),
        ));

        items.push(TelemetryItem::item(
            "voltage",
            environment_metrics.voltage,
            environment_metrics.voltage.and_then(|v| Some(format!("{:.1}V", v))),
        ));

        items.push(TelemetryItem::item(
            "IAQ",
            environment_metrics.iaq,
            environment_metrics.iaq.and_then(|v| Some(v)),
        ));

        items.push(TelemetryItem::item(
            "distance",
            environment_metrics.distance,
            environment_metrics.distance.and_then(|v| Some(format!("{:.3}mm", v))),
        ));

        items.push(TelemetryItem::item(
            "lux",
            environment_metrics.lux,
            environment_metrics.lux.and_then(|v| Some(format!("{:.2}", v))),
        ));

        items.push(TelemetryItem::item(
            "white lux",
            environment_metrics.white_lux,
            environment_metrics.white_lux.and_then(|v| Some(format!("{:.2}", v))),
        ));

        items.push(TelemetryItem::item(
            "infrared lux",
            environment_metrics.ir_lux,
            environment_metrics.ir_lux.and_then(|v| Some(format!("{:.2}", v))),
        ));

        items.push(TelemetryItem::item(
            "ultraviolet lux",
            environment_metrics.uv_lux,
            environment_metrics.uv_lux.and_then(|v| Some(format!("{:.2}", v))),
        ));

        items.push(TelemetryItem::item(
            "wind direction",
            environment_metrics.wind_direction,
            environment_metrics.wind_direction.and_then(|v| Some(format!("{}°", v))),
        ));

        items.push(TelemetryItem::item(
            "wind speed",
            environment_metrics.wind_speed,
            environment_metrics
                .wind_speed
                .and_then(|v| Some(format!("{:.2}m/s", v))),
        ));

        items.push(TelemetryItem::item(
            "wind gust",
            environment_metrics.wind_gust,
            environment_metrics.wind_gust.and_then(|v| Some(format!("{:.2}m/s", v))),
        ));

        items.push(TelemetryItem::item(
            "wind lull",
            environment_metrics.wind_lull,
            environment_metrics.wind_lull.and_then(|v| Some(format!("{:.2}m/s", v))),
        ));

        items.push(TelemetryItem::item(
            "weight",
            environment_metrics.weight,
            environment_metrics.weight.and_then(|v| Some(format!("{:.3}kg", v))),
        ));

        items.push(TelemetryItem::item(
            "radiation",
            environment_metrics.radiation,
            environment_metrics
                .radiation
                .and_then(|v| Some(format!("{:.3}µR/h", v))),
        ));

        items.push(TelemetryItem::item(
            "rainfall 1h",
            environment_metrics.rainfall_1h,
            environment_metrics
                .rainfall_1h
                .and_then(|v| Some(format!("{:.1}mm", v))),
        ));

        items.push(TelemetryItem::item(
            "rainfall 24h",
            environment_metrics.rainfall_24h,
            environment_metrics
                .rainfall_24h
                .and_then(|v| Some(format!("{:.1}mm", v))),
        ));

        items.push(TelemetryItem::item(
            "soil moisture",
            environment_metrics.soil_moisture,
            environment_metrics.soil_moisture.and_then(|v| Some(format!("{}%", v))),
        ));

        items.push(TelemetryItem::item(
            "soil temperature",
            environment_metrics.soil_temperature,
            environment_metrics
                .soil_temperature
                .and_then(|v| Some(format!("{:.1}°C", v))),
        ));
    }

    // air quality metrics
    if let Some(air_quality_metrics) = &last_telemetry.air_quality_metrics {
        items.push(TelemetryItem::group(
            "Air Quality Metrics",
            serde_json::to_string_pretty(air_quality_metrics).unwrap_or("serialize failed".to_owned()),
        ));

        items.push(TelemetryItem::item(
            "PM1.0 standard",
            air_quality_metrics.pm10_standard,
            air_quality_metrics
                .pm10_standard
                .and_then(|v| Some(format!("{}µg/m³", v))),
        ));

        items.push(TelemetryItem::item(
            "PM2.5 standard",
            air_quality_metrics.pm25_standard,
            air_quality_metrics
                .pm25_standard
                .and_then(|v| Some(format!("{}µg/m³", v))),
        ));

        items.push(TelemetryItem::item(
            "PM10 standard",
            air_quality_metrics.pm100_standard,
            air_quality_metrics
                .pm100_standard
                .and_then(|v| Some(format!("{}µg/m³", v))),
        ));

        items.push(TelemetryItem::item(
            "PM1.0 environmental",
            air_quality_metrics.pm10_environmental,
            air_quality_metrics
                .pm10_environmental
                .and_then(|v| Some(format!("{}µg/m³", v))),
        ));

        items.push(TelemetryItem::item(
            "PM2.5 environmental",
            air_quality_metrics.pm25_environmental,
            air_quality_metrics
                .pm25_environmental
                .and_then(|v| Some(format!("{}µg/m³", v))),
        ));

        items.push(TelemetryItem::item(
            "PM10 environmental",
            air_quality_metrics.pm100_environmental,
            air_quality_metrics
                .pm100_environmental
                .and_then(|v| Some(format!("{}µg/m³", v))),
        ));

        items.push(TelemetryItem::item(
            "0.3µm particles",
            air_quality_metrics.particles_03um,
            air_quality_metrics
                .particles_03um
                .and_then(|v| Some(format!("{}/0.1L", v))),
        ));

        items.push(TelemetryItem::item(
            "0.5µm particles",
            air_quality_metrics.particles_05um,
            air_quality_metrics
                .particles_05um
                .and_then(|v| Some(format!("{}/0.1L", v))),
        ));

        items.push(TelemetryItem::item(
            "1.0µm particles",
            air_quality_metrics.particles_10um,
            air_quality_metrics
                .particles_10um
                .and_then(|v| Some(format!("{}/0.1L", v))),
        ));

        items.push(TelemetryItem::item(
            "2.5µm particles",
            air_quality_metrics.particles_25um,
            air_quality_metrics
                .particles_25um
                .and_then(|v| Some(format!("{}/0.1L", v))),
        ));

        items.push(TelemetryItem::item(
            "5.0µm particles",
            air_quality_metrics.particles_50um,
            air_quality_metrics
                .particles_50um
                .and_then(|v| Some(format!("{}/0.1L", v))),
        ));

        items.push(TelemetryItem::item(
            "10.0µm particles",
            air_quality_metrics.particles_100um,
            air_quality_metrics
                .particles_100um
                .and_then(|v| Some(format!("{}/0.1L", v))),
        ));

        items.push(TelemetryItem::item(
            "CO2",
            air_quality_metrics.co2,
            air_quality_metrics.co2.and_then(|v| Some(format!("{}ppm", v))),
        ));

        items.push(TelemetryItem::item(
            "CO2 temperature",
            air_quality_metrics.co2_temperature,
            air_quality_metrics
                .co2_temperature
                .and_then(|v| Some(format!("{:.1}°C", v))),
        ));

        items.push(TelemetryItem::item(
            "CO2 humidity",
            air_quality_metrics.co2_humidity,
            air_quality_metrics
                .co2_humidity
                .and_then(|v| Some(format!("{:.1}%", v))),
        ));

        items.push(TelemetryItem::item(
            "formaldehyde",
            air_quality_metrics.form_formaldehyde,
            air_quality_metrics
                .form_formaldehyde
                .and_then(|v| Some(format!("{:.1}ppb", v))),
        ));

        items.push(TelemetryItem::item(
            "formaldehyde humidity",
            air_quality_metrics.form_humidity,
            air_quality_metrics
                .form_humidity
                .and_then(|v| Some(format!("{:.1}%RH", v))),
        ));

        items.push(TelemetryItem::item(
            "formaldehyde temperature",
            air_quality_metrics.form_temperature,
            air_quality_metrics
                .form_temperature
                .and_then(|v| Some(format!("{:.1}°C", v))),
        ));

        items.push(TelemetryItem::item(
            "PM4.0 standard",
            air_quality_metrics.pm40_standard,
            air_quality_metrics
                .pm40_standard
                .and_then(|v| Some(format!("{}µg/m³", v))),
        ));

        items.push(TelemetryItem::item(
            "4.0µm particles",
            air_quality_metrics.particles_40um,
            air_quality_metrics
                .particles_40um
                .and_then(|v| Some(format!("{}/0.1L", v))),
        ));

        items.push(TelemetryItem::item(
            "PM temperature",
            air_quality_metrics.pm_temperature,
            air_quality_metrics
                .pm_temperature
                .and_then(|v| Some(format!("{:.1}°C", v))),
        ));

        items.push(TelemetryItem::item(
            "PM humidity",
            air_quality_metrics.pm_humidity,
            air_quality_metrics.pm_humidity.and_then(|v| Some(format!("{:.1}%", v))),
        ));

        items.push(TelemetryItem::item(
            "PM VOC index",
            air_quality_metrics.pm_voc_idx,
            air_quality_metrics.pm_voc_idx.and_then(|v| Some(format!("{:.2}", v))),
        ));

        items.push(TelemetryItem::item(
            "PM NOx index",
            air_quality_metrics.pm_nox_idx,
            air_quality_metrics.pm_nox_idx.and_then(|v| Some(format!("{:.2}", v))),
        ));

        items.push(TelemetryItem::item(
            "typical particle size",
            air_quality_metrics.particles_tps,
            air_quality_metrics
                .particles_tps
                .and_then(|v| Some(format!("{:.2}µm", v))),
        ));
    }

    // host metrics
    if let Some(host_metrics) = &last_telemetry.host_metrics {
        items.push(TelemetryItem::group(
            "Host Metrics",
            serde_json::to_string_pretty(host_metrics).unwrap_or("serialize failed".to_owned()),
        ));

        items.push(TelemetryItem::item(
            "uptime",
            Some(host_metrics.uptime_seconds),
            Some(humanize_uptime(host_metrics.uptime_seconds)),
        ));

        items.push(TelemetryItem::item(
            "free memory",
            Some(host_metrics.freemem_bytes),
            Some(format!("{}MB", host_metrics.freemem_bytes / 1024)),
        ));

        items.push(TelemetryItem::item(
            "disk 1 free space",
            Some(host_metrics.diskfree1_bytes),
            Some(format!("{}MB", host_metrics.diskfree1_bytes / 1024)),
        ));

        items.push(TelemetryItem::item(
            "disk 2 free space",
            host_metrics.diskfree2_bytes,
            host_metrics
                .diskfree2_bytes
                .and_then(|b| Some(format!("{}MB", b / 1024))),
        ));

        items.push(TelemetryItem::item(
            "disk 3 free space",
            host_metrics.diskfree3_bytes,
            host_metrics
                .diskfree3_bytes
                .and_then(|b| Some(format!("{}MB", b / 1024))),
        ));

        items.push(TelemetryItem::item(
            "load 1 min",
            Some(host_metrics.load1),
            Some(host_metrics.load1),
        ));

        items.push(TelemetryItem::item(
            "load 5 min",
            Some(host_metrics.load5),
            Some(host_metrics.load5),
        ));

        items.push(TelemetryItem::item(
            "load 15 min",
            Some(host_metrics.load15),
            Some(host_metrics.load15),
        ));

        items.push(TelemetryItem::item(
            "user string",
            host_metrics.user_string.as_ref(),
            host_metrics.user_string.as_ref(),
        ));
    }

    // power metrics
    if let Some(power_metrics) = &last_telemetry.power_metrics {
        items.push(TelemetryItem::group(
            "Power Metrics",
            serde_json::to_string_pretty(power_metrics).unwrap_or("serialize failed".to_owned()),
        ));

        power_metrics!(items, power_metrics, "1", "2", "3", "4", "5", "6", "7", "8");
    }

    // local stats
    if let Some(local_stats) = &last_telemetry.local_stats {
        items.push(TelemetryItem::group(
            "Local Stats",
            serde_json::to_string_pretty(local_stats).unwrap_or("serialize failed".to_owned()),
        ));

        items.push(TelemetryItem::item(
            "uptime",
            Some(local_stats.uptime_seconds),
            Some(humanize_uptime(local_stats.uptime_seconds)),
        ));

        items.push(TelemetryItem::item(
            "channel utilization",
            Some(local_stats.channel_utilization),
            Some(format!("{:.2}%", local_stats.channel_utilization)),
        ));

        items.push(TelemetryItem::item(
            "air util tx",
            Some(local_stats.air_util_tx),
            Some(format!("{:.2}%", local_stats.air_util_tx)),
        ));

        items.push(TelemetryItem::item(
            "packets tx",
            Some(local_stats.num_packets_tx),
            Some(format!("{}", local_stats.num_packets_tx)),
        ));

        items.push(TelemetryItem::item(
            "packets rx",
            Some(local_stats.num_packets_rx),
            Some(format!("{}", local_stats.num_packets_rx)),
        ));

        items.push(TelemetryItem::item(
            "packets rx bad",
            Some(local_stats.num_packets_rx_bad),
            Some(format!("{}", local_stats.num_packets_rx_bad)),
        ));

        items.push(TelemetryItem::item(
            "online nodes",
            Some(local_stats.num_online_nodes),
            Some(format!("{}", local_stats.num_online_nodes)),
        ));

        items.push(TelemetryItem::item(
            "total nodes",
            Some(local_stats.num_total_nodes),
            Some(format!("{}", local_stats.num_total_nodes)),
        ));

        items.push(TelemetryItem::item(
            "rx dupe",
            Some(local_stats.num_rx_dupe),
            Some(format!("{}", local_stats.num_rx_dupe)),
        ));

        items.push(TelemetryItem::item(
            "tx relay",
            Some(local_stats.num_tx_relay),
            Some(format!("{}", local_stats.num_tx_relay)),
        ));

        items.push(TelemetryItem::item(
            "tx relay canceled",
            Some(local_stats.num_tx_relay_canceled),
            Some(format!("{}", local_stats.num_tx_relay_canceled)),
        ));

        items.push(TelemetryItem::item(
            "heap total",
            Some(local_stats.heap_total_bytes),
            Some(format!("{}KB", local_stats.heap_total_bytes / 1024)),
        ));

        items.push(TelemetryItem::item(
            "heap free",
            Some(local_stats.heap_free_bytes),
            Some(format!("{}KB", local_stats.heap_free_bytes / 1024)),
        ));

        items.push(TelemetryItem::item(
            "tx dropped",
            Some(local_stats.num_tx_dropped),
            Some(format!("{}", local_stats.num_tx_dropped)),
        ));

        items.push(TelemetryItem::item(
            "noise floor",
            Some(local_stats.noise_floor),
            Some(format!("{}dBm", local_stats.noise_floor)),
        ));
    }

    // health metrics
    if let Some(health_metrics) = &last_telemetry.health_metrics {
        items.push(TelemetryItem::group(
            "Health Metrics",
            serde_json::to_string_pretty(health_metrics).unwrap_or("serialize failed".to_owned()),
        ));

        items.push(TelemetryItem::item(
            "heart rate",
            health_metrics.heart_bpm,
            health_metrics.heart_bpm.and_then(|v| Some(format!("{}bpm", v))),
        ));

        items.push(TelemetryItem::item(
            "SpO2",
            health_metrics.sp_o2,
            health_metrics.sp_o2.and_then(|v| Some(format!("{}%", v))),
        ));

        items.push(TelemetryItem::item(
            "body temperature",
            health_metrics.temperature,
            health_metrics.temperature.and_then(|v| Some(format!("{:.1}°C", v))),
        ));
    }

    // traffic management stats
    if let Some(traffic_management_stats) = &last_telemetry.traffic_management_stats {
        items.push(TelemetryItem::group(
            "Traffic Management Stats",
            serde_json::to_string_pretty(traffic_management_stats).unwrap_or("serialize failed".to_owned()),
        ));

        items.push(TelemetryItem::item(
            "packets inspected",
            Some(traffic_management_stats.packets_inspected),
            Some(format!("{}", traffic_management_stats.packets_inspected)),
        ));

        items.push(TelemetryItem::item(
            "position dedup drops",
            Some(traffic_management_stats.position_dedup_drops),
            Some(format!("{}", traffic_management_stats.position_dedup_drops)),
        ));

        items.push(TelemetryItem::item(
            "nodeinfo cache hits",
            Some(traffic_management_stats.nodeinfo_cache_hits),
            Some(format!("{}", traffic_management_stats.nodeinfo_cache_hits)),
        ));

        items.push(TelemetryItem::item(
            "rate limit drops",
            Some(traffic_management_stats.rate_limit_drops),
            Some(format!("{}", traffic_management_stats.rate_limit_drops)),
        ));

        items.push(TelemetryItem::item(
            "unknown packet drops",
            Some(traffic_management_stats.unknown_packet_drops),
            Some(format!("{}", traffic_management_stats.unknown_packet_drops)),
        ));

        items.push(TelemetryItem::item(
            "hop exhausted packets",
            Some(traffic_management_stats.hop_exhausted_packets),
            Some(format!("{}", traffic_management_stats.hop_exhausted_packets)),
        ));

        items.push(TelemetryItem::item(
            "router hops preserved",
            Some(traffic_management_stats.router_hops_preserved),
            Some(format!("{}", traffic_management_stats.router_hops_preserved)),
        ));
    }

    state.nodeinfo_telemetry = items
        .into_iter()
        .filter(|item| match item {
            TelemetryItem::Group { .. } => true,
            TelemetryItem::Item { value, .. } => value.is_some(),
        })
        .collect();

    true
}
