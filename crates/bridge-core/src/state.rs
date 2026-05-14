use tokio::sync::watch;

use crate::error::BridgeError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Stopped,
    Starting,
    Running,
    Stopping,
}

impl std::fmt::Display for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppState::Stopped => write!(f, "Stopped"),
            AppState::Starting => write!(f, "Starting"),
            AppState::Running => write!(f, "Running"),
            AppState::Stopping => write!(f, "Stopping"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StateManager {
    tx: watch::Sender<AppState>,
    _rx: watch::Receiver<AppState>,
}

impl StateManager {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(AppState::Stopped);
        Self { tx, _rx: rx }
    }

    pub fn subscribe(&self) -> watch::Receiver<AppState> {
        self.tx.subscribe()
    }

    pub fn current(&self) -> AppState {
        *self.tx.borrow()
    }

    pub fn try_transition(&self, to: AppState) -> Result<(), BridgeError> {
        let current = *self.tx.borrow();
        let valid = matches!(
            (current, to),
            (AppState::Stopped, AppState::Starting)
                | (AppState::Starting, AppState::Running)
                | (AppState::Running, AppState::Stopping)
                | (AppState::Stopping, AppState::Stopped)
        );
        if valid {
            self.tx
                .send(to)
                .map_err(|_| BridgeError::StateSync)?;
            Ok(())
        } else {
            Err(BridgeError::InvalidStateTransition {
                from: format!("{:?}", current),
                to: format!("{:?}", to),
            })
        }
    }
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}
