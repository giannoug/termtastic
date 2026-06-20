mod log2state;
mod meshtastic;
mod repository;
mod serde;
mod service;
mod state;
mod types;
mod ui;

use etcetera::BaseStrategy;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle, Toplevel};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use tracing_unwrap::ResultExt;

use crate::service::PersistenceService;
use crate::{
    log2state::LogToState,
    meshtastic::MeshtasticService,
    service::{ChatService, ConfigService, ConnectionService, NodesService, SettingsService, UiService},
    state::{State, Store},
    types::AppEvent,
    ui::Ui,
};

pub const APP_NAME: &str = env!("CARGO_PKG_NAME");
pub const APP_VERSION: &str = env!("APP_VERSION");

#[tokio::main]
async fn main() {
    let xdg = etcetera::choose_base_strategy().expect_or_log("xdg config build failed");
    let data_dir = xdg.data_dir().join(APP_NAME);
    let config_dir = xdg.config_dir().join(APP_NAME);

    let (store, state_action_tx, state_rx, state_changed_rx) = Store::new(State::default());

    let (file_writer, _file_writer_guard) = tracing_appender::non_blocking(tracing_appender::rolling::daily(
        data_dir.join("logs"),
        format!("{}.log", APP_NAME),
    ));

    let file_logger_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false);

    let log_to_state_layer = LogToState::new(state_action_tx.clone());

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,meshtastic=off")))
        .with(file_logger_layer)
        .with(log_to_state_layer)
        .init();

    tracing::info!("application started");
    tracing::info!("data dir: {}", data_dir.display());
    tracing::info!("config dir: {}", config_dir.display());

    let (app_event_tx, app_event_rx) = broadcast::channel::<AppEvent>(1024);

    let (persistence_service, persisted_state_action_tx) = PersistenceService::new(
        app_event_tx.clone(),
        app_event_rx.resubscribe(),
        state_action_tx.clone(),
        data_dir,
    );

    let (meshtastic_service, meshtastic_command_tx, meshtastic_event_rx) = MeshtasticService::new();

    let config_service = ConfigService::new(
        app_event_tx.clone(),
        app_event_rx.resubscribe(),
        state_rx.clone(),
        persisted_state_action_tx.clone(),
        state_changed_rx.resubscribe(),
        config_dir,
    );

    let ui_service = UiService::new(app_event_rx.resubscribe(), persisted_state_action_tx.clone());

    let nodes_service = NodesService::new(
        app_event_tx.clone(),
        app_event_rx.resubscribe(),
        state_rx.clone(),
        persisted_state_action_tx.clone(),
        meshtastic_command_tx.clone(),
        meshtastic_event_rx.resubscribe(),
    );

    let connection_service = ConnectionService::new(
        app_event_tx.clone(),
        app_event_rx.resubscribe(),
        state_rx.clone(),
        persisted_state_action_tx.clone(),
        meshtastic_command_tx.clone(),
        meshtastic_event_rx.resubscribe(),
    );

    let chat_service = ChatService::new(
        app_event_rx.resubscribe(),
        state_rx.clone(),
        persisted_state_action_tx.clone(),
        meshtastic_command_tx.clone(),
        meshtastic_event_rx.resubscribe(),
    );

    let settings_service = SettingsService::new(
        app_event_rx.resubscribe(),
        state_rx.clone(),
        persisted_state_action_tx.clone(),
        state_changed_rx.resubscribe(),
        meshtastic_command_tx.clone(),
        meshtastic_event_rx.resubscribe(),
    );

    app_event_tx
        .send(AppEvent::InitializationRequested)
        .expect_or_log("InitializationRequested event should be sent");

    Toplevel::new(async |s: &mut SubsystemHandle| {
        s.start(SubsystemBuilder::new(
            "PersistenceService",
            async |subsys: &mut SubsystemHandle| persistence_service.run(subsys).await,
        ));

        s.start(SubsystemBuilder::new(
            "ConfigService",
            async |subsys: &mut SubsystemHandle| config_service.run(subsys).await,
        ));

        s.start(SubsystemBuilder::new("Store", async |subsys: &mut SubsystemHandle| {
            store.run(subsys).await
        }));

        s.start(SubsystemBuilder::new(
            "UiService",
            async |subsys: &mut SubsystemHandle| ui_service.run(subsys).await,
        ));

        s.start(SubsystemBuilder::new(
            "NodesService",
            async |subsys: &mut SubsystemHandle| nodes_service.run(subsys).await,
        ));

        s.start(SubsystemBuilder::new(
            "ConnectionService",
            async |subsys: &mut SubsystemHandle| connection_service.run(subsys).await,
        ));

        s.start(SubsystemBuilder::new(
            "SettingsService",
            async |subsys: &mut SubsystemHandle| settings_service.run(subsys).await,
        ));

        s.start(SubsystemBuilder::new(
            "ChatService",
            async |subsys: &mut SubsystemHandle| chat_service.run(subsys).await,
        ));

        s.start(SubsystemBuilder::new(
            "MeshtasticService",
            async |subsys: &mut SubsystemHandle| meshtastic_service.run(subsys).await,
        ));

        s.start(SubsystemBuilder::new(
            "UI",
            async move |subsys: &mut SubsystemHandle| Ui::new(app_event_tx, state_rx).run(subsys).await,
        ));
    })
    .catch_signals()
    .handle_shutdown_requests(Duration::from_secs(5))
    .await
    .expect_or_log("application stopped unexpectedly");

    tracing::info!("application stopped gracefully");
}
