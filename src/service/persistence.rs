use itertools::Itertools;
use std::path::PathBuf;
use tokio::sync::{broadcast, mpsc};
use tokio_graceful_shutdown::SubsystemHandle;

use crate::repository::{Repository, create_sqlite_repository};
use crate::types::Node;
use crate::{
    APP_NAME,
    state::StateAction,
    types::{AppEvent, Toast},
};

pub struct PersistenceService {
    app_event_rx: broadcast::Receiver<AppEvent>,
    forward_state_action_tx: mpsc::UnboundedSender<StateAction>,
    state_action_rx: mpsc::UnboundedReceiver<StateAction>,
    data_dir: PathBuf,
    repository: Option<Repository>,
}

impl PersistenceService {
    pub fn new(
        app_event_rx: broadcast::Receiver<AppEvent>,
        forward_state_action_tx: mpsc::UnboundedSender<StateAction>,
        data_dir: PathBuf,
    ) -> (Self, mpsc::UnboundedSender<StateAction>) {
        let (state_action_tx, state_action_rx) = mpsc::unbounded_channel::<StateAction>();

        (
            Self {
                app_event_rx,
                forward_state_action_tx,
                state_action_rx,
                data_dir,
                repository: None,
            },
            state_action_tx,
        )
    }

    pub async fn run(mut self, subsys: &mut SubsystemHandle) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                Some(action) = self.state_action_rx.recv() => self.handle_action(action)?,
                event = self.app_event_rx.recv() => self.handle_app_event(event)?,
                _ = subsys.on_shutdown_requested() => {
                    tracing::info!("shutdown");
                    break;
                }
            }
        }

        Ok(())
    }

    fn handle_action(&mut self, action: StateAction) -> anyhow::Result<()> {
        let Some(repository) = &self.repository else {
            self.forward_state_action_tx.send(action)?;
            return Ok(());
        };

        match &action {
            StateAction::NodeInit(node) if node.user.is_some() => {
                if let Err(e) = repository.node_upsert(node.into()) {
                    tracing::error!("node init failed: {}", e);

                    self.forward_state_action_tx
                        .send(StateAction::Toast(Toast::error("DB error: see logs")))?;
                }
            }
            StateAction::NodeUpdate(node) => {
                if let Err(e) = repository.node_upsert(node.into()) {
                    tracing::error!("node update failed: {}", e);

                    self.forward_state_action_tx
                        .send(StateAction::Toast(Toast::error("DB error: see logs")))?;
                }
            }
            StateAction::NodeDelete(node_key) => {
                if let Err(e) = repository.node_remove(*node_key) {
                    tracing::error!("node delete failed: {}", e);

                    self.forward_state_action_tx
                        .send(StateAction::Toast(Toast::error("DB error: see logs")))?;
                }
            }
            StateAction::NodeUpdateLastHeard {
                node_key,
                hops,
                snr,
                rssi,
            } => match repository.node_get_by_key(*node_key) {
                Ok(Some(mut node)) => {
                    node.hops = Some(*hops);
                    node.snr = *snr;
                    node.rssi = Some(*rssi);

                    if let Err(e) = repository.node_upsert(node) {
                        tracing::error!("node update failed: {}", e);

                        self.forward_state_action_tx
                            .send(StateAction::Toast(Toast::error("DB error: see logs")))?;
                    }
                }
                Ok(None) => {
                    tracing::debug!("node {} not found for updating last heard – skip", node_key);
                }
                Err(e) => {
                    tracing::error!("node find by key failed: {}", e);

                    self.forward_state_action_tx
                        .send(StateAction::Toast(Toast::error("DB error: see logs")))?;
                }
            },
            _ => {}
        };

        self.forward_state_action_tx.send(action)?;

        Ok(())
    }

    fn handle_app_event(&mut self, event: Result<AppEvent, broadcast::error::RecvError>) -> anyhow::Result<()> {
        match event {
            Ok(AppEvent::DbLoadRequested(node_key)) => {
                let db_file = self.data_dir.join(format!("{}_{}.sqlite3", APP_NAME, node_key));

                match create_sqlite_repository(db_file) {
                    Ok(repository) => {
                        self.repository = Some(repository);
                    }
                    Err(e) => {
                        tracing::error!("DB initialization failed: {}", e);

                        self.forward_state_action_tx
                            .send(StateAction::Toast(Toast::error("DB initialization failed")))?;
                    }
                }

                self.load_data()?;
            }
            Ok(AppEvent::DbCompactRequested) => {
                let Some(repository) = &self.repository else {
                    return Ok(());
                };

                let old_size = std::fs::metadata(repository.get_file_path())?.len();

                match repository.vacuum() {
                    Ok(_) => {
                        let new_size = std::fs::metadata(repository.get_file_path())?.len();

                        tracing::info!(
                            "DB compacted, old size: {}KB, new size: {}KB",
                            old_size / 1024,
                            new_size / 1024
                        );

                        self.forward_state_action_tx
                            .send(StateAction::Toast(Toast::success("DB compacted")))?;
                    }
                    Err(e) => {
                        tracing::error!("DB compact failed: {}", e);

                        self.forward_state_action_tx
                            .send(StateAction::Toast(Toast::error("DB compact failed")))?;
                    }
                }
            }
            Ok(AppEvent::TelemetryArrived(packet)) => {
                let Some(repository) = &self.repository else {
                    return Ok(());
                };

                match packet.data {
                    meshtastic::protobufs::telemetry::Variant::DeviceMetrics(_) => {
                        repository.telemetry_device_metrics_insert(packet.try_into()?)?;
                    }
                    _ => {
                        tracing::debug!("telemetry packet not handled: {:?}", packet.data);
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("broadcast receiver lagged by {} events", n);
            }
            _ => {}
        }

        Ok(())
    }

    fn load_data(&self) -> anyhow::Result<()> {
        let Some(repository) = &self.repository else {
            tracing::warn!("DB data not loaded");

            self.forward_state_action_tx
                .send(StateAction::Toast(Toast::warning("DB data not loaded")))?;

            return Ok(());
        };

        // nodes
        let nodes: Vec<Node> = match repository.node_find_all() {
            Ok(nodes) => {
                tracing::info!("nodes loaded from DB: {}", nodes.len());

                nodes.into_iter().map_into().collect()
            }
            Err(e) => {
                tracing::error!("nodes load failed: {}", e);

                self.forward_state_action_tx
                    .send(StateAction::Toast(Toast::error("nodes not loaded from DB")))?;

                return Ok(());
            }
        };

        self.forward_state_action_tx.send(StateAction::DbDataLoaded { nodes })?;

        // telemetry: device metrics
        match repository.telemetry_device_metrics_find_last_for_each_node() {
            Ok(map) => {
                for (node_key, metrics) in map.into_iter() {
                    self.forward_state_action_tx.send(StateAction::NodeLastTelemetrySet(
                        node_key,
                        meshtastic::protobufs::telemetry::Variant::DeviceMetrics(metrics.into()),
                    ))?;
                }
            }
            Err(e) => {
                tracing::error!("nodes telemetry load failed: {}", e);

                self.forward_state_action_tx
                    .send(StateAction::Toast(Toast::error("nodes telemetry not loaded from DB")))?;

                return Ok(());
            }
        };

        self.forward_state_action_tx
            .send(StateAction::Toast(Toast::normal("DB data loaded")))?;

        Ok(())
    }
}
