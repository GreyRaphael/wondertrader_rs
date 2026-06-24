use thiserror::Error;

pub type Result<T> = std::result::Result<T, WtCoreError>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WtCoreError {
    #[error("symbol cannot be empty")]
    EmptySymbol,

    #[error("invalid kline interval: {0}")]
    InvalidKlineInterval(String),

    #[error("timestamp is before unix epoch")]
    TimeBeforeUnixEpoch,
}
