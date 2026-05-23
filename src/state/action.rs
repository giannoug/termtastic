use std::time::Duration;

use meshtastic::protobufs::{config, module_config, routing};

use crate::types::{
    AppConfig, Channel, Chat, Device, FormData, FormId, FormItemKey, FormValue, LogRecord, Message, Node, NodeUser,
    NodesSortBy, Toast, UiConfig,
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
    ConnectionSuccess,
    ReconnectionBackoffSet(Duration),
    DbDataLoaded {
        nodes: Vec<Node>,
    },
    DeviceActiveSet(Device),
    DevicesAdd(Device),
    DevicesRemove(Device),
    DevicesDiscoveredAdd(Device),
    DeviceDiscoveringStart,
    DeviceDiscoveringFail(String),
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
    NodeInit(Node),
    NodeInfoPopupSetKey(u32),
    NodeInfoPopupUnsetKey,
    NodeUpdate(Node),
    NodeUpdateLastHeard {
        node_key: u32,
        hops: u32,
        snr: f32,
        rssi: i32,
    },
    NodeDelete(u32),
    NodeOwnerSet(NodeUser),
    NodesSortBySet(NodesSortBy),
    NodesFilterSet(String),
    NodesOnlineSet(u16),
    RxTrigger,
    SplashLogo,
    TabSwitchToNext,
    TabSwitchToPrevious,
    FrameCleared,
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
