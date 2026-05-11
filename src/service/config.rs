use std::hash::{DefaultHasher, Hash, Hasher};

use tokio::sync::{broadcast, mpsc, watch};
use tokio_graceful_shutdown::SubsystemHandle;

use crate::state::StateSnapshot;
use crate::{
    state::StateAction,
    types::{AppConfig, AppEvent, Toast},
};

pub struct ConfigService {
    app_event_rx: broadcast::Receiver<AppEvent>,
    state_rx: watch::Receiver<StateSnapshot>,
    state_action_tx: mpsc::UnboundedSender<StateAction>,
    app_config_last_hash: u64,
}

impl ConfigService {
    pub fn new(
        app_event_rx: broadcast::Receiver<AppEvent>,
        state_rx: watch::Receiver<StateSnapshot>,
        state_action_tx: mpsc::UnboundedSender<StateAction>,
    ) -> Self {
        Self {
            app_event_rx,
            state_rx,
            state_action_tx,
            app_config_last_hash: 0,
        }
    }

    pub async fn run(mut self, subsys: &mut SubsystemHandle) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                event = self.app_event_rx.recv() => self.handle_app_event(event).await?,
                _ = self.state_rx.changed() => self.handle_state_change()?,
                _ = subsys.on_shutdown_requested() => {
                    tracing::info!("shutdown");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_app_event(&mut self, event: Result<AppEvent, broadcast::error::RecvError>) -> anyhow::Result<()> {
        match event {
            Ok(AppEvent::InitializationRequested) => {
                let app_config: AppConfig = confy::load(crate::APP_NAME, "app")?;

                self.state_action_tx.send(StateAction::AppConfigApply(app_config))?;

                self.state_action_tx
                    .send(StateAction::Toast(Toast::normal("config loaded")))?;
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("broadcast receiver lagged by {} events", n);
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_state_change(&mut self) -> anyhow::Result<()> {
        let state_snapshot = &self.state_rx.borrow();

        let app_config: AppConfig = (&state_snapshot.state).into();
        let app_config_hash = calculate_hash(&app_config);

        if app_config_hash != self.app_config_last_hash {
            confy::store(crate::APP_NAME, "app", &app_config)?;
            self.app_config_last_hash = app_config_hash;
        }

        Ok(())
    }
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}
