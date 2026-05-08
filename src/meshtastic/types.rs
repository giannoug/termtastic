use crate::types::FormId;
use emoji::Emoji;
use hostaddr::HostAddr;
use meshtastic::protobufs::{config, from_radio, module_config};

#[derive(Debug, Clone)]
pub enum MeshtasticEvent {
    Connected,
    ConnectionError(String),
    Disconnected,
    IncomingPacket(from_radio::PayloadVariant),
    MessageAccepted,
    MessageRejected(String),
    RadioStopped,
    #[allow(dead_code)]
    ConfigSaveError(FormId, String),
    ConfigSaved(FormId),
    #[allow(dead_code)]
    ChannelsSaveError(FormId, String),
    ChannelsSaved(FormId),
    #[allow(dead_code)]
    UserSaveError(FormId, String),
    UserSaved(FormId),
    NodeInfoBroadcastSent,
    NodeInfoBroadcastFailed(String),
}

#[derive(Debug, Clone)]
pub enum TextMessage {
    Text(String),
    Emoji(&'static Emoji),
}

#[derive(Debug, Clone)]
pub enum CommandToMeshtastic {
    ConnectViaTcp(HostAddr<String>),
    ConnectViaBle(String),
    ConnectViaSerial(String),
    Disconnect,
    Reboot {
        my_node_id: u32,
        secs: i32,
    },
    Shutdown {
        my_node_id: u32,
        secs: i32,
    },
    SendBroadcastTextMessage {
        my_node_id: u32,
        channel_id: u32,
        reply_message_id: Option<u32>,
        text: TextMessage,
    },
    SendDirectTextMessage {
        my_node_id: u32,
        node_id: u32,
        reply_message_id: Option<u32>,
        text: TextMessage,
    },
    BroadcastNodeInfo {
        channel_id: u32,
        user: meshtastic::protobufs::User,
    },
    SaveConfig {
        form_id: FormId,
        my_node_id: u32,
        config: config::PayloadVariant,
    },
    SaveModuleConfig {
        form_id: FormId,
        my_node_id: u32,
        config: module_config::PayloadVariant,
    },
    SaveChannelsConfig {
        form_id: FormId,
        my_node_id: u32,
        channels: Vec<meshtastic::protobufs::Channel>,
    },
    SaveUser {
        form_id: FormId,
        my_node_id: u32,
        user: meshtastic::protobufs::User,
    },
}
