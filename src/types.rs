use std::{collections::HashMap, fmt::Debug, time::Instant};

use anyhow::anyhow;
use chrono::{DateTime, TimeZone, Utc};
use emoji::Emoji;
use hostaddr::HostAddr;
use meshtastic::protobufs::{DeviceUiConfig, MeshPacket, User, config, module_config, routing};
use ordermap::OrderMap;
use ratatui::{
    style::{self, Stylize as _},
    text,
};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumCount, EnumIter, FromRepr};
use tokio::sync::watch::Ref;
use tracing::Level;

use crate::{state::State, ui::helpers::pad_center};

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub active_tab: Tab,
    #[serde(default)]
    pub active_device: Option<Device>,
    #[serde(default)]
    pub tcp_devices: Vec<HostAddr<String>>,
    #[serde(default)]
    pub nodes_sort_by: NodesSortBy,
}

impl From<&Ref<'_, State>> for AppConfig {
    fn from(value: &Ref<'_, State>) -> Self {
        Self {
            active_tab: value.active_tab,
            active_device: value.active_device.clone(),
            tcp_devices: value.tcp_devices.clone(),
            nodes_sort_by: value.nodes_sort_by.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    ChannelSelected(u32),
    SwitchChannelRequested,
    DeviceRediscoverRequested,
    DeviceSelected(Device),
    DisconnectionRequested,
    InitializationRequested,
    NextTabRequested,
    PreviousTabRequested,
    TcpDeviceRemoved(HostAddr<String>),
    TcpDeviceSubmitted(HostAddr<String>),
    ChatMessageSubmitted {
        text: String,
        reply_message_id: Option<u32>,
    },
    ChatReactionSubmitted {
        emoji: &'static Emoji,
        reply_message_id: Option<u32>,
    },
    SplashLogoRequested,
    DirectChatRequested(u32),
    SettingsFormSelected(FormId),
    SettingsFormCancelRequested,
    SettingsFormResetRequested,
    SettingsFormSaveRequested(FormId),
    SettingsFormItemSubmitted(&'static FormItem, FormValue),
    CopyToClipboardRequested(String),
    NodesSortByCyclePressed,
    NodesFilterChanged(String),
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Serialize, Deserialize, Hash)]
pub enum Device {
    Ble { name: String, address: String },
    Tcp(HostAddr<String>),
    Serial(String),
}

impl Ord for Device {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Device::Tcp { .. }, Device::Ble { .. }) => std::cmp::Ordering::Less,
            (Device::Tcp { .. }, Device::Serial { .. }) => std::cmp::Ordering::Less,
            (Device::Tcp(hostaddr), Device::Tcp(other_hostaddr)) => hostaddr.cmp(other_hostaddr),

            (Device::Ble { .. }, Device::Tcp { .. }) => std::cmp::Ordering::Greater,
            (Device::Ble { .. }, Device::Serial { .. }) => std::cmp::Ordering::Less,
            (
                Device::Ble { address, .. },
                Device::Ble {
                    address: other_address, ..
                },
            ) => address.cmp(other_address),

            (Device::Serial { .. }, Device::Tcp { .. }) => std::cmp::Ordering::Greater,
            (Device::Serial { .. }, Device::Ble { .. }) => std::cmp::Ordering::Greater,
            (Device::Serial(address), Device::Serial(other_address)) => address.cmp(other_address),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum DeviceDiscoveringState {
    #[default]
    NotStarted,
    Discovering,
    Failed(String),
    Done,
}

#[derive(Debug, Clone, Default)]
pub enum ConnectionState {
    #[default]
    NotConnected,
    ProblemDetected {
        since: Instant,
        error: String,
    },
    Connecting,
    Connected,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogRecord {
    pub datetime: DateTime<Utc>,
    pub level: Level,
    pub source: String,
    pub message: String,
}

impl Into<String> for LogRecord {
    fn into(self) -> String {
        format!(
            "{} {} {}: {}",
            self.datetime.to_rfc3339(),
            self.level.to_string(),
            self.source.clone(),
            self.message
        )
    }
}

#[derive(
    Debug, Default, Clone, Copy, PartialEq, Display, FromRepr, EnumIter, EnumCount, Serialize, Deserialize, Hash,
)]
pub enum Tab {
    #[default]
    #[strum(to_string = "Chat")]
    Chat,
    #[strum(to_string = "Nodes")]
    Nodes,
    #[strum(to_string = "Settings")]
    Settings,
    #[strum(to_string = "Connection")]
    Connection,
    #[strum(to_string = "Logs")]
    Logs,
}

impl Tab {
    pub fn prev(self) -> Self {
        let current_index: usize = self as usize;
        let (previous_index, overflowed) = current_index.overflowing_sub(1);

        Self::from_repr(if overflowed { Tab::COUNT - 1 } else { previous_index }).unwrap_or(self)
    }

    pub fn next(self) -> Self {
        let current_index = self as usize;
        let next_index = current_index.saturating_add(1);

        Self::from_repr(if next_index > Tab::COUNT - 1 { 0 } else { next_index }).unwrap_or(self)
    }
}

#[derive(Debug, Clone)]
pub struct Hotkey {
    pub key: String,
    pub label: String,
}

impl Hotkey {
    pub fn new<S: Into<String>>(key: S, label: S) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
#[repr(u128)]
pub enum ToastKind {
    Success,
    Normal,
    Warning,
    Error,
}

impl ToastKind {
    pub fn timeout(&self) -> u128 {
        match self {
            Self::Success => 1500,
            Self::Normal => 1500,
            Self::Warning => 2000,
            Self::Error => 3000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
}

#[allow(dead_code)]
impl Toast {
    pub fn success<S: Into<String>>(text: S) -> Self {
        Self {
            kind: ToastKind::Success,
            text: text.into(),
        }
    }

    pub fn normal<S: Into<String>>(text: S) -> Self {
        Self {
            kind: ToastKind::Normal,
            text: text.into(),
        }
    }

    pub fn warning<S: Into<String>>(text: S) -> Self {
        Self {
            kind: ToastKind::Warning,
            text: text.into(),
        }
    }

    pub fn error<S: Into<String>>(text: S) -> Self {
        Self {
            kind: ToastKind::Error,
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Display, FromRepr, EnumIter, EnumCount, Serialize, Deserialize, Hash, Default)]
pub enum NodesSortBy {
    #[default]
    #[strum(to_string = "Hops / SNR")]
    Hops,
    #[strum(to_string = "Short Name")]
    ShortName,
    #[strum(to_string = "Long Name")]
    LongName,
    #[strum(to_string = "Last Heard")]
    LastHeard,
    #[strum(to_string = "Role / Hops / SNR")]
    Role,
    #[strum(to_string = "HW Model / Short Name")]
    HwModel,
}

#[allow(dead_code)]
impl NodesSortBy {
    pub fn prev(self) -> Self {
        let current_index: usize = self as usize;
        let (previous_index, overflowed) = current_index.overflowing_sub(1);

        Self::from_repr(if overflowed {
            NodesSortBy::COUNT - 1
        } else {
            previous_index
        })
        .unwrap_or(self)
    }

    pub fn next(self) -> Self {
        let current_index = self as usize;
        let next_index = current_index.saturating_add(1);

        Self::from_repr(if next_index > NodesSortBy::COUNT - 1 {
            0
        } else {
            next_index
        })
        .unwrap_or(self)
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub key: u32,
    pub short_name: String,
    pub long_name: String,
    pub hops_away: Option<u32>,
    pub last_heard: Option<DateTime<Utc>>,
    pub snr: f32,
    pub role: String,
    pub hw_model: String,
    pub my: bool,
    pub fulltext: String,
}

impl Node {
    pub fn unknown() -> Self {
        Self {
            id: "?".to_owned(),
            key: 0,
            short_name: "?".to_owned(),
            long_name: "Unknown".to_owned(),
            hops_away: None,
            last_heard: None,
            snr: 0.0,
            role: "UNKNOWN".to_owned(),
            hw_model: "UNKNOWN".to_owned(),
            my: false,
            fulltext: "UNKNOWN".to_owned(),
        }
    }

    pub fn to_span(&self) -> text::Span<'_> {
        text::Span::from(pad_center(&self.short_name, 6))
            .black()
            .patch_style(if self.my {
                style::Style::new().white().on_blue()
            } else {
                style::Style::new().on_green()
            })
    }
}

impl TryFrom<&meshtastic::protobufs::NodeInfo> for Node {
    type Error = anyhow::Error;

    fn try_from(value: &meshtastic::protobufs::NodeInfo) -> Result<Self, Self::Error> {
        let user = value.user.as_ref().ok_or(anyhow!("no user information"))?;
        let last_heard = DateTime::from_timestamp(value.last_heard as i64, 0);
        let role = user.role().as_str_name();
        let hw_model = user.hw_model().as_str_name();

        Ok(Self {
            id: user.id.clone(),
            key: value.num,
            short_name: user.short_name.clone(),
            long_name: user.long_name.clone(),
            hops_away: value.hops_away,
            last_heard,
            snr: value.snr,
            role: role.to_string(),
            hw_model: hw_model.to_string(),
            my: false,
            fulltext: format!(
                "{} {} {} {} {}",
                user.short_name.to_lowercase(),
                user.long_name.to_lowercase(),
                role.to_lowercase(),
                hw_model.to_lowercase(),
                user.id,
            ),
        })
    }
}

impl TryFrom<(&MeshPacket, &User)> for Node {
    type Error = anyhow::Error;

    fn try_from((packet, user): (&MeshPacket, &User)) -> Result<Self, Self::Error> {
        let last_heard = DateTime::from_timestamp(packet.rx_time as i64, 0);
        let role = user.role().as_str_name();
        let hw_model = user.hw_model().as_str_name();

        Ok(Self {
            id: user.id.clone(),
            key: packet.from,
            short_name: user.short_name.clone(),
            long_name: user.long_name.clone(),
            hops_away: Some(packet.hop_start.saturating_sub(packet.hop_limit)),
            last_heard,
            snr: packet.rx_snr,
            role: role.to_string(),
            hw_model: hw_model.to_string(),
            my: false,
            fulltext: format!(
                "{} {} {} {} {}",
                user.short_name.to_lowercase(),
                user.long_name.to_lowercase(),
                role.to_lowercase(),
                hw_model.to_lowercase(),
                user.id,
            ),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChannelRole {
    Disabled = 0,
    Primary = 1,
    Secondary = 2,
    Direct = 3,
}

impl ChannelRole {
    pub fn is_disabled(&self) -> bool {
        self == &Self::Disabled
    }

    pub fn is_direct(&self) -> bool {
        self == &Self::Direct
    }
}

impl From<meshtastic::protobufs::channel::Role> for ChannelRole {
    fn from(value: meshtastic::protobufs::channel::Role) -> Self {
        match value {
            meshtastic::protobufs::channel::Role::Disabled => ChannelRole::Disabled,
            meshtastic::protobufs::channel::Role::Primary => ChannelRole::Primary,
            meshtastic::protobufs::channel::Role::Secondary => ChannelRole::Secondary,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Channel {
    pub key: u32,
    #[allow(dead_code)]
    pub id: u32,
    pub role: ChannelRole,
    pub name: String,
}

impl Channel {
    pub fn disabled(index: u32) -> Self {
        Self {
            key: index,
            id: 0,
            role: ChannelRole::Disabled,
            name: String::default(),
        }
    }

    pub fn direct(node_key: u32) -> Self {
        Self {
            key: node_key,
            id: 0,
            role: ChannelRole::Direct,
            name: String::default(),
        }
    }
}

impl From<&meshtastic::protobufs::Channel> for Channel {
    fn from(value: &meshtastic::protobufs::Channel) -> Self {
        match &value.settings {
            Some(settings) => Self {
                key: value.index as u32,
                id: settings.id,
                role: value.role().into(),
                name: settings.name.to_string(),
            },
            None => Channel::disabled(value.index as u32),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub id: u32,
    pub reply_message_id: u32,
    pub from: u32,
    pub datetime: DateTime<Utc>,
    pub text: String,
    pub reactions: OrderMap<String, HashMap<u32, DateTime<Utc>>>,
    #[allow(dead_code)]
    pub hops: Option<u32>,
    pub snr: f32,
    pub rssi: i32,
    pub error: Option<routing::Error>,
}

impl TryFrom<(&meshtastic::protobufs::MeshPacket, &meshtastic::protobufs::Data)> for Message {
    type Error = anyhow::Error;

    fn try_from(
        (packet, data): (&meshtastic::protobufs::MeshPacket, &meshtastic::protobufs::Data),
    ) -> Result<Self, Self::Error> {
        if data.payload.is_empty() {
            return Err(anyhow!("payload is empty"));
        }

        Ok(Self {
            id: packet.id,
            reply_message_id: data.reply_id,
            from: packet.from,
            datetime: Utc
                .timestamp_opt(packet.rx_time as i64, 0)
                .single()
                .unwrap_or(Utc::now()),
            text: String::from_utf8(data.payload.clone())?,
            reactions: OrderMap::default(),
            hops: Some(packet.hop_start.saturating_sub(packet.hop_limit)),
            snr: packet.rx_snr,
            rssi: packet.rx_rssi,
            error: None,
        })
    }
}

#[derive(Debug, Display, Clone, PartialEq, Eq, Hash)]
pub enum FormId {
    RadioLora,
    RadioChannels,
    RadioSecurity,
    DeviceUser,
    DeviceDevice,
    DevicePosition,
    DevicePower,
    DeviceDisplay,
    DeviceBluetooth,
    ModuleMqtt,
    ModuleSerial,
    ModuleExternalNotification,
    ModuleStoreAndForward,
    ModuleRangeTest,
    ModuleTelemetry,
    ModuleCannedMessage,
    ModuleNeighborInfo,
    AppUi,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum SettingsFormState {
    #[default]
    Inactive,
    Loading {
        id: FormId,
    },
    LoadingFailed {
        id: FormId,
        error: String,
    },
    Loaded {
        id: FormId,
    },
}

#[derive(Debug, Clone)]
pub enum SettingsItem {
    Group { title: &'static str },
    Form { title: &'static str, id: FormId },
}

impl SettingsItem {
    pub fn group(title: &'static str) -> Self {
        Self::Group { title }
    }

    pub fn form(title: &'static str, id: FormId) -> Self {
        Self::Form { title, id }
    }
}

pub type FormData = HashMap<&'static str, FormValue>;

#[derive(Debug, Clone, PartialEq)]
pub enum FormValue {
    String(String),
    Int32(i32),
    UnsignedInt8(u8),
    UnsignedInt32(u32),
    UnsignedInt64(u64),
    Float32(f32),
    Bool(bool),
    Option(Option<Box<FormValue>>),
    Vec(Vec<FormValue>),
    Nested(FormData),
}

impl FormValue {
    pub fn as_string(&self) -> &String {
        let Self::String(v) = self else {
            panic!("expected String");
        };

        v
    }

    pub fn as_i32(&self) -> i32 {
        let Self::Int32(v) = self else {
            panic!("expected Int32");
        };

        *v
    }

    pub fn as_u8(&self) -> u8 {
        let Self::UnsignedInt8(v) = self else {
            panic!("expected UnsignedInt8");
        };

        *v
    }

    pub fn as_u32(&self) -> u32 {
        let Self::UnsignedInt32(v) = self else {
            panic!("expected UnsignedInt32");
        };

        *v
    }

    pub fn as_u64(&self) -> u64 {
        let Self::UnsignedInt64(v) = self else {
            panic!("expected UnsignedInt64");
        };

        *v
    }

    pub fn as_f32(&self) -> f32 {
        let Self::Float32(v) = self else {
            panic!("expected Float32");
        };

        *v
    }

    pub fn as_bool(&self) -> bool {
        let Self::Bool(v) = self else {
            panic!("expected Bool");
        };

        *v
    }

    pub fn as_option(&self) -> Option<&Self> {
        let Self::Option(v) = self else {
            panic!("expected Option");
        };

        v.as_deref()
    }

    pub fn as_option_mut(&mut self) -> Option<&mut Self> {
        let Self::Option(v) = self else {
            panic!("expected Option");
        };

        v.as_deref_mut()
    }

    pub fn as_vec(&self) -> &Vec<Self> {
        let Self::Vec(v) = self else {
            panic!("expected Vec");
        };

        v
    }

    pub fn as_nested(&self) -> &FormData {
        let Self::Nested(v) = self else {
            panic!("expected Nested");
        };

        v
    }

    pub fn as_nested_mut(&mut self) -> &mut FormData {
        let Self::Nested(v) = self else {
            panic!("expected Nested");
        };

        v
    }
}

impl std::fmt::Display for FormValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::String(v) => write!(f, "{}", v),
            Self::Int32(v) => write!(f, "{}", v),
            Self::UnsignedInt8(v) => write!(f, "{}", v),
            Self::UnsignedInt32(v) => write!(f, "{}", v),
            Self::UnsignedInt64(v) => write!(f, "{}", v),
            Self::Float32(v) => write!(f, "{}", v),
            Self::Bool(v) => {
                if *v {
                    write!(f, "true")
                } else {
                    write!(f, "false")
                }
            }
            Self::Option(v) => write!(f, "{:?}", v),
            Self::Vec(v) => write!(f, "{:?}", v),
            Self::Nested(v) => write!(f, "{:?}", v),
        }
    }
}

impl From<String> for FormValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<i32> for FormValue {
    fn from(value: i32) -> Self {
        Self::Int32(value)
    }
}

impl From<u8> for FormValue {
    fn from(value: u8) -> Self {
        Self::UnsignedInt8(value)
    }
}

impl From<u32> for FormValue {
    fn from(value: u32) -> Self {
        Self::UnsignedInt32(value)
    }
}

impl From<u64> for FormValue {
    fn from(value: u64) -> Self {
        Self::UnsignedInt64(value)
    }
}

impl From<f32> for FormValue {
    fn from(value: f32) -> Self {
        Self::Float32(value)
    }
}

impl From<bool> for FormValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

#[derive(Debug, Clone)]
pub struct FormItem {
    pub key: FormItemKey,
    pub title: &'static str,
    pub description: Option<&'static str>,
    pub kind: FormItemKind,
    pub formatter: fn(&FormValue) -> String,
    pub validator: fn(&FormValue) -> anyhow::Result<()>,
}

impl FormItem {
    pub fn new(
        key: FormItemKey,
        title: &'static str,
        description: Option<&'static str>,
        kind: FormItemKind,
        formatter: fn(&FormValue) -> String,
        validator: fn(&FormValue) -> anyhow::Result<()>,
    ) -> Self {
        Self {
            key,
            title,
            description,
            kind,
            formatter,
            validator,
        }
    }
}

#[derive(Debug, Clone)]
pub enum FormItemKey {
    Simple(&'static str),
    Custom {
        getter: fn(&FormData) -> &FormValue,
        setter: fn(&mut FormData, FormValue),
    },
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum FormItemKind {
    InputOfString,
    InputOfInt32,
    InputOfUnsignedInt8,
    InputOfUnsignedInt32,
    InputOfUnsignedInt64,
    InputOfFloat32,
    Enum(Vec<FormEnumVariant>),
    BitMask(Vec<FormBitMaskVariant>),
    Switch,
    Button { event: AppEvent, confirm: bool },
}

impl FormItemKind {
    pub fn is_enum(&self) -> bool {
        return matches!(self, Self::Enum(_));
    }
}

#[derive(Debug, Clone)]
pub struct FormEnumVariant {
    pub title: String,
    pub value: FormValue,
}

impl FormEnumVariant {
    pub fn new<S: Into<String>, F: Into<FormValue>>(title: S, value: F) -> Self {
        Self {
            title: title.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FormBitMaskVariant {
    pub title: String,
    pub value: u32,
}

impl FormBitMaskVariant {
    pub fn new<S: Into<String>>(title: S, value: u32) -> Self {
        Self {
            title: title.into(),
            value,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DeviceConfig {
    pub bluetooth: Option<config::BluetoothConfig>,
    pub device: Option<config::DeviceConfig>,
    pub device_ui: Option<DeviceUiConfig>,
    pub display: Option<config::DisplayConfig>,
    pub lora: Option<config::LoRaConfig>,
    pub network: Option<config::NetworkConfig>,
    pub position: Option<config::PositionConfig>,
    pub power: Option<config::PowerConfig>,
    pub security: Option<config::SecurityConfig>,
    pub sessionkey: Option<config::SessionkeyConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceModuleConfig {
    pub ambient_lighting: Option<module_config::AmbientLightingConfig>,
    pub audio: Option<module_config::AudioConfig>,
    pub canned_message: Option<module_config::CannedMessageConfig>,
    pub detection_sensor: Option<module_config::DetectionSensorConfig>,
    pub external_notification: Option<module_config::ExternalNotificationConfig>,
    pub mqtt: Option<module_config::MqttConfig>,
    pub neighbor: Option<module_config::NeighborInfoConfig>,
    pub paxcounter: Option<module_config::PaxcounterConfig>,
    pub range_test: Option<module_config::RangeTestConfig>,
    pub remote_hardware: Option<module_config::RemoteHardwareConfig>,
    pub serial: Option<module_config::SerialConfig>,
    pub status_message: Option<module_config::StatusMessageConfig>,
    pub store_forward: Option<module_config::StoreForwardConfig>,
    pub telemetry: Option<module_config::TelemetryConfig>,
    pub traffic_management: Option<module_config::TrafficManagementConfig>,
}
