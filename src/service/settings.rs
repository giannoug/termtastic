use std::sync::LazyLock;

use meshtastic::Message;
use meshtastic::protobufs::config::{
    self, BluetoothConfig, DeviceConfig, DisplayConfig, LoRaConfig, PositionConfig, PowerConfig,
};
use meshtastic::protobufs::module_config::{
    ExternalNotificationConfig, MqttConfig, RangeTestConfig, SerialConfig, StoreForwardConfig,
};
use meshtastic::protobufs::{
    AdminMessage, Config, ModuleConfig, PortNum, User, admin_message, from_radio, mesh_packet, module_config,
};
use tokio::sync::{broadcast, mpsc, watch};
use tokio_graceful_shutdown::SubsystemHandle;

use crate::serde::{from_formdata, to_formdata};
use crate::types::SettingsItem;
use crate::types::{AppEvent, FormData, FormId, Toast};
use crate::{
    meshtastic::types::{CommandToMeshtastic, MeshtasticEvent},
    state::{State, StateAction},
};

pub static SETTINGS: LazyLock<Vec<SettingsItem>> = LazyLock::new(|| build_settings());

pub struct SettingsService {
    app_event_rx: broadcast::Receiver<AppEvent>,
    state_rx: watch::Receiver<State>,
    state_action_tx: mpsc::UnboundedSender<StateAction>,
    meshtastic_command_tx: mpsc::UnboundedSender<CommandToMeshtastic>,
    meshtastic_event_rx: broadcast::Receiver<MeshtasticEvent>,
}

impl SettingsService {
    pub fn new(
        app_event_rx: broadcast::Receiver<AppEvent>,
        state_rx: watch::Receiver<State>,
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
                Ok(event) = self.app_event_rx.recv() => self.handle_app_event(event).await?,
                Ok(event) = self.meshtastic_event_rx.recv() => self.handle_meshtastic_event(event)?,
                _ = subsys.on_shutdown_requested() => {
                    tracing::info!("shutdown");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_app_event(&self, event: AppEvent) -> anyhow::Result<()> {
        match event {
            AppEvent::SettingsFormSelected(id) => {
                self.state_action_tx
                    .send(StateAction::SettingsFormLoadingStart { id: id.clone() })?;

                match self.load_config(&id) {
                    Ok(data) => self
                        .state_action_tx
                        .send(StateAction::SettingsFormLoadingDone { id: id.clone(), data })?,
                    Err(e) => self.state_action_tx.send(StateAction::SettingsFormLoadingFail {
                        id: id.clone(),
                        error: e.to_string(),
                    })?,
                }
            }
            AppEvent::SettingsFormCancelRequested => {
                self.state_action_tx.send(StateAction::SettingsFormClose)?;
            }
            AppEvent::SettingsFormResetRequested => {
                self.state_action_tx.send(StateAction::SettingsFormReset)?;

                self.state_action_tx
                    .send(StateAction::Toast(Toast::normal("the data was reset")))?;
            }
            AppEvent::SettingsFormSaveRequested(form_id) => {
                self.state_action_tx
                    .send(StateAction::Toast(Toast::normal("saving...")))?;

                self.save_config(&form_id)?;
            }
            AppEvent::SettingsFormItemSubmitted(form_item, value) => {
                self.state_action_tx.send(StateAction::SettingsFormValueSet {
                    key: form_item.key.clone(),
                    value,
                })?;
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_meshtastic_event(&mut self, event: MeshtasticEvent) -> anyhow::Result<()> {
        match event {
            MeshtasticEvent::IncomingPacket(packet) => {
                self.handle_meshtastic_packet(packet)?;
            }
            MeshtasticEvent::ConfigSaveError(e) | MeshtasticEvent::UserSaveError(e) => {
                self.state_action_tx.send(StateAction::Toast(Toast::error(e)))?;
            }
            MeshtasticEvent::ConfigSaved | MeshtasticEvent::UserSaved => {
                self.state_action_tx
                    .send(StateAction::Toast(Toast::success("config saved")))?;

                self.state_action_tx.send(StateAction::SettingsFormSavingDone)?;
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_meshtastic_packet(&mut self, packet: from_radio::PayloadVariant) -> anyhow::Result<()> {
        match packet {
            from_radio::PayloadVariant::Config(Config {
                payload_variant: Some(variant),
            }) => {
                self.state_action_tx.send(StateAction::DeviceConfigSet(variant))?;
            }
            from_radio::PayloadVariant::ModuleConfig(ModuleConfig {
                payload_variant: Some(variant),
            }) => {
                self.state_action_tx.send(StateAction::DeviceModuleConfigSet(variant))?;
            }
            from_radio::PayloadVariant::Packet(mesh_packet) => match &mesh_packet.payload_variant {
                Some(mesh_packet::PayloadVariant::Decoded(data)) => match data.portnum() {
                    PortNum::AdminApp => match AdminMessage::decode(&*data.payload) {
                        Ok(admin_message) => match admin_message.payload_variant {
                            Some(admin_message::PayloadVariant::SetConfig(Config {
                                payload_variant: Some(variant),
                            })) => {
                                self.state_action_tx.send(StateAction::DeviceConfigSet(variant))?;
                            }
                            Some(admin_message::PayloadVariant::SetModuleConfig(ModuleConfig {
                                payload_variant: Some(variant),
                            })) => {
                                self.state_action_tx.send(StateAction::DeviceModuleConfigSet(variant))?;
                            }
                            _ => {}
                        },
                        Err(e) => {
                            tracing::debug!("can't decode AdminMessage payload: {:?}", e);
                        }
                    },
                    _ => {}
                },
                _ => {}
            },
            _ => {}
        }

        Ok(())
    }

    fn load_config(&self, id: &FormId) -> anyhow::Result<FormData> {
        let state = &self.state_rx.borrow();

        let data = match id {
            FormId::RadioLora => to_formdata(
                state
                    .device_config
                    .lora
                    .as_ref()
                    .ok_or(anyhow::anyhow!("Lora config not loaded"))?,
            )?,
            FormId::DeviceDevice => to_formdata(
                state
                    .device_config
                    .device
                    .as_ref()
                    .ok_or(anyhow::anyhow!("Device config not loaded"))?,
            )?,
            FormId::DeviceUser => to_formdata(
                state
                    .device_user
                    .as_ref()
                    .ok_or(anyhow::anyhow!("User config not loaded"))?,
            )?,
            FormId::DevicePosition => to_formdata(
                state
                    .device_config
                    .position
                    .as_ref()
                    .ok_or(anyhow::anyhow!("Position config not loaded"))?,
            )?,
            FormId::DevicePower => to_formdata(
                state
                    .device_config
                    .power
                    .as_ref()
                    .ok_or(anyhow::anyhow!("Power config not loaded"))?,
            )?,
            FormId::DeviceDisplay => to_formdata(
                state
                    .device_config
                    .display
                    .as_ref()
                    .ok_or(anyhow::anyhow!("Display config not loaded"))?,
            )?,
            FormId::DeviceBluetooth => to_formdata(
                state
                    .device_config
                    .bluetooth
                    .as_ref()
                    .ok_or(anyhow::anyhow!("Bluetooth config not loaded"))?,
            )?,
            FormId::ModuleMqtt => to_formdata(
                state
                    .device_module_config
                    .mqtt
                    .as_ref()
                    .ok_or(anyhow::anyhow!("MQTT config not loaded"))?,
            )?,
            FormId::ModuleSerial => to_formdata(
                state
                    .device_module_config
                    .serial
                    .as_ref()
                    .ok_or(anyhow::anyhow!("Serial config not loaded"))?,
            )?,
            FormId::ModuleExternalNotification => to_formdata(
                state
                    .device_module_config
                    .external_notification
                    .as_ref()
                    .ok_or(anyhow::anyhow!("External Notification config not loaded"))?,
            )?,
            FormId::ModuleStoreAndForward => to_formdata(
                state
                    .device_module_config
                    .store_forward
                    .as_ref()
                    .ok_or(anyhow::anyhow!("Store & Forward config not loaded"))?,
            )?,
            FormId::ModuleRangeTest => to_formdata(
                state
                    .device_module_config
                    .range_test
                    .as_ref()
                    .ok_or(anyhow::anyhow!("Range Test config not loaded"))?,
            )?,
            _ => return Err(anyhow::anyhow!("Loader not implemented for FormId: {}", id)),
        };

        Ok(data)
    }

    fn save_config(&self, id: &FormId) -> anyhow::Result<()> {
        let state = &self.state_rx.borrow();
        let form_data = state.settings_form_data.as_ref().expect("should be Some");

        match id {
            FormId::RadioLora => {
                self.meshtastic_command_tx.send(CommandToMeshtastic::SaveConfig {
                    my_node_id: state.my_node_key.expect("should be Some"),
                    config: config::PayloadVariant::Lora(from_formdata::<LoRaConfig>(&form_data)?),
                })?;
            }
            FormId::DeviceDevice => {
                self.meshtastic_command_tx.send(CommandToMeshtastic::SaveConfig {
                    my_node_id: state.my_node_key.expect("should be Some"),
                    config: config::PayloadVariant::Device(from_formdata::<DeviceConfig>(&form_data)?),
                })?;
            }
            FormId::DeviceUser => {
                self.meshtastic_command_tx.send(CommandToMeshtastic::SaveUser {
                    my_node_id: state.my_node_key.expect("should be Some"),
                    user: from_formdata::<User>(&form_data)?,
                })?;
            }
            FormId::DevicePosition => {
                self.meshtastic_command_tx.send(CommandToMeshtastic::SaveConfig {
                    my_node_id: state.my_node_key.expect("should be Some"),
                    config: config::PayloadVariant::Position(from_formdata::<PositionConfig>(&form_data)?),
                })?;
            }
            FormId::DevicePower => {
                self.meshtastic_command_tx.send(CommandToMeshtastic::SaveConfig {
                    my_node_id: state.my_node_key.expect("should be Some"),
                    config: config::PayloadVariant::Power(from_formdata::<PowerConfig>(&form_data)?),
                })?;
            }
            FormId::DeviceDisplay => {
                self.meshtastic_command_tx.send(CommandToMeshtastic::SaveConfig {
                    my_node_id: state.my_node_key.expect("should be Some"),
                    config: config::PayloadVariant::Display(from_formdata::<DisplayConfig>(&form_data)?),
                })?;
            }
            FormId::DeviceBluetooth => {
                self.meshtastic_command_tx.send(CommandToMeshtastic::SaveConfig {
                    my_node_id: state.my_node_key.expect("should be Some"),
                    config: config::PayloadVariant::Bluetooth(from_formdata::<BluetoothConfig>(&form_data)?),
                })?;
            }
            FormId::ModuleMqtt => {
                self.meshtastic_command_tx.send(CommandToMeshtastic::SaveModuleConfig {
                    my_node_id: state.my_node_key.expect("should be Some"),
                    config: module_config::PayloadVariant::Mqtt(from_formdata::<MqttConfig>(&form_data)?),
                })?;
            }
            FormId::ModuleSerial => {
                self.meshtastic_command_tx.send(CommandToMeshtastic::SaveModuleConfig {
                    my_node_id: state.my_node_key.expect("should be Some"),
                    config: module_config::PayloadVariant::Serial(from_formdata::<SerialConfig>(&form_data)?),
                })?;
            }
            FormId::ModuleExternalNotification => {
                self.meshtastic_command_tx.send(CommandToMeshtastic::SaveModuleConfig {
                    my_node_id: state.my_node_key.expect("should be Some"),
                    config: module_config::PayloadVariant::ExternalNotification(from_formdata::<
                        ExternalNotificationConfig,
                    >(&form_data)?),
                })?;
            }
            FormId::ModuleStoreAndForward => {
                self.meshtastic_command_tx.send(CommandToMeshtastic::SaveModuleConfig {
                    my_node_id: state.my_node_key.expect("should be Some"),
                    config: module_config::PayloadVariant::StoreForward(from_formdata::<StoreForwardConfig>(
                        &form_data,
                    )?),
                })?;
            }
            FormId::ModuleRangeTest => {
                self.meshtastic_command_tx.send(CommandToMeshtastic::SaveModuleConfig {
                    my_node_id: state.my_node_key.expect("should be Some"),
                    config: module_config::PayloadVariant::RangeTest(from_formdata::<RangeTestConfig>(&form_data)?),
                })?;
            }
            _ => unimplemented!(),
        };

        Ok(())
    }
}

fn build_settings() -> Vec<SettingsItem> {
    Vec::from([
        // Radio
        SettingsItem::group("Radio"),
        SettingsItem::form("LoRa", FormId::RadioLora),
        SettingsItem::form("Channels", FormId::RadioChannels),
        SettingsItem::form("Security", FormId::RadioSecurity),
        // Device
        SettingsItem::group("Device"),
        SettingsItem::form("User", FormId::DeviceUser),
        SettingsItem::form("Device", FormId::DeviceDevice),
        SettingsItem::form("Position", FormId::DevicePosition),
        SettingsItem::form("Power", FormId::DevicePower),
        SettingsItem::form("Display", FormId::DeviceDisplay),
        SettingsItem::form("Bluetooth", FormId::DeviceBluetooth),
        // Module
        SettingsItem::group("Module"),
        SettingsItem::form("MQTT", FormId::ModuleMqtt),
        SettingsItem::form("Serial", FormId::ModuleSerial),
        SettingsItem::form("External Notification", FormId::ModuleExternalNotification),
        SettingsItem::form("Store & Forward", FormId::ModuleStoreAndForward),
        SettingsItem::form("Range Test", FormId::ModuleRangeTest),
        SettingsItem::form("Telemetry", FormId::ModuleTelemetry),
        SettingsItem::form("Canned Message", FormId::ModuleCannedMessage),
        SettingsItem::form("Neighbor Info", FormId::ModuleNeighborInfo),
        // App
        SettingsItem::group("App"),
        SettingsItem::form("UI", FormId::AppUi),
    ])
}
