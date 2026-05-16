use itertools::Itertools;
use std::fs;
use std::path::PathBuf;
use tokio::sync::{broadcast, mpsc};
use tokio_graceful_shutdown::SubsystemHandle;

use crate::repository::{create_repository, Repository};
use crate::types::{DbInfo, Node};
use crate::{
    state::StateAction,
    types::{AppEvent, Toast},
    APP_NAME,
};

// We read the data from the DB only once during the initialization.
// The cache size is set to zero to avoid unnecessary memory usage.
const DB_CACHE_SIZE: usize = 0;

pub struct PersistenceService<'a> {
    app_event_rx: broadcast::Receiver<AppEvent>,
    forward_state_action_tx: mpsc::UnboundedSender<StateAction>,
    state_action_rx: mpsc::UnboundedReceiver<StateAction>,
    file_path: PathBuf,
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
                file_path: data_dir.join(format!("{}.db", APP_NAME)),
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
                self.load_db_info()?;
            }
            StateAction::NodeUpdate(node) => {
                repository.nodes_upsert(node.into())?;
                self.load_db_info()?;
            }
            StateAction::NodeDelete(node_key) => {
                repository.nodes_remove(*node_key)?;
                self.load_db_info()?;
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
                    self.load_db_info()?;
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
                tracing::info!("DB initializing started. Path: {}", self.file_path.display());

                self.forward_state_action_tx.send(StateAction::DbInitStart)?;

                match create_repository(&self.file_path, DB_CACHE_SIZE) {
                    Ok(mut repository) => match repository.check_integrity() {
                        Ok(res) => {
                            if res {
                                tracing::info!("DB integrity check passed");
                            } else {
                                tracing::warn!("DB integrity check passed after repair");
                            }

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

                tracing::info!("DB initializing finished");

                self.load_data()?;
                self.load_db_info()?;
            }
            Ok(AppEvent::DbCompactRequested) => {
                if let Some(repository) = self.repository.as_mut() {
                    match repository.compact() {
                        Ok(true) => {
                            tracing::info!("DB compacted");

                            self.forward_state_action_tx
                                .send(StateAction::Toast(Toast::success("DB compacted")))?;
                        }
                        Ok(false) => {
                            tracing::info!("DB already compacted");

                            self.forward_state_action_tx
                                .send(StateAction::Toast(Toast::success("DB already compacted")))?;
                        }
                        Err(e) => {
                            tracing::error!("DB compact failed: {}", e);

                            self.forward_state_action_tx
                                .send(StateAction::Toast(Toast::error("DB compact failed")))?;
                        }
                    }

                    self.load_db_info()?;
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

    fn load_db_info(&self) -> anyhow::Result<()> {
        let metadata = fs::metadata(&self.file_path)?;

        self.forward_state_action_tx.send(StateAction::DbInfoSet(DbInfo {
            file_size: metadata.len(),
        }))?;

        Ok(())
    }
}
