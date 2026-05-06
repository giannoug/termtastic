use std::time::Duration;

use meshtastic::protobufs::{admin_message, AdminMessage};
use meshtastic::{
    api::ConnectedStreamApi,
    packet::{PacketDestination, PacketRouter},
    protobufs::{from_radio, Config, FromRadio, MeshPacket, ModuleConfig, PortNum},
    types::{EncodedMeshPacketData, MeshChannel, NodeId},
    Message,
};
use tokio::time::sleep;
use tokio::{
    sync::{
        broadcast::{self, error::SendError},
        mpsc,
    },
    time::timeout,
};
use tokio_graceful_shutdown::{ErrorAction, NestedSubsystem, SubsystemBuilder, SubsystemHandle};
use tracing_unwrap::OptionExt;

use crate::meshtastic::{
    connect_via_ble, connect_via_serial, connect_via_tcp, types::{CommandToMeshtastic, MeshtasticEvent, TextMessage},
    RadioService,
};

const CONNECTION_TIMEOUT_SECS: u64 = 2;
const SAVE_CONFIG_TIMEOUT_SECS: u64 = 5;
const SAVE_CONFIG_AFTER_PAUSE_MILLIS: u64 = 100;
const SAVE_SET_CHANNEL_DELAY_MILLIS: u64 = 80;

pub struct MeshtasticService {
    command_rx: mpsc::UnboundedReceiver<CommandToMeshtastic>,
    event_tx: broadcast::Sender<MeshtasticEvent>,
    event_rx: broadcast::Receiver<MeshtasticEvent>,
    stream_api: Option<ConnectedStreamApi>,
    radio_subsys: Option<NestedSubsystem>,
}

impl MeshtasticService {
    pub fn new() -> (
        Self,
        mpsc::UnboundedSender<CommandToMeshtastic>,
        broadcast::Receiver<MeshtasticEvent>,
    ) {
        let (command_tx, command_rx) = mpsc::unbounded_channel::<CommandToMeshtastic>();
        let (event_tx, event_rx) = broadcast::channel::<MeshtasticEvent>(100);

        (
            Self {
                command_rx,
                event_tx: event_tx.clone(),
                event_rx: event_rx.resubscribe(),
                stream_api: None,
                radio_subsys: None,
            },
            command_tx.clone(),
            event_rx,
        )
    }

    pub async fn run(mut self, subsys: &mut SubsystemHandle) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                Ok(event) = self.event_rx.recv() => self.handle_meshtastic_event(event).await?,
                Some(cmd) = self.command_rx.recv() => self.handle_command(cmd, subsys).await?,
                _ = subsys.on_shutdown_requested() => {
                    tracing::info!("shutdown");
                    self.disconnect().await?;
                    self.event_tx.send(MeshtasticEvent::Disconnected)?;

                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_meshtastic_event(&mut self, event: MeshtasticEvent) -> anyhow::Result<()> {
        match event {
            MeshtasticEvent::RadioStopped => {
                self.disconnect().await?;

                self.event_tx.send(MeshtasticEvent::ConnectionError(
                    "connection channel was closed unexpectedly".to_owned(),
                ))?;
            }
            _ => {}
        }

        Ok(())
    }

    async fn handle_command(&mut self, cmd: CommandToMeshtastic, subsys: &mut SubsystemHandle) -> anyhow::Result<()> {
        match cmd {
            CommandToMeshtastic::ConnectViaTcp(hostaddr) => {
                match timeout(Duration::from_secs(CONNECTION_TIMEOUT_SECS), connect_via_tcp(hostaddr)).await {
                    Ok(Ok((radio_rx, stream_api))) => {
                        self.handle_connection(radio_rx, stream_api, subsys);
                        self.event_tx.send(MeshtasticEvent::Connected)?;
                    }
                    Ok(Err(e)) => {
                        tracing::error!("can't connect via TCP: {:?}", e);

                        self.event_tx.send(MeshtasticEvent::ConnectionError(e.to_string()))?;
                    }
                    Err(e) => {
                        tracing::error!("connection timeout: {:?}", e);

                        self.event_tx.send(MeshtasticEvent::ConnectionError(e.to_string()))?;
                    }
                };
            }
            CommandToMeshtastic::ConnectViaBle(address) => {
                match timeout(Duration::from_secs(CONNECTION_TIMEOUT_SECS), connect_via_ble(address)).await {
                    Ok(Ok((radio_rx, stream_api))) => {
                        self.handle_connection(radio_rx, stream_api, subsys);

                        self.event_tx.send(MeshtasticEvent::Connected)?;
                    }
                    Ok(Err(e)) => {
                        tracing::error!("can't connect via BLE: {:?}", e);

                        self.event_tx.send(MeshtasticEvent::ConnectionError(e.to_string()))?;
                    }
                    Err(e) => {
                        tracing::error!("connection timeout: {:?}", e);

                        self.event_tx.send(MeshtasticEvent::ConnectionError(e.to_string()))?;
                    }
                };
            }
            CommandToMeshtastic::ConnectViaSerial(address) => {
                match timeout(
                    Duration::from_secs(CONNECTION_TIMEOUT_SECS),
                    connect_via_serial(address),
                )
                .await
                {
                    Ok(Ok((radio_rx, stream_api))) => {
                        self.handle_connection(radio_rx, stream_api, subsys);
                        self.event_tx.send(MeshtasticEvent::Connected)?;
                    }
                    Ok(Err(e)) => {
                        tracing::error!("can't connect via serial: {:?}", e);

                        self.event_tx.send(MeshtasticEvent::ConnectionError(e.to_string()))?;
                    }
                    Err(e) => {
                        tracing::error!("connection timeout: {:?}", e);

                        self.event_tx.send(MeshtasticEvent::ConnectionError(e.to_string()))?;
                    }
                };
            }
            CommandToMeshtastic::Disconnect => {
                self.disconnect().await?;
                self.event_tx.send(MeshtasticEvent::Disconnected)?;
            }
            CommandToMeshtastic::Reboot { my_node_id, secs } => {
                self.send_admin_message(my_node_id, admin_message::PayloadVariant::RebootSeconds(secs))
                    .await?;
            }
            CommandToMeshtastic::SendBroadcastTextMessage {
                my_node_id,
                channel_id,
                reply_message_id,
                text,
            } => {
                let mut packet_router = RetransmitPacketRouter {
                    my_node_id,
                    event_tx: &self.event_tx,
                };

                match self
                    .stream_api
                    .as_mut()
                    .expect_or_log("should be connected")
                    .send_mesh_packet(
                        &mut packet_router,
                        EncodedMeshPacketData::new(match &text {
                            TextMessage::Text(v) => v.clone().into_bytes(),
                            TextMessage::Emoji(e) => e.glyph.as_bytes().to_vec(),
                        }),
                        PortNum::TextMessageApp,
                        PacketDestination::Broadcast,
                        MeshChannel::from(channel_id),
                        true,                                               // want_ack
                        false,                                              // want_response
                        true,                                               // echo_response
                        reply_message_id,                                   // reply_id
                        matches!(text, TextMessage::Emoji(_)).then_some(1), // emoji
                    )
                    .await
                {
                    Ok(()) => self.event_tx.send(MeshtasticEvent::MessageAccepted)?,
                    Err(e) => self.event_tx.send(MeshtasticEvent::MessageRejected(e.to_string()))?,
                };
            }
            CommandToMeshtastic::SendDirectTextMessage {
                my_node_id,
                node_id,
                reply_message_id,
                text,
            } => {
                let mut packet_router = RetransmitPacketRouter {
                    my_node_id,
                    event_tx: &self.event_tx,
                };

                match self
                    .stream_api
                    .as_mut()
                    .expect_or_log("should be connected")
                    .send_mesh_packet(
                        &mut packet_router,
                        EncodedMeshPacketData::new(match &text {
                            TextMessage::Text(v) => v.clone().into_bytes(),
                            TextMessage::Emoji(e) => e.glyph.as_bytes().to_vec(),
                        }),
                        PortNum::TextMessageApp,
                        PacketDestination::Node(NodeId::from(node_id)),
                        MeshChannel::from(0),
                        true,                                               // want_ack
                        false,                                              // want_response
                        true,                                               // echo_response
                        reply_message_id,                                   // reply_id
                        matches!(text, TextMessage::Emoji(_)).then_some(1), // emoji
                    )
                    .await
                {
                    Ok(()) => self.event_tx.send(MeshtasticEvent::MessageAccepted)?,
                    Err(e) => self.event_tx.send(MeshtasticEvent::MessageRejected(e.to_string()))?,
                };
            }
            CommandToMeshtastic::SaveConfig {
                form_id,
                my_node_id,
                config,
            } => {
                let api = self.stream_api.as_mut().expect_or_log("should be connected");

                let mut packet_router = RetransmitPacketRouter {
                    my_node_id,
                    event_tx: &self.event_tx,
                };

                match async {
                    api.start_config_transaction().await?;

                    api.update_config(
                        &mut packet_router,
                        Config {
                            payload_variant: Some(config),
                        },
                    )
                    .await?;

                    api.commit_config_transaction().await?;

                    sleep(Duration::from_millis(SAVE_CONFIG_AFTER_PAUSE_MILLIS)).await;

                    Ok::<(), anyhow::Error>(())
                }
                .await
                {
                    Ok(_) => {
                        self.event_tx.send(MeshtasticEvent::ConfigSaved(form_id))?;
                    }
                    Err(e) => {
                        tracing::error!("save config error: {:?}", e);

                        self.event_tx
                            .send(MeshtasticEvent::ConfigSaveError(form_id, e.to_string()))?;
                    }
                }
            }
            CommandToMeshtastic::SaveModuleConfig {
                form_id,
                my_node_id,
                config,
            } => {
                let api = self.stream_api.as_mut().expect_or_log("should be connected");

                let mut packet_router = RetransmitPacketRouter {
                    my_node_id,
                    event_tx: &self.event_tx,
                };

                match timeout(Duration::from_secs(SAVE_CONFIG_TIMEOUT_SECS), async {
                    api.start_config_transaction().await?;

                    api.update_module_config(
                        &mut packet_router,
                        ModuleConfig {
                            payload_variant: Some(config),
                        },
                    )
                    .await?;

                    api.commit_config_transaction().await?;

                    sleep(Duration::from_millis(SAVE_CONFIG_AFTER_PAUSE_MILLIS)).await;

                    Ok::<(), anyhow::Error>(())
                })
                .await
                {
                    Ok(Ok(_)) => {
                        self.event_tx.send(MeshtasticEvent::ConfigSaved(form_id))?;
                    }
                    Ok(Err(e)) => {
                        tracing::error!("save config error: {:?}", e);

                        self.event_tx
                            .send(MeshtasticEvent::ConfigSaveError(form_id, e.to_string()))?;
                    }
                    Err(e) => {
                        tracing::error!("save config timeout: {:?}", e);

                        self.event_tx
                            .send(MeshtasticEvent::ConfigSaveError(form_id, e.to_string()))?;
                    }
                }
            }
            CommandToMeshtastic::SaveChannelsConfig {
                form_id,
                my_node_id,
                channels,
            } => {
                let api = self.stream_api.as_mut().expect_or_log("should be connected");

                let mut packet_router = RetransmitPacketRouter {
                    my_node_id,
                    event_tx: &self.event_tx,
                };

                match timeout(Duration::from_secs(SAVE_CONFIG_TIMEOUT_SECS), async {
                    api.start_config_transaction().await?;

                    for channel in channels {
                        api.update_channel_config(&mut packet_router, channel).await?;
                        sleep(Duration::from_millis(SAVE_SET_CHANNEL_DELAY_MILLIS)).await;
                    }

                    api.commit_config_transaction().await?;

                    sleep(Duration::from_millis(SAVE_CONFIG_AFTER_PAUSE_MILLIS)).await;

                    Ok::<(), anyhow::Error>(())
                })
                .await
                {
                    Ok(Ok(_)) => {
                        self.event_tx.send(MeshtasticEvent::ChannelsSaved(form_id))?;
                    }
                    Ok(Err(e)) => {
                        tracing::error!("save channels error: {:?}", e);

                        self.event_tx
                            .send(MeshtasticEvent::ChannelsSaveError(form_id, e.to_string()))?;
                    }
                    Err(e) => {
                        tracing::error!("save channels timeout: {:?}", e);

                        self.event_tx
                            .send(MeshtasticEvent::ChannelsSaveError(form_id, e.to_string()))?;
                    }
                }
            }
            CommandToMeshtastic::SaveUser {
                form_id,
                my_node_id,
                user,
            } => {
                let api = self.stream_api.as_mut().expect_or_log("should be connected");

                let mut packet_router = RetransmitPacketRouter {
                    my_node_id,
                    event_tx: &self.event_tx,
                };

                match timeout(Duration::from_secs(SAVE_CONFIG_TIMEOUT_SECS), async {
                    api.update_user(&mut packet_router, user).await?;

                    sleep(Duration::from_millis(SAVE_CONFIG_AFTER_PAUSE_MILLIS)).await;

                    Ok::<(), anyhow::Error>(())
                })
                .await
                {
                    Ok(Ok(_)) => {
                        self.event_tx.send(MeshtasticEvent::UserSaved(form_id))?;
                    }
                    Ok(Err(e)) => {
                        tracing::error!("save user error: {:?}", e);

                        self.event_tx
                            .send(MeshtasticEvent::UserSaveError(form_id, e.to_string()))?;
                    }
                    Err(e) => {
                        tracing::error!("save user timeout: {:?}", e);

                        self.event_tx
                            .send(MeshtasticEvent::UserSaveError(form_id, e.to_string()))?;
                    }
                }
            }
        };

        Ok(())
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        if let Some(subsys) = self.radio_subsys.take() {
            if !subsys.is_finished() {
                subsys.initiate_shutdown();
                subsys.join().await?;
            }
        }

        if let Some(stream_api) = self.stream_api.take() {
            let _ = stream_api
                .disconnect()
                .await
                .inspect_err(|e| tracing::error!("stream api disconnect error: {}", e));
        }

        Ok(())
    }

    fn handle_connection(
        &mut self,
        radio_rx: mpsc::UnboundedReceiver<FromRadio>,
        stream_api: ConnectedStreamApi,
        subsys: &mut SubsystemHandle,
    ) {
        self.stream_api = Some(stream_api);

        let event_tx = self.event_tx.clone();

        let subsys = subsys.start(
            SubsystemBuilder::new("RadioService", async |nested_subsys: &mut SubsystemHandle| {
                RadioService::new(event_tx).run(radio_rx, nested_subsys).await
            })
            .on_failure(ErrorAction::CatchAndLocalShutdown),
        );

        self.radio_subsys = Some(subsys);
    }

    async fn send_admin_message(
        &mut self,
        my_node_id: u32,
        payload: admin_message::PayloadVariant,
    ) -> anyhow::Result<()> {
        let mut packet_router = RetransmitPacketRouter {
            my_node_id,
            event_tx: &self.event_tx,
        };

        let packet = AdminMessage {
            payload_variant: Some(payload),
            session_passkey: Vec::new(),
        };

        self.stream_api
            .as_mut()
            .expect_or_log("should be connected")
            .send_mesh_packet(
                &mut packet_router,
                EncodedMeshPacketData::new(packet.encode_to_vec().into()),
                PortNum::AdminApp,
                PacketDestination::Local,
                MeshChannel::new(0)?,
                true,  // want_ack
                false, // want_response
                true,  // echo_response
                None,  // reply_id
                None,  // emoji
            )
            .await?;

        Ok(())
    }
}

struct RetransmitPacketRouter<'a> {
    pub my_node_id: u32,
    pub event_tx: &'a broadcast::Sender<MeshtasticEvent>,
}

#[derive(thiserror::Error, Debug)]
enum RetransmitPacketRouterErr {
    #[error("event send error: {0}")]
    EventSendError(#[from] SendError<MeshtasticEvent>),
}

impl<'a> PacketRouter<(), RetransmitPacketRouterErr> for RetransmitPacketRouter<'a> {
    fn handle_packet_from_radio(&mut self, packet: FromRadio) -> Result<(), RetransmitPacketRouterErr> {
        if let Some(payload) = packet.payload_variant {
            self.event_tx.send(MeshtasticEvent::IncomingPacket(payload))?;
        }

        Ok(())
    }

    fn handle_mesh_packet(&mut self, packet: MeshPacket) -> Result<(), RetransmitPacketRouterErr> {
        self.event_tx
            .send(MeshtasticEvent::IncomingPacket(from_radio::PayloadVariant::Packet(
                packet,
            )))?;

        Ok(())
    }

    fn source_node_id(&self) -> NodeId {
        NodeId::new(self.my_node_id)
    }
}
