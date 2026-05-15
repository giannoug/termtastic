use chrono::Utc;
use meshtastic::protobufs::{config, module_config};
use nameof::name_of;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tokio::{
    sync::{broadcast, mpsc, watch},
    time,
};
use tokio_graceful_shutdown::SubsystemHandle;

use crate::types::ChannelRole;
use crate::{
    state::{State, StateAction},
    types::{Channel, ConnectionState, DeviceDiscoveringState, FormItemKey, SettingsFormState, Tab},
};

const TICK_INTERVAL_MILLIS: u64 = 33;
const RX_TIMEOUT_MILLIS: u128 = 250;
const TOAST_QUICK_TIMEOUT_MILLIS: u128 = 500;
const SPLASH_LOGO_TIMEOUT_MILLIS: u128 = 1500;

pub struct Store {
    state_action_rx: mpsc::UnboundedReceiver<StateAction>,
    state_tx: watch::Sender<State>,
    changed_tx: broadcast::Sender<Vec<&'static str>>,
}

impl Store {
    pub fn new(
        initial_state: State,
    ) -> (
        Self,
        mpsc::UnboundedSender<StateAction>,
        watch::Receiver<State>,
        broadcast::Receiver<Vec<&'static str>>,
    ) {
        let (state_action_tx, state_action_rx) = mpsc::unbounded_channel::<StateAction>();
        let (state_tx, state_rx) = watch::channel(initial_state);
        let (changed_tx, changed_rx) = broadcast::channel::<Vec<&'static str>>(1000);

        (
            Self {
                state_action_rx,
                state_tx,
                changed_tx,
            },
            state_action_tx,
            state_rx,
            changed_rx,
        )
    }

    pub async fn run(mut self, subsys: &mut SubsystemHandle) -> anyhow::Result<()> {
        let mut tick_interval = time::interval(Duration::from_millis(TICK_INTERVAL_MILLIS));

        loop {
            tokio::select! {
                Some(action) = self.state_action_rx.recv() => self.handle_action(action)?,
                _ = tick_interval.tick() => self.handle_tick()?,
                _ = subsys.on_shutdown_requested() => {
                    tracing::info!("shutdown");
                    break;
                }
            }
        }

        Ok(())
    }

    fn handle_action(&mut self, action: StateAction) -> anyhow::Result<()> {
        let mut changed: Vec<&'static str> = Vec::new();

        match action {
            StateAction::SplashLogo => {
                self.state_tx.send_modify(|state| {
                    state.splash_logo = true;
                    state.splash_logo_t = Instant::now();

                    changed.extend(vec![name_of!(splash_logo in State), name_of!(splash_logo_t in State)]);
                });
            }
            StateAction::AppConfigApply(cfg) => {
                self.state_tx.send_modify(|state| {
                    state.active_tab = cfg.active_tab;
                    state.active_device = cfg.active_device;
                    state.tcp_devices = cfg.tcp_devices;
                    state.nodes_sort_by = cfg.nodes_sort_by;
                    state.nodes_filter = cfg.nodes_filter;
                    state.ui_config = cfg.ui_config;
                    state.my_node_key = cfg.my_node_key;
                    state.config_loaded = true;
                    state.update_aggregated_devices();

                    changed.extend(vec![
                        name_of!(active_tab in State),
                        name_of!(active_device in State),
                        name_of!(tcp_devices in State),
                        name_of!(nodes_sort_by in State),
                        name_of!(nodes_filter in State),
                        name_of!(ui_config in State),
                        name_of!(my_node_key in State),
                        name_of!(config_loaded in State),
                        name_of!(aggregated_devices in State),
                    ]);
                });
            }
            StateAction::TabSwitchToNext => {
                self.state_tx.send_modify(|state| {
                    state.active_tab = state.active_tab.next();
                    state.need_clear_frame = true;

                    changed.extend(vec![name_of!(active_tab in State), name_of!(need_clear_frame in State)]);
                });
            }
            StateAction::TabSwitchToPrevious => {
                self.state_tx.send_modify(|state| {
                    state.active_tab = state.active_tab.prev();
                    state.need_clear_frame = true;

                    changed.extend(vec![name_of!(active_tab in State), name_of!(need_clear_frame in State)]);
                });
            }
            StateAction::DeviceActiveSet(device) => {
                self.state_tx.send_modify(|state| {
                    state.active_device = Some(device);

                    changed.push(name_of!(active_device in State));
                });
            }
            StateAction::ConnectionStart => {
                self.state_tx.send_modify(|state| {
                    state.connection_state = ConnectionState::Connecting;
                    state.connection_attempt += 1;
                    state.reconnection_backoff = None;

                    changed.extend(vec![
                        name_of!(connection_state in State),
                        name_of!(connection_attempt in State),
                        name_of!(reconnection_backoff in State),
                    ]);

                    tracing::debug!("connection attempt #{}", state.connection_attempt);
                });
            }
            StateAction::ConnectionFail(error) => {
                self.state_tx.send_modify(|state| {
                    state.connection_state = ConnectionState::ProblemDetected {
                        since: Instant::now(),
                        error,
                    };

                    changed.push(name_of!(connection_state in State));
                });
            }
            StateAction::ConnectionStop => {
                self.state_tx.send_modify(|state| {
                    state.active_device = None;
                    state.channels.clear();
                    state.connection_attempt = 0;
                    state.connection_state = Default::default();
                    state.device_config = Default::default();
                    state.device_module_config = Default::default();
                    state.device_user = Default::default();
                    state.online_nodes = 0;
                    state.reconnection_backoff = None;
                    state.settings_form_data = None;
                    state.settings_form_is_changed = false;
                    state.settings_form_original_data = None;
                    state.settings_form_state = Default::default();

                    changed.extend(vec![
                        name_of!(active_device in State),
                        name_of!(channels in State),
                        name_of!(connection_attempt in State),
                        name_of!(connection_state in State),
                        name_of!(device_config in State),
                        name_of!(device_module_config in State),
                        name_of!(device_user in State),
                        name_of!(online_nodes in State),
                        name_of!(reconnection_backoff in State),
                        name_of!(settings_form_data in State),
                        name_of!(settings_form_is_changed in State),
                        name_of!(settings_form_original_data in State),
                        name_of!(settings_form_state in State),
                    ]);
                });
            }
            StateAction::ConnectionSuccess => {
                self.state_tx.send_modify(|state| {
                    state.connection_state = ConnectionState::Connected;
                    state.connection_attempt = 0;
                    state.reconnection_backoff = None;

                    changed.extend(vec![
                        name_of!(connection_state in State),
                        name_of!(connection_attempt in State),
                        name_of!(reconnection_backoff in State),
                    ]);
                });
            }
            StateAction::DbDataApply { nodes } => {
                self.state_tx.send_modify(|state| {
                    state.nodes = nodes.into_iter().map(|n| (n.key, n)).collect();
                    state.db_loaded = true;

                    changed.extend(vec![name_of!(nodes in State), name_of!(db_loaded in State)]);
                });
            }
            StateAction::ReconnectionBackoffSet(duration) => {
                self.state_tx.send_modify(|state| {
                    state.reconnection_backoff = Some(duration);

                    changed.push(name_of!(reconnection_backoff in State));
                });
            }
            StateAction::LogRecordAdd(r) => {
                self.state_tx.send_modify(|state| {
                    state.logs.push(r);

                    changed.push(name_of!(logs in State));
                });
            }
            StateAction::DeviceDiscoveringStart => {
                self.state_tx.send_modify(|state| {
                    state.device_discovering_state = DeviceDiscoveringState::Discovering;

                    changed.push(name_of!(device_discovering_state in State));
                });
            }
            StateAction::DeviceDiscoveringFail(error) => {
                self.state_tx.send_modify(|state| {
                    state.device_discovering_state = DeviceDiscoveringState::Failed(error);

                    changed.push(name_of!(device_discovering_state in State));
                });
            }
            StateAction::DeviceDiscoveringDone(devices) => {
                self.state_tx.send_modify(|state| {
                    state.discovered_devices = devices;
                    state.device_discovering_state = DeviceDiscoveringState::Done;
                    state.update_aggregated_devices();

                    changed.extend(vec![
                        name_of!(discovered_devices in State),
                        name_of!(device_discovering_state in State),
                        name_of!(aggregated_devices in State),
                    ]);
                });
            }
            StateAction::DevicesAddTcp(hostaddr) => {
                self.state_tx.send_if_modified(|state| {
                    if state.tcp_devices.contains(&hostaddr) {
                        return false;
                    }

                    state.tcp_devices.push(hostaddr);
                    state.update_aggregated_devices();

                    changed.extend(vec![
                        name_of!(tcp_devices in State),
                        name_of!(aggregated_devices in State),
                    ]);

                    true
                });
            }
            StateAction::DeviceConfigSet(variant) => {
                self.state_tx.send_modify(|state| {
                    match variant {
                        config::PayloadVariant::Bluetooth(cfg) => {
                            state.device_config.bluetooth = Some(cfg);
                        }
                        config::PayloadVariant::Device(cfg) => {
                            state.device_config.device = Some(cfg);
                        }
                        config::PayloadVariant::DeviceUi(cfg) => {
                            state.device_config.device_ui = Some(cfg);
                        }
                        config::PayloadVariant::Display(cfg) => {
                            state.device_config.display = Some(cfg);
                        }
                        config::PayloadVariant::Lora(cfg) => {
                            state.device_config.lora = Some(cfg);
                        }
                        config::PayloadVariant::Network(cfg) => {
                            state.device_config.network = Some(cfg);
                        }
                        config::PayloadVariant::Position(cfg) => {
                            state.device_config.position = Some(cfg);
                        }
                        config::PayloadVariant::Power(cfg) => {
                            state.device_config.power = Some(cfg);
                        }
                        config::PayloadVariant::Security(cfg) => {
                            state.device_config.security = Some(cfg);
                        }
                        config::PayloadVariant::Sessionkey(cfg) => {
                            state.device_config.sessionkey = Some(cfg);
                        }
                    }

                    changed.push(name_of!(device_config in State));
                });
            }
            StateAction::DeviceModuleConfigSet(variant) => {
                self.state_tx.send_modify(|state| {
                    match variant {
                        module_config::PayloadVariant::AmbientLighting(cfg) => {
                            state.device_module_config.ambient_lighting = Some(cfg);
                        }
                        module_config::PayloadVariant::Audio(cfg) => {
                            state.device_module_config.audio = Some(cfg);
                        }
                        module_config::PayloadVariant::CannedMessage(cfg) => {
                            state.device_module_config.canned_message = Some(cfg);
                        }
                        module_config::PayloadVariant::DetectionSensor(cfg) => {
                            state.device_module_config.detection_sensor = Some(cfg);
                        }
                        module_config::PayloadVariant::ExternalNotification(cfg) => {
                            state.device_module_config.external_notification = Some(cfg);
                        }
                        module_config::PayloadVariant::Mqtt(cfg) => {
                            state.device_module_config.mqtt = Some(cfg);
                        }
                        module_config::PayloadVariant::NeighborInfo(cfg) => {
                            state.device_module_config.neighbor = Some(cfg);
                        }
                        module_config::PayloadVariant::Paxcounter(cfg) => {
                            state.device_module_config.paxcounter = Some(cfg);
                        }
                        module_config::PayloadVariant::RangeTest(cfg) => {
                            state.device_module_config.range_test = Some(cfg);
                        }
                        module_config::PayloadVariant::RemoteHardware(cfg) => {
                            state.device_module_config.remote_hardware = Some(cfg);
                        }
                        module_config::PayloadVariant::Serial(cfg) => {
                            state.device_module_config.serial = Some(cfg);
                        }
                        module_config::PayloadVariant::Statusmessage(cfg) => {
                            state.device_module_config.status_message = Some(cfg);
                        }
                        module_config::PayloadVariant::StoreForward(cfg) => {
                            state.device_module_config.store_forward = Some(cfg);
                        }
                        module_config::PayloadVariant::Telemetry(cfg) => {
                            state.device_module_config.telemetry = Some(cfg);
                        }
                        module_config::PayloadVariant::TrafficManagement(cfg) => {
                            state.device_module_config.traffic_management = Some(cfg);
                        }
                    }

                    changed.push(name_of!(device_module_config in State));
                });
            }
            StateAction::DeviceUserSet(user) => {
                self.state_tx.send_modify(|state| {
                    state.device_user = Some(user);

                    changed.push(name_of!(device_user in State));
                });
            }
            StateAction::DevicesRemoveTcp(hostaddr) => {
                self.state_tx.send_if_modified(|state| {
                    let Some(index) = state.tcp_devices.iter().position(|addr| addr == &hostaddr) else {
                        return false;
                    };

                    state.tcp_devices.remove(index);
                    state.update_aggregated_devices();

                    changed.extend(vec![
                        name_of!(tcp_devices in State),
                        name_of!(aggregated_devices in State),
                    ]);

                    true
                });
            }
            StateAction::NodeInit(node) => {
                self.state_tx.send_modify(|state| {
                    state.nodes.insert(node.key, node);

                    changed.push(name_of!(nodes in State));
                });
            }
            StateAction::NodeInitTemporary(node) => {
                self.state_tx.send_if_modified(|state| {
                    let mut inserted = false;

                    state.nodes.entry(node.key).or_insert_with(|| {
                        inserted = true;
                        node
                    });

                    if inserted {
                        changed.push(name_of!(nodes in State));
                    }

                    inserted
                });
            }
            StateAction::NodeInfoPopupSetKey(node_key) => {
                self.state_tx.send_modify(|state| {
                    state.nodeinfo_popup = Some(node_key);

                    changed.push(name_of!(nodeinfo_popup in State));
                });
            }
            StateAction::NodeInfoPopupUnsetKey => {
                self.state_tx.send_modify(|state| {
                    state.nodeinfo_popup = None;

                    changed.push(name_of!(nodeinfo_popup in State));
                });
            }
            StateAction::NodeUpdate(node) => {
                self.state_tx.send_if_modified(|state| {
                    let Some(existing_node) = state.nodes.get_mut(&node.key) else {
                        return false;
                    };

                    existing_node.user = node.user;
                    existing_node.hops = node.hops;
                    existing_node.last_heard = node.last_heard;
                    existing_node.snr = node.snr;
                    existing_node.rssi = node.rssi;
                    if existing_node.public_key.is_empty() {
                        existing_node.public_key = node.public_key;
                    }
                    existing_node.update_fulltext();

                    changed.push(name_of!(nodes in State));

                    true
                });
            }
            StateAction::NodeUpdateLastHeard {
                node_key,
                hops,
                snr,
                rssi,
            } => {
                self.state_tx.send_if_modified(|state| {
                    let Some(node) = state.nodes.get_mut(&node_key) else {
                        return false;
                    };

                    node.last_heard = Some(Utc::now());
                    node.hops = Some(hops);

                    if hops == 0 {
                        node.snr = snr;
                        node.rssi = Some(rssi);
                    }

                    node.update_fulltext();

                    changed.push(name_of!(nodes in State));

                    true
                });
            }
            StateAction::NodeDelete(node_key) => {
                self.state_tx.send_if_modified(|state| {
                    if state.nodes.remove(&node_key).is_none() {
                        return false;
                    }

                    if let Some(nodeinfo_key) = state.nodeinfo_popup
                        && nodeinfo_key == node_key
                    {
                        state.nodeinfo_popup = None;

                        changed.push(name_of!(nodeinfo_popup in State));
                    }

                    changed.push(name_of!(nodes in State));

                    true
                });
            }
            StateAction::ChannelSet(key, channel) => {
                self.state_tx.send_modify(|state| {
                    state.channels.insert(key, channel);
                    state.channels.sort_keys();

                    changed.push(name_of!(channels in State));
                });
            }
            StateAction::ChannelActiveSet(key) => {
                self.state_tx.send_modify(|state| {
                    state.active_channel_key = Some(key);

                    changed.push(name_of!(active_channel_key in State));
                });
            }
            StateAction::ChannelActiveUnset => {
                self.state_tx.send_modify(|state| {
                    state.active_channel_key = None;

                    changed.push(name_of!(active_channel_key in State));
                });
            }
            StateAction::ChannelPurge(key) => {
                self.state_tx.send_if_modified(|state| {
                    let Some(channel) = state.channels.get(&key) else {
                        return false;
                    };

                    match &channel.role {
                        ChannelRole::Primary | ChannelRole::Secondary => {
                            state.messages.get_mut(&key).map(|messages| messages.clear());

                            changed.push(name_of!(messages in State));

                            true
                        }
                        ChannelRole::Direct => {
                            state.messages.get_mut(&key).map(|messages| messages.clear());
                            state.channels.remove(&key);
                            state.active_channel_key = None;

                            changed.extend(vec![
                                name_of!(messages in State),
                                name_of!(channels in State),
                                name_of!(active_channel_key in State),
                            ]);

                            true
                        }
                        _ => false,
                    }
                });
            }
            StateAction::RxTrigger => {
                self.state_tx.send_modify(|state| {
                    state.rx_t = Instant::now();
                    state.rx = true;

                    changed.extend(vec![name_of!(rx in State), name_of!(rx_t in State)]);
                });
            }
            StateAction::NodesSortBySet(sort_by) => {
                self.state_tx.send_modify(|state| {
                    state.nodes_sort_by = sort_by;

                    changed.push(name_of!(nodes_sort_by in State));
                });
            }
            StateAction::NodesFilterSet(filter) => {
                self.state_tx.send_modify(|state| {
                    state.nodes_filter = filter.to_lowercase();

                    changed.push(name_of!(nodes_filter in State));
                });
            }
            StateAction::NodesOnlineSet(count) => {
                self.state_tx.send_if_modified(|state| {
                    if state.online_nodes == count {
                        return false;
                    }

                    state.online_nodes = count;

                    changed.push(name_of!(online_nodes in State));

                    true
                });
            }
            StateAction::MyNodeKeySet(number) => {
                self.state_tx.send_modify(|state| {
                    state.my_node_key = Some(number);

                    changed.push(name_of!(my_node_key in State));
                });
            }
            StateAction::DirectChatStart(node_key) => {
                self.state_tx.send_modify(|state| {
                    state.channels.entry(node_key).or_insert_with(|| {
                        changed.push(name_of!(channels in State));

                        Channel::direct(node_key)
                    });

                    state.active_channel_key = Some(node_key);
                    state.active_tab = Tab::Chat;

                    changed.extend(vec![
                        name_of!(active_channel_key in State),
                        name_of!(active_tab in State),
                    ]);
                });
            }
            StateAction::MessageAdd(channel_key, message) => {
                self.state_tx.send_modify(|state| {
                    if let Some(messages_vec) = state.messages.get_mut(&channel_key) {
                        messages_vec.push_back(message);
                    } else {
                        state.messages.insert(channel_key, VecDeque::from(vec![message]));
                    }

                    changed.push(name_of!(messages in State));
                });
            }
            StateAction::MessageReactionAdd {
                channel_key,
                message_id,
                reaction,
            } => {
                self.state_tx.send_if_modified(|state| {
                    if let Some(message) = state
                        .messages
                        .get_mut(&channel_key)
                        .and_then(|messages| messages.iter_mut().find(|msg| msg.id == message_id))
                    {
                        if !message
                            .reactions
                            .iter()
                            .any(|r| r.node_key == reaction.node_key && r.emoji == reaction.emoji)
                        {
                            message.reactions.push(reaction);

                            changed.push(name_of!(messages in State));

                            return true;
                        }
                    }

                    false
                });
            }
            StateAction::MessageErrorSet {
                channel_key,
                message_id,
                error,
            } => {
                self.state_tx.send_if_modified(|state| {
                    if let Some(messages) = state.messages.get_mut(&channel_key) {
                        if let Some(message) = messages
                            .binary_search_by_key(&message_id, |m| m.id)
                            .ok()
                            .and_then(|index| messages.get_mut(index))
                        {
                            message.error = error;

                            changed.push(name_of!(messages in State));

                            return true;
                        }
                    }

                    false
                });
            }
            StateAction::FrameCleared => {
                self.state_tx.send_modify(|state| {
                    state.need_clear_frame = false;

                    changed.push(name_of!(need_clear_frame in State));
                });
            }
            StateAction::Toast(toast) => {
                self.state_tx.send_modify(|state| {
                    state.toast_queue.push_back(toast);

                    changed.push(name_of!(toast_queue in State));
                });
            }
            StateAction::SettingsFormLoadingStart { id } => {
                self.state_tx.send_modify(|state| {
                    state.settings_form_original_data = None;
                    state.settings_form_data = None;
                    state.settings_form_is_changed = false;
                    state.settings_form_state = SettingsFormState::Loading { id };

                    changed.extend(vec![
                        name_of!(settings_form_original_data in State),
                        name_of!(settings_form_data in State),
                        name_of!(settings_form_is_changed in State),
                        name_of!(settings_form_state in State),
                    ]);
                });
            }
            StateAction::SettingsFormLoadingFail { id, error } => {
                self.state_tx.send_modify(|state| {
                    state.settings_form_original_data = None;
                    state.settings_form_data = None;
                    state.settings_form_is_changed = false;
                    state.settings_form_state = SettingsFormState::LoadingFailed { id, error };

                    changed.extend(vec![
                        name_of!(settings_form_original_data in State),
                        name_of!(settings_form_data in State),
                        name_of!(settings_form_is_changed in State),
                        name_of!(settings_form_state in State),
                    ]);
                });
            }
            StateAction::SettingsFormLoadingDone { id, data } => {
                self.state_tx.send_modify(|state| {
                    state.settings_form_original_data = Some(data.clone());
                    state.settings_form_data = Some(data);
                    state.settings_form_is_changed = false;
                    state.settings_form_state = SettingsFormState::Loaded { id };

                    changed.extend(vec![
                        name_of!(settings_form_original_data in State),
                        name_of!(settings_form_data in State),
                        name_of!(settings_form_is_changed in State),
                        name_of!(settings_form_state in State),
                    ]);
                });
            }
            StateAction::SettingsFormSavingStart { id } => {
                self.state_tx.send_modify(|state| {
                    state.settings_form_state = SettingsFormState::Saving { id };

                    changed.push(name_of!(settings_form_state in State));
                });
            }
            StateAction::SettingsFormSavingFailed { id } => {
                self.state_tx.send_modify(|state| {
                    state.settings_form_state = SettingsFormState::Loaded { id };

                    changed.push(name_of!(settings_form_state in State));
                });
            }
            StateAction::SettingsFormClose => {
                self.state_tx.send_modify(|state| {
                    state.settings_form_original_data = None;
                    state.settings_form_data = None;
                    state.settings_form_is_changed = false;
                    state.settings_form_state = SettingsFormState::Inactive;

                    changed.extend(vec![
                        name_of!(settings_form_original_data in State),
                        name_of!(settings_form_data in State),
                        name_of!(settings_form_is_changed in State),
                        name_of!(settings_form_state in State),
                    ]);
                });
            }
            StateAction::SettingsFormReset => {
                self.state_tx.send_modify(|state| {
                    state.settings_form_data = state.settings_form_original_data.clone();
                    state.settings_form_is_changed = false;

                    changed.extend(vec![
                        name_of!(settings_form_data in State),
                        name_of!(settings_form_is_changed in State),
                    ]);
                });
            }
            StateAction::SettingsFormValueSet { key, value } => {
                self.state_tx.send_if_modified(|state| {
                    let Some(data) = state.settings_form_data.as_mut() else {
                        return false;
                    };

                    match key {
                        FormItemKey::Simple(k) => {
                            data.insert(k.to_owned(), value);
                        }
                        FormItemKey::Custom { setter, .. } => {
                            setter(data, value);
                        }
                    }

                    state.settings_form_is_changed = state.settings_form_data != state.settings_form_original_data;

                    changed.extend(vec![
                        name_of!(settings_form_data in State),
                        name_of!(settings_form_is_changed in State),
                    ]);

                    true
                });
            }
            StateAction::UiConfigSet { config } => {
                self.state_tx.send_modify(|state| {
                    state.ui_config = config;

                    changed.push(name_of!(ui_config in State));
                });
            }
        }

        if !changed.is_empty() {
            let watch_fields = [
                name_of!(my_node_key in State),
                name_of!(nodes in State),
                name_of!(nodes_filter in State),
                name_of!(nodes_sort_by in State),
            ];

            if changed.iter().any(|field| watch_fields.contains(field)) {
                self.state_tx.send_modify(|state| {
                    state.update_nodes_view();

                    changed.push(name_of!(nodes_view in State));
                })
            }

            self.changed_tx.send(changed)?;
        }

        Ok(())
    }

    fn handle_tick(&mut self) -> anyhow::Result<()> {
        let mut changed: Vec<&'static str> = Vec::new();

        self.state_tx.send_if_modified(|state| {
            if state.rx && state.rx_t.elapsed().as_millis() > RX_TIMEOUT_MILLIS {
                state.rx = false;

                changed.push(name_of!(rx in State));
            }

            if state.splash_logo && state.splash_logo_t.elapsed().as_millis() > SPLASH_LOGO_TIMEOUT_MILLIS {
                state.splash_logo = false;

                changed.push(name_of!(splash_logo in State));
            }

            if let Some(toast) = &state.toast {
                // skip toast quickly if there is another in queue
                let timeout = toast.kind.timeout().min(if state.toast_queue.is_empty() {
                    u128::MAX
                } else {
                    TOAST_QUICK_TIMEOUT_MILLIS
                });

                if state.toast_t.elapsed().as_millis() > timeout {
                    state.toast = None;

                    changed.push(name_of!(toast in State));
                }
            }

            if !state.toast_queue.is_empty() {
                state.toast = state.toast_queue.pop_front();
                state.toast_t = Instant::now();

                changed.extend(vec![name_of!(toast in State), name_of!(toast_t in State)]);
            }

            !changed.is_empty()
        });

        if !changed.is_empty() {
            self.changed_tx.send(changed)?;
        }

        Ok(())
    }
}
