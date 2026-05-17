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
                | (AppState::Starting, AppState::Stopped)
                | (AppState::Running, AppState::Stopping)
                | (AppState::Stopping, AppState::Stopped)
        );
        if valid {
            self.tx.send(to).map_err(|_| BridgeError::StateSync)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        let sm = StateManager::new();
        assert_eq!(sm.current(), AppState::Stopped);

        assert!(sm.try_transition(AppState::Starting).is_ok());
        assert_eq!(sm.current(), AppState::Starting);

        assert!(sm.try_transition(AppState::Running).is_ok());
        assert_eq!(sm.current(), AppState::Running);

        assert!(sm.try_transition(AppState::Stopping).is_ok());
        assert_eq!(sm.current(), AppState::Stopping);

        assert!(sm.try_transition(AppState::Stopped).is_ok());
        assert_eq!(sm.current(), AppState::Stopped);
    }

    #[test]
    fn test_invalid_transitions() {
        let sm = StateManager::new();

        // Stopped -> anything but Starting is illegal
        assert!(sm.try_transition(AppState::Running).is_err());
        assert!(sm.try_transition(AppState::Stopping).is_err());
        assert!(sm.try_transition(AppState::Stopped).is_err());

        // Start from Stopped -> Starting
        assert!(sm.try_transition(AppState::Starting).is_ok());

        // Starting -> Stopped is now valid (startup failure rollback)
        assert!(sm.try_transition(AppState::Stopped).is_ok());

        // Restart -> Starting
        assert!(sm.try_transition(AppState::Starting).is_ok());

        // Starting -> Starting is illegal
        assert!(sm.try_transition(AppState::Starting).is_err());
        // Starting -> Stopping is illegal
        assert!(sm.try_transition(AppState::Stopping).is_err());

        // Go to Running
        assert!(sm.try_transition(AppState::Running).is_ok());

        // Running -> anything but Stopping is illegal
        assert!(sm.try_transition(AppState::Starting).is_err());
        assert!(sm.try_transition(AppState::Running).is_err());
        assert!(sm.try_transition(AppState::Stopped).is_err());

        // Go to Stopping
        assert!(sm.try_transition(AppState::Stopping).is_ok());

        // Stopping -> anything but Stopped is illegal
        assert!(sm.try_transition(AppState::Starting).is_err());
        assert!(sm.try_transition(AppState::Running).is_err());
        assert!(sm.try_transition(AppState::Stopping).is_err());
    }

    #[test]
    fn test_initial_state_is_stopped() {
        let sm = StateManager::new();
        assert_eq!(sm.current(), AppState::Stopped);
    }

    #[test]
    fn test_default_is_stopped() {
        let sm = StateManager::default();
        assert_eq!(sm.current(), AppState::Stopped);
    }

    #[test]
    fn test_subscribe_sees_transitions() {
        let sm = StateManager::new();
        let rx = sm.subscribe();

        assert_eq!(*rx.borrow(), AppState::Stopped);

        sm.try_transition(AppState::Starting).unwrap();
        assert_eq!(*rx.borrow(), AppState::Starting);

        sm.try_transition(AppState::Running).unwrap();
        assert_eq!(*rx.borrow(), AppState::Running);
    }

    #[test]
    fn test_current_returns_current_state() {
        let sm = StateManager::new();
        assert_eq!(sm.current(), AppState::Stopped);

        sm.try_transition(AppState::Starting).unwrap();
        assert_eq!(sm.current(), AppState::Starting);

        sm.try_transition(AppState::Running).unwrap();
        assert_eq!(sm.current(), AppState::Running);
    }
}
