use thiserror::Error;

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("discover pipes: {0}")]
    Discovery(String),

    #[error("pipe I/O: {0}")]
    PipeIo(#[from] std::io::Error),

    /// The bridge connection is down (read loop exited / writer reclaimed /
    /// reconnect failed). The caller may safely retry — a retry can trigger
    /// an automatic reconnect.
    #[error("bridge connection is down: {0}")]
    Connection(String),

    #[error("protocol: {0}")]
    Protocol(String),

    #[error("rpc error in {method}: {message}")]
    Rpc { method: String, message: String },

    #[error("cdp error in {method}: ({code}) {message}")]
    Cdp {
        method: String,
        code: i64,
        message: String,
    },

    #[error("timeout waiting for {0} response")]
    Timeout(String),

    #[error("{0}")]
    User(String),
}

pub type Result<T> = std::result::Result<T, BridgeError>;
