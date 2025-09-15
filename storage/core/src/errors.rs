use crate::types::QueryTarget;
use errors::AppError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Store not found for target: {target:?}")]
    StoreNotFound { target: QueryTarget },

    #[error("Database query failed: {0}")]
    QueryFailed(String),

    #[error("Database connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Database Error: {0}")]
    Db(#[from] sea_orm::DbErr),
}

impl From<StorageError> for AppError {
    fn from(e: StorageError) -> Self {
        AppError::Storage(Box::new(e))
    }
}
