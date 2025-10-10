use crate::space_watcher::{SpaceEvent, SpaceEventType};
use entity::repository::{Entity as Repository, Model as RepositoryModel};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};
use errors::AppError;
use log::info;
use serde_json;
use chrono::{DateTime, Utc};

#[derive(Clone)]
pub struct RepositoryService {
    db: DatabaseConnection,
}

impl RepositoryService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn store_event(
        &self,
        space_key: &str,
        event: &SpaceEvent,
    ) -> Result<(), AppError> {
        let event_type = match &event.event_type {
            SpaceEventType::Created => "created",
            SpaceEventType::Modified => "modified",
            SpaceEventType::Deleted => "deleted",
            SpaceEventType::Moved { .. } => "moved",
        };

        let metadata = serde_json::to_string(&event)
            .map_err(|e| AppError::Config(format!("Failed to serialize event metadata: {}", e)))?;

        let repository_event = entity::repository::ActiveModel {
            space_key: Set(space_key.to_string()),
            file_path: Set(event.path.to_string_lossy().to_string()),
            event_type: Set(event_type.to_string()),
            timestamp: Set(DateTime::<Utc>::from(event.timestamp).into()),
            file_size: Set(event.size.map(|s| s as i64)),
            file_hash: Set(event.hash.clone()),
            metadata: Set(Some(metadata)),
            ..Default::default()
        };

        repository_event
            .insert(&self.db)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))?;

        info!(
            "Stored event: {} for file {} in space {}",
            event_type,
            event.path.display(),
            space_key
        );

        Ok(())
    }

    pub async fn get_events_for_space(
        &self,
        space_key: &str,
    ) -> Result<Vec<RepositoryModel>, AppError> {
        Repository::find()
            .filter(entity::repository::Column::SpaceKey.eq(space_key))
            .order_by_desc(entity::repository::Column::Timestamp)
            .all(&self.db)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))
    }

    pub async fn get_recent_events(&self, limit: u64) -> Result<Vec<RepositoryModel>, AppError> {
        Repository::find()
            .order_by_desc(entity::repository::Column::Timestamp)
            .limit(limit)
            .all(&self.db)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))
    }
}
