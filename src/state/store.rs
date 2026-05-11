use std::{
    cmp::Ordering,
    collections::VecDeque,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use itertools::Itertools;
use meshtastic::protobufs::{config, module_config};
use nameof::name_of;
use tokio::{
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        watch,
    },
    time,
};
use tokio_graceful_shutdown::SubsystemHandle;

use crate::{
    state::{State, StateAction},
    types::{
        Channel, ConnectionState, Device, DeviceDiscoveringState, FormItemKey, NodesSortBy, SettingsFormState, Tab,
    },
};

const TICK_INTERVAL_MILLIS: u64 = 33;
const RX_TIMEOUT_MILLIS: u128 = 250;
const TOAST_QUICK_TIMEOUT_MILLIS: u128 = 500;
const SPLASH_LOGO_TIMEOUT_MILLIS: u128 = 1500;

pub struct StateSnapshot {
    pub state: State,
    pub changed: Vec<&'static str>,
}

impl StateSnapshot {
    pub fn new(state: State, changed: Vec<&'static str>) -> Self {
        Self { state, changed }
    }
}

pub struct Store {
    state: State,
    action_rx: UnboundedReceiver<StateAction>,
    state_tx: watch::Sender<StateSnapshot>,
}

impl Store {
    pub fn new(initial_state: State) -> (Self, UnboundedSender<StateAction>, watch::Receiver<StateSnapshot>) {
        let (action_tx, action_rx) = unbounded_channel::<StateAction>();
        let (state_tx, state_rx) = watch::channel(StateSnapshot::new(initial_state.clone(), [].into()));

        (
            Self {
                state: initial_state.clone(),
                action_rx,
                state_tx,
            },
            action_tx,
            state_rx,
        )
    }

    pub async fn run(mut self, subsys: &mut SubsystemHandle) -> anyhow::Result<()> {
        let mut tick_interval = time::interval(Duration::from_millis(TICK_INTERVAL_MILLIS));

        loop {
            tokio::select! {
                Some(action) = self.action_rx.recv() => self.handle_action(action)?,
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
                self.state.splash_logo = true;
                self.state.splash_logo_t = Instant::now();

                changed.extend(vec![name_of!(splash_logo in State), name_of!(splash_logo_t in State)]);
            }
            StateAction::AppConfigApply(cfg) => {
                self.state.active_tab = cfg.active_tab;
                self.state.active_device = cfg.active_device;
                self.state.tcp_devices = cfg.tcp_devices;
                self.state.nodes_sort_by = cfg.nodes_sort_by;
                self.state.ui_config = cfg.ui_config;
                self.update_aggregated_devices();

                changed.extend(vec![
                    name_of!(active_tab in State),
                    name_of!(active_device in State),
                    name_of!(tcp_devices in State),
                    name_of!(nodes_sort_by in State),
                    name_of!(ui_config in State),
                    name_of!(aggregated_devices in State),
                ]);
            }
            StateAction::TabSwitchTo(tab) => {
                self.state.active_tab = tab;
                self.state.need_clear_frame = true;

                changed.extend(vec![name_of!(active_tab in State), name_of!(need_clear_frame in State)]);
            }
            StateAction::TabSwitchToNext => {
                self.state.active_tab = self.state.active_tab.next();
                self.state.need_clear_frame = true;

                changed.extend(vec![name_of!(active_tab in State), name_of!(need_clear_frame in State)]);
            }
            StateAction::TabSwitchToPrevious => {
                self.state.active_tab = self.state.active_tab.prev();
                self.state.need_clear_frame = true;

                changed.extend(vec![name_of!(active_tab in State), name_of!(need_clear_frame in State)]);
            }
            StateAction::DeviceActiveSet(device) => {
                self.state.active_device = Some(device);

                changed.push(name_of!(active_device in State));
            }
            StateAction::ConnectionStart => {
                self.state.connection_state = ConnectionState::Connecting;
                self.state.connection_attempt += 1;
                self.state.reconnection_backoff = None;

                changed.extend(vec![
                    name_of!(connection_state in State),
                    name_of!(connection_attempt in State),
                    name_of!(reconnection_backoff in State),
                ]);

                tracing::debug!("connection attempt #{}", self.state.connection_attempt);
            }
            StateAction::ConnectionFail(error) => {
                self.state.connection_state = ConnectionState::ProblemDetected {
                    since: Instant::now(),
                    error,
                };

                changed.push(name_of!(connection_state in State));
            }
            StateAction::ConnectionStop => {
                self.state.connection_state = Default::default();
                self.state.connection_attempt = 0;
                self.state.reconnection_backoff = None;
                self.state.active_device = None;
                self.state.settings_form_state = Default::default();
                self.state.settings_form_data = None;
                self.state.settings_form_original_data = None;
                self.state.settings_form_is_changed = false;
                self.state.device_config = Default::default();
                self.state.device_module_config = Default::default();
                self.state.device_user = Default::default();
                self.state.channels.clear();
                self.state.nodes_view.clear();
                self.state.nodes.clear();
                self.state.online_nodes = 0;

                changed.extend(vec![
                    name_of!(connection_state in State),
                    name_of!(connection_attempt in State),
                    name_of!(reconnection_backoff in State),
                    name_of!(active_device in State),
                    name_of!(settings_form_state in State),
                    name_of!(settings_form_data in State),
                    name_of!(settings_form_original_data in State),
                    name_of!(settings_form_is_changed in State),
                    name_of!(device_config in State),
                    name_of!(device_module_config in State),
                    name_of!(device_user in State),
                    name_of!(channels in State),
                    name_of!(nodes_view in State),
                    name_of!(nodes in State),
                    name_of!(online_nodes in State),
                ]);
            }
            StateAction::ConnectionSuccess => {
                self.state.connection_state = ConnectionState::Connected;
                self.state.connection_attempt = 0;
                self.state.reconnection_backoff = None;

                changed.extend(vec![
                    name_of!(connection_state in State),
                    name_of!(connection_attempt in State),
                    name_of!(reconnection_backoff in State),
                ]);
            }
            StateAction::ReconnectionBackoffSet(duration) => {
                self.state.reconnection_backoff = Some(duration);

                changed.push(name_of!(reconnection_backoff in State));
            }
            StateAction::LogRecordAdd(r) => {
                self.state.logs.push(r);

                changed.push(name_of!(logs in State));
            }
            StateAction::DeviceDiscoveringStart => {
                self.state.device_discovering_state = DeviceDiscoveringState::Discovering;

                changed.push(name_of!(device_discovering_state in State));
            }
            StateAction::DeviceDiscoveringFail(error) => {
                self.state.device_discovering_state = DeviceDiscoveringState::Failed(error);

                changed.push(name_of!(device_discovering_state in State));
            }
            StateAction::DeviceDiscoveringDone(devices) => {
                self.state.discovered_devices = devices;
                self.state.device_discovering_state = DeviceDiscoveringState::Done;
                self.update_aggregated_devices();

                changed.extend(vec![
                    name_of!(discovered_devices in State),
                    name_of!(device_discovering_state in State),
                    name_of!(aggregated_devices in State),
                ]);
            }
            StateAction::DevicesAddTcp(hostaddr) => {
                if !self.state.tcp_devices.contains(&hostaddr) {
                    self.state.tcp_devices.push(hostaddr);
                    self.update_aggregated_devices();

                    changed.extend(vec![
                        name_of!(tcp_devices in State),
                        name_of!(aggregated_devices in State),
                    ]);
                }
            }
            StateAction::DeviceConfigSet(variant) => {
                match variant {
                    config::PayloadVariant::Bluetooth(cfg) => {
                        self.state.device_config.bluetooth = Some(cfg);
                    }
                    config::PayloadVariant::Device(cfg) => {
                        self.state.device_config.device = Some(cfg);
                    }
                    config::PayloadVariant::DeviceUi(cfg) => {
                        self.state.device_config.device_ui = Some(cfg);
                    }
                    config::PayloadVariant::Display(cfg) => {
                        self.state.device_config.display = Some(cfg);
                    }
                    config::PayloadVariant::Lora(cfg) => {
                        self.state.device_config.lora = Some(cfg);
                    }
                    config::PayloadVariant::Network(cfg) => {
                        self.state.device_config.network = Some(cfg);
                    }
                    config::PayloadVariant::Position(cfg) => {
                        self.state.device_config.position = Some(cfg);
                    }
                    config::PayloadVariant::Power(cfg) => {
                        self.state.device_config.power = Some(cfg);
                    }
                    config::PayloadVariant::Security(cfg) => {
                        self.state.device_config.security = Some(cfg);
                    }
                    config::PayloadVariant::Sessionkey(cfg) => {
                        self.state.device_config.sessionkey = Some(cfg);
                    }
                }

                changed.push(name_of!(device_config in State));
            }
            StateAction::DeviceModuleConfigSet(variant) => {
                match variant {
                    module_config::PayloadVariant::AmbientLighting(cfg) => {
                        self.state.device_module_config.ambient_lighting = Some(cfg);
                    }
                    module_config::PayloadVariant::Audio(cfg) => {
                        self.state.device_module_config.audio = Some(cfg);
                    }
                    module_config::PayloadVariant::CannedMessage(cfg) => {
                        self.state.device_module_config.canned_message = Some(cfg);
                    }
                    module_config::PayloadVariant::DetectionSensor(cfg) => {
                        self.state.device_module_config.detection_sensor = Some(cfg);
                    }
                    module_config::PayloadVariant::ExternalNotification(cfg) => {
                        self.state.device_module_config.external_notification = Some(cfg);
                    }
                    module_config::PayloadVariant::Mqtt(cfg) => {
                        self.state.device_module_config.mqtt = Some(cfg);
                    }
                    module_config::PayloadVariant::NeighborInfo(cfg) => {
                        self.state.device_module_config.neighbor = Some(cfg);
                    }
                    module_config::PayloadVariant::Paxcounter(cfg) => {
                        self.state.device_module_config.paxcounter = Some(cfg);
                    }
                    module_config::PayloadVariant::RangeTest(cfg) => {
                        self.state.device_module_config.range_test = Some(cfg);
                    }
                    module_config::PayloadVariant::RemoteHardware(cfg) => {
                        self.state.device_module_config.remote_hardware = Some(cfg);
                    }
                    module_config::PayloadVariant::Serial(cfg) => {
                        self.state.device_module_config.serial = Some(cfg);
                    }
                    module_config::PayloadVariant::Statusmessage(cfg) => {
                        self.state.device_module_config.status_message = Some(cfg);
                    }
                    module_config::PayloadVariant::StoreForward(cfg) => {
                        self.state.device_module_config.store_forward = Some(cfg);
                    }
                    module_config::PayloadVariant::Telemetry(cfg) => {
                        self.state.device_module_config.telemetry = Some(cfg);
                    }
                    module_config::PayloadVariant::TrafficManagement(cfg) => {
                        self.state.device_module_config.traffic_management = Some(cfg);
                    }
                }

                changed.push(name_of!(device_module_config in State));
            }
            StateAction::DeviceUserSet(user) => {
                self.state.device_user = Some(user);

                changed.push(name_of!(device_user in State));
            }
            StateAction::DevicesRemoveTcp(hostaddr) => {
                if let Some(index) = self.state.tcp_devices.iter().position(|addr| addr == &hostaddr) {
                    self.state.tcp_devices.remove(index);
                    self.update_aggregated_devices();

                    changed.extend(vec![
                        name_of!(tcp_devices in State),
                        name_of!(aggregated_devices in State),
                    ]);
                }
            }
            StateAction::NodeSet(mut node) => {
                if let Some(number) = self.state.my_node_key
                    && node.key == number
                {
                    node.my = true;
                }

                self.state.nodes.insert(node.key, node);
                self.update_nodes_view();

                changed.extend(vec![name_of!(nodes in State), name_of!(nodes_view in State)]);
            }
            StateAction::NodeDelete(node_key) => {
                if self.state.nodes.remove(&node_key).is_some() {
                    self.update_nodes_view();

                    changed.extend(vec![name_of!(nodes in State), name_of!(nodes_view in State)]);
                }
            }
            StateAction::NodeEnsure(mut node) => {
                if let Some(number) = self.state.my_node_key
                    && node.key == number
                {
                    node.my = true;
                }

                self.state.nodes.entry(node.key).or_insert(node);
                self.update_nodes_view();

                changed.extend(vec![name_of!(nodes in State), name_of!(nodes_view in State)]);
            }
            StateAction::ChannelSet(key, channel) => {
                self.state.channels.insert(key, channel);
                self.state.channels.sort_keys();

                changed.push(name_of!(channels in State));
            }
            StateAction::ChannelActiveSet(id) => {
                self.state.active_channel_key = Some(id);

                changed.push(name_of!(active_channel_key in State));
            }
            StateAction::ChannelActiveUnset => {
                self.state.active_channel_key = None;

                changed.push(name_of!(active_channel_key in State));
            }
            StateAction::RxTrigger => {
                self.state.rx_t = Instant::now();
                self.state.rx = true;

                changed.extend(vec![name_of!(rx in State), name_of!(rx_t in State)]);
            }
            StateAction::NodesSortBySet(sort_by) => {
                self.state.nodes_sort_by = sort_by;
                self.update_nodes_view();

                changed.extend(vec![name_of!(nodes_sort_by in State), name_of!(nodes_view in State)]);
            }
            StateAction::NodesFilterSet(filter) => {
                self.state.nodes_sort_filter = filter.to_lowercase();
                self.update_nodes_view();

                changed.extend(vec![
                    name_of!(nodes_sort_filter in State),
                    name_of!(nodes_view in State),
                ]);
            }
            StateAction::NodesOnlineSet(count) => {
                if self.state.online_nodes != count {
                    self.state.online_nodes = count;

                    changed.push(name_of!(online_nodes in State));
                }
            }
            StateAction::NodeUpdateLastHeard { node_key, hops, snr } => {
                if let Some(node) = self.state.nodes.get_mut(&node_key) {
                    node.last_heard = Some(Utc::now());
                    node.hops = Some(hops);

                    if hops == 0 {
                        node.snr = snr;
                    }

                    self.update_nodes_view();

                    changed.extend(vec![name_of!(nodes in State), name_of!(nodes_view in State)]);
                }
            }
            StateAction::MyNodeKeySet(number) => {
                self.state.my_node_key = Some(number);

                if let Some(node) = self.state.nodes.get_mut(&number) {
                    node.my = true;

                    changed.extend(vec![name_of!(my_node_key in State), name_of!(nodes in State)]);
                }
            }
            StateAction::DirectChatStart(node_key) => {
                self.state.channels.entry(node_key).or_insert_with(|| {
                    changed.push(name_of!(channels in State));

                    Channel::direct(node_key)
                });

                self.state.active_channel_key = Some(node_key);
                self.state.active_tab = Tab::Chat;

                changed.extend(vec![
                    name_of!(active_channel_key in State),
                    name_of!(active_tab in State),
                ]);
            }
            StateAction::MessageAdd(channel_key, message) => {
                if let Some(messages_vec) = self.state.messages.get_mut(&channel_key) {
                    messages_vec.push_back(message);
                } else {
                    self.state.messages.insert(channel_key, VecDeque::from(vec![message]));
                }

                changed.push(name_of!(messages in State));
            }
            StateAction::MessageReactionAdd {
                channel_key,
                message_id,
                reaction,
            } => {
                if let Some(message) = self
                    .state
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
                    }
                }
            }
            StateAction::MessageErrorSet {
                channel_key,
                message_id,
                error,
            } => {
                if let Some(messages) = self.state.messages.get_mut(&channel_key) {
                    if let Some(message) = messages
                        .binary_search_by_key(&message_id, |m| m.id)
                        .ok()
                        .and_then(|index| messages.get_mut(index))
                    {
                        message.error = error;

                        changed.push(name_of!(messages in State));
                    }
                }
            }
            StateAction::FrameCleared => {
                self.state.need_clear_frame = false;

                changed.push(name_of!(need_clear_frame in State));
            }
            StateAction::Toast(toast) => {
                self.state.toast_queue.push_back(toast);

                changed.push(name_of!(toast_queue in State));
            }
            StateAction::SettingsFormLoadingStart { id } => {
                self.state.settings_form_original_data = None;
                self.state.settings_form_data = None;
                self.state.settings_form_is_changed = false;
                self.state.settings_form_state = SettingsFormState::Loading { id };

                changed.extend(vec![
                    name_of!(settings_form_original_data in State),
                    name_of!(settings_form_data in State),
                    name_of!(settings_form_is_changed in State),
                    name_of!(settings_form_state in State),
                ]);
            }
            StateAction::SettingsFormLoadingFail { id, error } => {
                self.state.settings_form_original_data = None;
                self.state.settings_form_data = None;
                self.state.settings_form_is_changed = false;
                self.state.settings_form_state = SettingsFormState::LoadingFailed { id, error };

                changed.extend(vec![
                    name_of!(settings_form_original_data in State),
                    name_of!(settings_form_data in State),
                    name_of!(settings_form_is_changed in State),
                    name_of!(settings_form_state in State),
                ]);
            }
            StateAction::SettingsFormLoadingDone { id, data } => {
                self.state.settings_form_original_data = Some(data.clone());
                self.state.settings_form_data = Some(data);
                self.state.settings_form_is_changed = false;
                self.state.settings_form_state = SettingsFormState::Loaded { id };

                changed.extend(vec![
                    name_of!(settings_form_original_data in State),
                    name_of!(settings_form_data in State),
                    name_of!(settings_form_is_changed in State),
                    name_of!(settings_form_state in State),
                ]);
            }
            StateAction::SettingsFormSavingStart { id } => {
                self.state.settings_form_state = SettingsFormState::Saving { id };

                changed.push(name_of!(settings_form_state in State));
            }
            StateAction::SettingsFormSavingFailed { id } => {
                self.state.settings_form_state = SettingsFormState::Loaded { id };

                changed.push(name_of!(settings_form_state in State));
            }
            StateAction::SettingsFormClose => {
                self.state.settings_form_original_data = None;
                self.state.settings_form_data = None;
                self.state.settings_form_is_changed = false;
                self.state.settings_form_state = SettingsFormState::Inactive;

                changed.extend(vec![
                    name_of!(settings_form_original_data in State),
                    name_of!(settings_form_data in State),
                    name_of!(settings_form_is_changed in State),
                    name_of!(settings_form_state in State),
                ]);
            }
            StateAction::SettingsFormReset => {
                self.state.settings_form_data = self.state.settings_form_original_data.clone();
                self.state.settings_form_is_changed = false;

                changed.extend(vec![
                    name_of!(settings_form_data in State),
                    name_of!(settings_form_is_changed in State),
                ]);
            }
            StateAction::SettingsFormValueSet { key, value } => {
                if let Some(data) = self.state.settings_form_data.as_mut() {
                    match key {
                        FormItemKey::Simple(k) => {
                            data.insert(k.to_owned(), value);
                        }
                        FormItemKey::Custom { setter, .. } => {
                            setter(data, value);
                        }
                    }

                    self.state.settings_form_is_changed =
                        self.state.settings_form_data != self.state.settings_form_original_data;

                    changed.extend(vec![
                        name_of!(settings_form_data in State),
                        name_of!(settings_form_is_changed in State),
                    ]);
                }
            }
            StateAction::UIConfigSet { config } => {
                self.state.ui_config = config;

                changed.push(name_of!(ui_config in State));
            }
        }

        if !changed.is_empty() {
            self.state_tx.send(StateSnapshot::new(self.state.clone(), changed))?;
        }

        Ok(())
    }

    fn handle_tick(&mut self) -> anyhow::Result<()> {
        if self.state.rx && self.state.rx_t.elapsed().as_millis() > RX_TIMEOUT_MILLIS {
            self.state.rx = false;
            self.state_tx
                .send(StateSnapshot::new(self.state.clone(), vec![name_of!(rx in State)]))?;
        }

        if self.state.splash_logo && self.state.splash_logo_t.elapsed().as_millis() > SPLASH_LOGO_TIMEOUT_MILLIS {
            self.state.splash_logo = false;
            self.state_tx.send(StateSnapshot::new(
                self.state.clone(),
                vec![name_of!(splash_logo in State)],
            ))?;
        }

        if let Some(toast) = &self.state.toast {
            // skip toast quickly if there is another in queue
            let timeout = toast.kind.timeout().min(if self.state.toast_queue.is_empty() {
                u128::MAX
            } else {
                TOAST_QUICK_TIMEOUT_MILLIS
            });

            if self.state.toast_t.elapsed().as_millis() > timeout {
                self.state.toast = None;
                self.state_tx
                    .send(StateSnapshot::new(self.state.clone(), vec![name_of!(toast in State)]))?;
            }
        }

        if !self.state.toast_queue.is_empty() {
            self.state.toast = self.state.toast_queue.pop_front();
            self.state.toast_t = Instant::now();
            self.state_tx
                .send(StateSnapshot::new(self.state.clone(), vec![name_of!(toast in State)]))?;
        }

        Ok(())
    }

    fn update_nodes_view(&mut self) {
        let filter = &self.state.nodes_sort_filter;

        self.state.nodes_view = self
            .state
            .nodes
            .values()
            .filter(|n| {
                if filter.is_empty() {
                    return true;
                }

                n.fulltext.contains(filter)
            })
            .sorted_by(|n1, n2| {
                match (n1.my, n2.my) {
                    (true, true) => return Ordering::Equal,
                    (false, true) => return Ordering::Greater,
                    (true, false) => return Ordering::Less,
                    _ => {}
                };

                match &self.state.nodes_sort_by {
                    NodesSortBy::Hops => n1
                        .hops
                        .unwrap_or(u32::MAX)
                        .cmp(&n2.hops.unwrap_or(u32::MAX))
                        .then(n1.snr.total_cmp(&n2.snr).reverse()),
                    NodesSortBy::LastHeard => n1
                        .last_heard
                        .unwrap_or(DateTime::default())
                        .cmp(&n2.last_heard.unwrap_or(DateTime::default()))
                        .reverse(),
                    NodesSortBy::ShortName => n1.short_name().cmp(&n2.short_name()),
                    NodesSortBy::LongName => n1.long_name().cmp(&n2.long_name()),
                    NodesSortBy::HwModel => n1
                        .hw_model()
                        .cmp(&n2.hw_model())
                        .then(n1.short_name().cmp(&n2.short_name())),
                    NodesSortBy::Role => n1.role().cmp(&n2.role()).then(
                        n1.hops
                            .unwrap_or(u32::MAX)
                            .cmp(&n2.hops.unwrap_or(u32::MAX))
                            .then(n1.snr.total_cmp(&n2.snr).reverse()),
                    ),
                }
            })
            .map(|node| node.key)
            .collect();
    }

    fn update_aggregated_devices(&mut self) {
        self.state.aggregated_devices = self
            .state
            .tcp_devices
            .iter()
            .map(|h| Device::Tcp(h.clone()))
            .chain(self.state.discovered_devices.clone())
            .sorted()
            .collect();
    }
}
