use itertools::Itertools;
use std::path::PathBuf;
use tokio::sync::{broadcast, mpsc};
use tokio_graceful_shutdown::SubsystemHandle;

use crate::repository::{create_repository, Repository};
use crate::types::Node;
use crate::{
    state::StateAction,
    types::{AppEvent, Toast},
};

// We read the data from the DB only once during the initialization.
// The cache size is set to zero to avoid unnecessary memory usage.
const DB_CACHE_SIZE: usize = 0;

pub struct PersistenceService<'a> {
    app_event_rx: broadcast::Receiver<AppEvent>,
    forward_state_action_tx: mpsc::UnboundedSender<StateAction>,
    state_action_rx: mpsc::UnboundedReceiver<StateAction>,
    data_dir: PathBuf,
    repository: Option<Repository<'a>>,
}

impl<'a> PersistenceService<'a> {
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
            StateAction::NodeInit(node) => {
                repository.nodes_upsert(node.into())?;
            }
            StateAction::NodeUpdate(node) => {
                repository.nodes_upsert(node.into())?;
            }
            StateAction::NodeDelete(node_key) => {
                repository.nodes_remove(*node_key)?;
            }
            StateAction::NodeUpdateLastHeard {
                node_key,
                hops,
                snr,
                rssi,
            } => {
                if let Some(mut node) = repository.nodes_find_by_key(*node_key)? {
                    node.hops = Some(*hops);
                    node.snr = *snr;
                    node.rssi = Some(*rssi);

                    repository.nodes_upsert(node)?;
                }
            }
            _ => {}
        };

        self.forward_state_action_tx.send(action)?;

        Ok(())
    }

    fn handle_app_event(&mut self, event: Result<AppEvent, broadcast::error::RecvError>) -> anyhow::Result<()> {
        match event {
            Ok(AppEvent::InitializationRequested) => {
                self.forward_state_action_tx.send(StateAction::DbInitStart)?;

                match create_repository(&self.data_dir, DB_CACHE_SIZE) {
                    Ok(mut repository) => match repository.check_integrity() {
                        Ok(res) => {
                            if res {
                                tracing::info!("DB integrity check passed");
                            } else {
                                tracing::warn!("DB integrity check passed after repair");
                            }

                            tracing::info!("DB initializing finished");

                            self.repository = Some(repository);
                            self.forward_state_action_tx.send(StateAction::DbInitSuccess)?;
                        }
                        Err(e) => {
                            tracing::error!("DB integrity check failed: {}", e);

                            self.forward_state_action_tx
                                .send(StateAction::DbInitFail(e.to_string()))?;
                        }
                    },
                    Err(e) => {
                        tracing::error!("DB initialization failed: {}", e);

                        self.forward_state_action_tx
                            .send(StateAction::DbInitFail(e.to_string()))?;

                        self.forward_state_action_tx
                            .send(StateAction::Toast(Toast::error("DB initialization failed")))?;
                    }
                }

                self.load_data()?;
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

        tracing::info!("DB data load started");

        self.forward_state_action_tx.send(StateAction::DbLoadStart)?;

        let nodes: Vec<Node> = match repository.nodes_get_all() {
            Ok(nodes) => {
                tracing::info!("nodes loaded from DB: {}", nodes.len());

                nodes.into_iter().map_into().collect()
            }
            Err(e) => {
                tracing::error!("nodes load failed: {}", e);

                self.forward_state_action_tx
                    .send(StateAction::DbLoadFail(e.to_string()))?;

                self.forward_state_action_tx
                    .send(StateAction::Toast(Toast::error("nodes not loaded from DB")))?;

                return Ok(());
            }
        };

        tracing::info!("DB data load finished");

        self.forward_state_action_tx
            .send(StateAction::DbLoadSuccess { nodes })?;

        self.forward_state_action_tx
            .send(StateAction::Toast(Toast::normal("DB data loaded")))?;

        Ok(())
    }
}
