use std::time::Duration;

use hostaddr::HostAddr;
use meshtastic::protobufs::{config, module_config, routing, User};

use crate::types::{
    AppConfig, Channel, Device, FormData, FormId, FormItemKey, FormValue, LogRecord, Message, MessageReaction, Node,
    NodesSortBy, Toast, UiConfig,
};

#[derive(Debug)]
pub enum StateAction {
    AppConfigApply(AppConfig),
    ChannelActiveSet(u32),
    ChannelActiveUnset,
    ChannelSet(u32, Channel),
    ChannelPurge(u32),
    ConnectionFail(String),
    ConnectionStart,
    ConnectionStop,
    ConnectionSuccess,
    ReconnectionBackoffSet(Duration),
    DbInitStart,
    DbInitFail(String),
    DbInitSuccess,
    DbLoadStart,
    DbLoadFail(String),
    DbLoadSuccess {
        nodes: Vec<Node>,
    },
    DeviceActiveSet(Device),
    DevicesAddTcp(HostAddr<String>),
    DeviceDiscoveringStart,
    DeviceDiscoveringFail(String),
    DeviceDiscoveringDone(Vec<Device>),
    DeviceConfigSet(config::PayloadVariant),
    DeviceModuleConfigSet(module_config::PayloadVariant),
    DeviceUserSet(User),
    DevicesRemoveTcp(HostAddr<String>),
    LogRecordAdd(LogRecord),
    DirectChatStart(u32),
    MessageAdd(u32, Message),
    MessageReactionAdd {
        channel_key: u32,
        message_id: u32,
        reaction: MessageReaction,
    },
    MessageErrorSet {
        channel_key: u32,
        message_id: u32,
        error: Option<routing::Error>,
    },
    MyNodeKeySet(u32),
    NodeInit(Node),
    NodeInitTemporary(Node),
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
