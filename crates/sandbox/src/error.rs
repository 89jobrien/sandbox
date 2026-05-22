use thiserror::Error;

use crate::capabilities::Cap;

#[derive(Debug, Error)]
pub enum ShellError {
    #[error("command not found: {0}")]
    CommandNotFound(String),

    #[error("permission denied: capability {0:?} not granted")]
    CapabilityDenied(Cap),

    #[error("resource limit exceeded: {0}")]
    LimitExceeded(String),

    #[error("parse error: {0}")]
    ParseError(String),

    #[error("parser fuel exhausted after {0} tokens")]
    ParserFuelExhausted(usize),

    #[error("execution timeout after {0}s")]
    Timeout(u64),

    #[error("max AST depth exceeded: {0}")]
    MaxDepthExceeded(usize),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("variable too large: {name} ({size} bytes, max {max})")]
    VarTooLarge {
        name: String,
        size: usize,
        max: usize,
    },

    #[error("output limit exceeded: {stream} ({size} bytes, max {max})")]
    OutputLimitExceeded {
        stream: String,
        size: usize,
        max: usize,
    },

    #[error("filesystem size limit exceeded ({size} bytes, max {max})")]
    FsSizeLimitExceeded { size: usize, max: usize },

    #[error("{0}")]
    Custom(String),
}

pub type ShellResult<T> = std::result::Result<T, ShellError>;
