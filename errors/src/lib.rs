use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Configuration Error: {0}")]
    Config(String),

    #[error("Initialization failed: {0}")]
    Bootstrap(String),

    #[error("Storage Error: {0}")]
    Storage(Box<dyn std::error::Error + Send + Sync>),

    #[error("I/O Error: {0}")]
    IO(#[from] std::io::Error),

    #[error("Cryptography Error: {0}")]
    Crypto(String),

    #[error("Migration failed: {0}")]
    Migration(Box<dyn std::error::Error + Send + Sync>),
}
