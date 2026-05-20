use nameof::name_of;
use std::hash::{DefaultHasher, Hash, Hasher};
use tokio::sync::{broadcast, mpsc, watch};
use tokio_graceful_shutdown::SubsystemHandle;

use crate::state::State;
use crate::{
    state::StateAction,
    types::{AppConfig, AppEvent, Toast},
};

pub struct ConfigService {
    app_event_tx: broadcast::Sender<AppEvent>,
    app_event_rx: broadcast::Receiver<AppEvent>,
    state_rx: watch::Receiver<State>,
    state_action_tx: mpsc::UnboundedSender<StateAction>,
    state_changed_rx: broadcast::Receiver<Vec<&'static str>>,
    app_config_last_hash: u64,
}

impl ConfigService {
    pub fn new(
        app_event_tx: broadcast::Sender<AppEvent>,
        app_event_rx: broadcast::Receiver<AppEvent>,
        state_rx: watch::Receiver<State>,
        state_action_tx: mpsc::UnboundedSender<StateAction>,
        state_changed_rx: broadcast::Receiver<Vec<&'static str>>,
    ) -> Self {
        Self {
            app_event_tx,
            app_event_rx,
            state_rx,
            state_action_tx,
            state_changed_rx,
            app_config_last_hash: 0,
        }
    }

    pub async fn run(mut self, subsys: &mut SubsystemHandle) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                event = self.app_event_rx.recv() => self.handle_app_event(event)?,
                event = self.state_changed_rx.recv() => self.handle_state_change(event)?,
                _ = subsys.on_shutdown_requested() => {
                    tracing::info!("shutdown");
                    break;
                }
            }
        }

        Ok(())
    }

    fn handle_app_event(&mut self, event: Result<AppEvent, broadcast::error::RecvError>) -> anyhow::Result<()> {
        match event {
            Ok(AppEvent::InitializationRequested) => match confy::load::<AppConfig>(crate::APP_NAME, "app") {
                Ok(app_config) => {
                    self.state_action_tx.send(StateAction::AppConfigApply(app_config))?;
                }
                Err(e) => {
                    tracing::error!("can't load app config: {}", e);

                    self.state_action_tx
                        .send(StateAction::Toast(Toast::error("app config load failed")))?;
                }
            },
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("broadcast receiver lagged by {} events", n);
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_state_change(
        &mut self,
        event: Result<Vec<&'static str>, broadcast::error::RecvError>,
    ) -> anyhow::Result<()> {
        match event {
            Ok(changed) => {
                if changed.contains(&name_of!(config_loaded in State)) {
                    self.app_event_tx.send(AppEvent::ConfigLoaded)?;

                    self.state_action_tx
                        .send(StateAction::Toast(Toast::normal("config loaded")))?;
                }

                let state = self.state_rx.borrow();

                if state.config_loaded {
                    let app_config: AppConfig = (&*state).into();
                    let app_config_hash = calculate_hash(&app_config);

                    if app_config_hash != self.app_config_last_hash {
                        confy::store(crate::APP_NAME, "app", &app_config)?;
                        self.app_config_last_hash = app_config_hash;
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
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}
