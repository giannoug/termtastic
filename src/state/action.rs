use std::time::Duration;

use meshtastic::protobufs::{config, module_config, routing};

use crate::types::{
    AppConfig, Channel, Chat, Device, FormData, FormId, FormItemKey, FormValue, LogRecord, Message, Node,
    NodeTelemetry, NodeUser, NodesSortBy, Toast, UiConfig,
};

#[derive(Debug)]
pub enum StateAction {
    AppConfigApply(AppConfig),
    ActiveChatSet(Chat),
    ActiveChatUnset,
    ChannelSet(u32, Channel),
    ChatPurge(Chat),
    ConnectionFail(String),
    ConnectionStart,
    ConnectionStop,
    ConnectionLoadConfig,
    ConnectionSuccess,
    ReconnectionBackoffSet(Duration),
    DbNodesLoad(Vec<Node>),
    DeviceActiveSet(Device),
    DevicesAdd(Device),
    DevicesRemove(Device),
    DevicesDiscoveredAdd(Device),
    DeviceDiscoveringStart,
    DeviceDiscoveringDone,
    DeviceConfigSet(config::PayloadVariant),
    DeviceModuleConfigSet(module_config::PayloadVariant),
    DeviceMetadataSet(meshtastic::protobufs::DeviceMetadata),
    DeviceCannedMessagesSet(String),
    LogRecordAdd(LogRecord),
    DirectChatStart(u32),
    MessageAdd(Message),
    MessageErrorSet {
        message_id: u32,
        error: Option<routing::Error>,
    },
    MyNodeKeySet(u32),
    NodesStashPush(Node),
    NodesStashCapSet(u32),
    NodesStashFlush,
    NodeInit(Node),
    NodeInfoSet(u32),
    NodeInfoUnset,
    NodeUpdate(Node),
    NodeUpdateLastHeard {
        node_key: u32,
        hops: u32,
        snr: f32,
        rssi: i32,
    },
    NodeDelete(u32),
    NodeFavoriteSet(u32, bool),
    NodeOwnerSet(NodeUser),
    NodeLastTelemetrySet(NodeTelemetry),
    NodesSortBySet(NodesSortBy),
    NodesFilterSet(String),
    NodesOnlineSet(u16),
    RxTrigger,
    SplashLogo,
    TabSwitchToNext,
    TabSwitchToPrevious,
    Toast(Toast),
    SettingsFormLoadingStart {
        id: FormId,
    },
    SettingsFormLoadingFail {
        id: FormId,
        error: String,
    },
    SettingsFormLoadingDone {
        id: FormId,
        data: FormData,
    },
    SettingsFormSavingStart {
        id: FormId,
    },
    SettingsFormSavingFailed {
        id: FormId,
    },
    SettingsFormClose,
    SettingsFormReset,
    SettingsFormValueSet {
        key: FormItemKey,
        value: FormValue,
    },
    UiConfigSet {
        config: UiConfig,
    },
}
