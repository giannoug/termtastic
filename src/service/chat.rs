use meshtastic::{
    protobufs::{from_radio::PayloadVariant, mesh_packet, routing, PortNum, Routing},
    Message as _,
};
use tokio::sync::{broadcast, mpsc, watch};
use tokio_graceful_shutdown::SubsystemHandle;
use tracing_unwrap::OptionExt;

use crate::state::StateSnapshot;
use crate::types::{MessageReaction, UNKNOWN_NODE};
use crate::{
    meshtastic::types::{CommandToMeshtastic, MeshtasticEvent, TextMessage},
    state::StateAction,
    types::{AppEvent, Channel, ChannelRole, Message, Toast},
};

pub struct ChatService {
    app_event_rx: broadcast::Receiver<AppEvent>,
    state_rx: watch::Receiver<StateSnapshot>,
    state_action_tx: mpsc::UnboundedSender<StateAction>,
    meshtastic_command_tx: mpsc::UnboundedSender<CommandToMeshtastic>,
    meshtastic_event_rx: broadcast::Receiver<MeshtasticEvent>,
}

impl ChatService {
    pub fn new(
        app_event_rx: broadcast::Receiver<AppEvent>,
        state_rx: watch::Receiver<StateSnapshot>,
        state_action_tx: mpsc::UnboundedSender<StateAction>,
        meshtastic_command_tx: mpsc::UnboundedSender<CommandToMeshtastic>,
        meshtastic_event_rx: broadcast::Receiver<MeshtasticEvent>,
    ) -> Self {
        Self {
            app_event_rx,
            state_rx,
            state_action_tx,
            meshtastic_command_tx,
            meshtastic_event_rx,
        }
    }

    pub async fn run(mut self, subsys: &mut SubsystemHandle) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                Ok(event) = self.app_event_rx.recv() => self.handle_app_event(event)?,
                Ok(event) = self.meshtastic_event_rx.recv() => self.handle_meshtastic_event(event)?,
                _ = subsys.on_shutdown_requested() => {
                    tracing::info!("shutdown");
                    break;
                }
            }
        }

        Ok(())
    }

    fn handle_app_event(&self, event: AppEvent) -> anyhow::Result<()> {
        let snapshot = &self.state_rx.borrow();

        match event {
            AppEvent::ChannelSelected(number) => {
                self.state_action_tx.send(StateAction::ChannelActiveSet(number))?;
            }
            AppEvent::SwitchChannelRequested => {
                self.state_action_tx.send(StateAction::ChannelActiveUnset)?;
            }
            AppEvent::ChatMessageSubmitted { text, reply_message_id } => match snapshot.state.get_active_channel() {
                Some(Channel {
                    key,
                    role: ChannelRole::Primary | ChannelRole::Secondary,
                    ..
                }) => {
                    self.meshtastic_command_tx
                        .send(CommandToMeshtastic::SendBroadcastTextMessage {
                            my_node_num: snapshot.state.my_node_key.expect_or_log("my node key should exists"),
                            channel_id: *key,
                            reply_message_id,
                            text: TextMessage::Text(text),
                        })?;
                }
                Some(Channel {
                    key,
                    role: ChannelRole::Direct,
                    ..
                }) => {
                    self.meshtastic_command_tx
                        .send(CommandToMeshtastic::SendDirectTextMessage {
                            my_node_num: snapshot.state.my_node_key.expect_or_log("my node key should exists"),
                            node_num: *key,
                            reply_message_id,
                            text: TextMessage::Text(text),
                        })?;
                }
                _ => {}
            },
            AppEvent::ChatReactionSubmitted {
                emoji,
                reply_message_id,
            } => match snapshot.state.get_active_channel() {
                Some(Channel {
                    key,
                    role: ChannelRole::Primary | ChannelRole::Secondary,
                    ..
                }) => {
                    self.meshtastic_command_tx
                        .send(CommandToMeshtastic::SendBroadcastTextMessage {
                            my_node_num: snapshot.state.my_node_key.expect_or_log("my node key should exists"),
                            channel_id: *key,
                            reply_message_id,
                            text: TextMessage::Emoji(emoji),
                        })?;
                }
                Some(Channel {
                    key,
                    role: ChannelRole::Direct,
                    ..
                }) => {
                    self.meshtastic_command_tx
                        .send(CommandToMeshtastic::SendDirectTextMessage {
                            my_node_num: snapshot.state.my_node_key.expect_or_log("my node key should exists"),
                            node_num: *key,
                            reply_message_id,
                            text: TextMessage::Emoji(emoji),
                        })?;
                }
                _ => {}
            },
            _ => {}
        }

        Ok(())
    }

    fn handle_meshtastic_event(&mut self, event: MeshtasticEvent) -> anyhow::Result<()> {
        match event {
            MeshtasticEvent::IncomingPacket(packet) => self.handle_meshtastic_packet(packet)?,
            MeshtasticEvent::MessageRejected(e) => {
                tracing::error!("message rejected: {}", e);

                self.state_action_tx
                    .send(StateAction::Toast(Toast::error("message rejected by node")))?;
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_meshtastic_packet(&mut self, payload_variant: PayloadVariant) -> anyhow::Result<()> {
        match payload_variant {
            PayloadVariant::Channel(ch) => {
                self.state_action_tx
                    .send(StateAction::ChannelSet(ch.index as u32, Channel::from(&ch)))?;
            }
            PayloadVariant::Packet(packet) => match &packet.payload_variant {
                Some(mesh_packet::PayloadVariant::Decoded(data)) => match data.portnum() {
                    PortNum::RoutingApp => match Routing::decode(&*data.payload) {
                        Ok(Routing {
                            variant: Some(routing::Variant::ErrorReason(e)),
                        }) => {
                            let snapshot = &self.state_rx.borrow();

                            if let Some(my) = snapshot.state.my_node_key
                                && packet.to == my
                            {
                                let channel_key = if packet.to == packet.from {
                                    packet.channel
                                } else {
                                    packet.from
                                };

                                self.state_action_tx.send(StateAction::MessageErrorSet {
                                    channel_key,
                                    message_id: data.request_id,
                                    error: Some(routing::Error::try_from(e).expect("invalid routing error")),
                                })?;
                            }
                        }
                        Err(e) => {
                            tracing::debug!("can't decode RoutingApp payload: {:?}", e);
                        }
                        _ => {}
                    },
                    PortNum::TextMessageApp | PortNum::ReplyApp => {
                        let snapshot = &self.state_rx.borrow();

                        let channel_key = match (packet.from, packet.to, snapshot.state.my_node_key) {
                            (_, 0 | u32::MAX, _) => packet.channel,
                            (from, to, Some(my)) if to == my => {
                                self.state_action_tx
                                    .send(StateAction::ChannelSet(from, Channel::direct(from)))?;

                                from
                            }
                            (from, to, Some(my)) if from == my => {
                                self.state_action_tx
                                    .send(StateAction::ChannelSet(to, Channel::direct(to)))?;

                                to
                            }
                            _ => return Ok(()),
                        };

                        if data.emoji > 0 {
                            match MessageReaction::try_from((&packet, data)) {
                                Ok(reaction) => self.state_action_tx.send(StateAction::MessageReactionAdd {
                                    channel_key,
                                    message_id: data.reply_id,
                                    reaction,
                                })?,
                                Err(e) => tracing::warn!(
                                    packet_id = packet.id,
                                    node_from = packet.from,
                                    node_to = packet.to,
                                    channel = packet.channel,
                                    "can't convert packet into message: {}",
                                    e
                                ),
                            };

                            return Ok(());
                        }

                        match Message::try_from((&packet, data)) {
                            Ok(message) => self
                                .state_action_tx
                                .send(StateAction::MessageAdd(channel_key, message))?,
                            Err(e) => tracing::warn!(
                                packet_id = packet.id,
                                node_from = packet.from,
                                node_to = packet.to,
                                channel = packet.channel,
                                "can't convert packet into message: {}",
                                e
                            ),
                        };
                    }
                    PortNum::RangeTestApp => {
                        let snapshot = &self.state_rx.borrow();
                        let text = String::from_utf8(data.payload.clone()).unwrap_or("can't decode payload".to_owned());
                        let node = snapshot.state.nodes.get(&packet.from).unwrap_or(&UNKNOWN_NODE);

                        tracing::info!(
                            packet_id = packet.id,
                            node_from = packet.from,
                            node_to = packet.to,
                            channel = packet.channel,
                            "RANGE TEST from [{}] {} ({}), text: \"{}\", hops: {}, snr: {}, rssi: {}",
                            node.short_name(),
                            node.long_name(),
                            node.hw_model(),
                            text,
                            packet.hop_start.saturating_sub(packet.hop_limit),
                            packet.rx_snr,
                            packet.rx_rssi,
                        );
                    }
                    _ => {}
                },
                _ => {}
            },
            _ => {}
        }

        Ok(())
    }
}
