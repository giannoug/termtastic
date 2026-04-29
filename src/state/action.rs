use std::time::Duration;

use hostaddr::HostAddr;
use meshtastic::protobufs::{User, config, module_config, routing};

use crate::types::{
    AppConfig, Channel, Device, FormData, FormId, FormValue, LogRecord, Message, Node, NodesSortBy, Tab, Toast,
};

#[derive(Debug)]
pub enum StateAction {
    AppConfigApply(AppConfig),
    ChannelActiveSet(u32),
    ChannelActiveUnset,
    ChannelEnsure(u32, Channel),
    ConnectionFail(String),
    ConnectionStart,
    ConnectionStop,
    ConnectionSuccess,
    ReconnectionBackoffSet(Duration),
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
        emoji: String,
        node_key: u32,
    },
    MessageErrorSet {
        channel_key: u32,
        message_id: u32,
        error: Option<routing::Error>,
    },
    MyNodeKeySet(u32),
    NodeAdd(Node),
    NodeUpdateLastHeard {
        node_key: u32,
        hops: u32,
        snr: f32,
    },
    NodesSortBySet(NodesSortBy),
    NodesFilterSet(String),
    NodesOnlineSet(u16),
    RxTrigger,
    SplashLogo,
    #[allow(dead_code)]
    TabSwitchTo(Tab),
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
    SettingsFormSavingDone,
    SettingsFormClose,
    SettingsFormReset,
    SettingsFormValueSet {
        key: &'static str,
        value: FormValue,
    },
}
