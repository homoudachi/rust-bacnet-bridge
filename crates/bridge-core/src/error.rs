use thiserror::Error;

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML deserialize error: {0}")]
    TomlDeserialize(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("Hub error: {0}")]
    Hub(String),

    #[error("Invalid state transition from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("State sync channel closed")]
    StateSync,

    #[error("Router error: {0}")]
    Router(String),

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Config validation error: {0}")]
    ConfigValidation(String),
}
