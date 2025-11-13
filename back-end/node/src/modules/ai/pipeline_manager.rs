use std::{
    collections::HashMap,
    sync::{Arc, atomic::Ordering},
};

use super::metrics::PipelineMetrics;
use anyhow::Result;
use entity::{prelude::SpaceIndexStatus, space, space_index_status};
use errors::AppError;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use tokio::sync::RwLock;

use super::config::IndexingConfig;
use tracing::{error, info, warn};

use crate::modules::ai::pipeline::Pipeline;

#[derive(Clone)]
pub struct PipelineManager {
    pipelines: Arc<RwLock<HashMap<String, Pipeline>>>,
    db: DatabaseConnection,
    config: IndexingConfig,
}

impl PipelineManager {
    pub fn new(db: DatabaseConnection, config: IndexingConfig) -> Self {
        Self {
            pipelines: Arc::new(RwLock::new(HashMap::new())),
            db,
            config,
        }
    }

    /// Initialize all spaces from database on startup
    pub async fn initialize_from_database(&self) -> std::result::Result<(), AppError> {
        info!("Loading all spaces from database...");

        let spaces = space::Entity::find()
            .all(&self.db)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))?;

        info!("Found {} spaces to initialize", spaces.len());

        for space_model in spaces {
            let index_status = SpaceIndexStatus::find()
                .filter(space_index_status::Column::SpaceId.eq(space_model.id))
                .one(&self.db)
                .await
                .map_err(|e| AppError::Storage(Box::new(e)))?;

            self.initialize_space_pipeline(
                space_model.id,
                &space_model.key,
                &space_model.location,
                index_status,
            )
            .await?;

            let manager = self.clone();
            let space_key = space_model.key.clone();
            tokio::spawn(async move {
                if let Err(e) = manager.index_space(&space_key).await {
                    error!("Failed to auto-index space {}: {}", space_key, e);
                }
            });
        }

        Ok(())
    }

    /// Initialize pipeline for a specific space
    pub async fn initialize_space_pipeline(
        &self,
        space_id: i32,
        space_key: &str,
        location: &str,
        index_status: Option<space_index_status::Model>,
    ) -> Result<(), AppError> {
        info!(
            "Initializing pipeline for space: {} at {}",
            space_key, location
        );

        let index_status = if let Some(status) = index_status {
            status
        } else {
            info!(
                "No index status found for space {}, creating initial record",
                space_key
            );

            let new_status = space_index_status::ActiveModel {
                space_id: Set(space_id),
                last_indexed: Set(None),
                indexing_in_progress: Set(Some(false)),
                files_indexed: Set(Some(0)),
                chunks_stored: Set(Some(0)),
                ..Default::default()
            };

            new_status
                .insert(&self.db)
                .await
                .map_err(|e| AppError::Storage(Box::new(e)))?
        };

        let space_pipeline = Pipeline {
            space_id,
            space_key: space_key.to_string(),
            location: location.to_string(),
            last_indexed: index_status
                .last_indexed
                .map(|dt| dt.with_timezone(&chrono::Utc)),
            indexing_in_progress: false,
            config: self.config.clone(),
            metrics: PipelineMetrics::from_model(&index_status),
        };

        self.pipelines
            .write()
            .await
            .insert(space_key.to_string(), space_pipeline);

        info!("Pipeline initialized for space: {}", space_key);
        Ok(())
    }

    /// Trigger indexing for a space
    pub async fn index_space(&self, space_key: &str) -> Result<(), AppError> {
        let mut pipelines = self.pipelines.write().await;
        let pipeline = pipelines
            .get_mut(space_key)
            .ok_or_else(|| AppError::NotFound(format!("Space not found: {}", space_key)))?;

        if pipeline.indexing_in_progress {
            return Err(AppError::Conflict(
                "Indexing already in progress".to_string(),
            ));
        }

        pipeline.indexing_in_progress = true;

        drop(pipelines); // Release lock before long operation

        // Run indexing in background
        tokio::spawn({
            let manager_clone = self.clone();
            let space_key = space_key.to_string();

            async move {
                let res = manager_clone.run_indexing(&space_key).await;

                match &res {
                    Ok(_) => info!("Indexing completed for space: {}", space_key),
                    Err(e) => error!("Indexing failed for space {}: {}", space_key, e),
                }

                let mut pipelines = manager_clone.pipelines.write().await;
                if let Some(p) = pipelines.get_mut(&space_key) {
                    p.indexing_in_progress = false;
                    if res.is_ok() {
                        p.last_indexed = Some(chrono::Utc::now());
                    }
                }
            }
        });

        Ok(())
    }

    async fn run_indexing(&self, space_key: &str) -> Result<(), AppError> {
        info!("Starting indexing");

        let (pipeline_struct, indexing_pipeline) = {
            let pipelines = self.pipelines.read().await;
            let pipeline = pipelines
                .get(space_key)
                .ok_or_else(|| AppError::NotFound(format!("Space not found: {}", space_key)))?;

            let pipeline_struct = pipeline.clone();

            // Build the indexing pipeline while we have the lock
            let indexing_pipeline = pipeline.build_indexing_pipeline().await.map_err(|e| {
                AppError::Internal(format!("Failed to build indexing pipeline: {}", e))
            })?;

            (pipeline_struct, indexing_pipeline)
        };

        info!("Running indexing.");

        // Run the indexing pipeline
        indexing_pipeline
            .run()
            .await
            .map_err(|e| AppError::Internal(format!("Indexing failed: {}", e)))?;

        // VALIDATION: Check failure threshold
        let metrics = &pipeline_struct.metrics;

        if let Err(msg) = metrics.validate_threshold(&pipeline_struct.config) {
            error!("Indexing failed for space '{}': {}", space_key, msg);

            // Update DB with failed status
            self.update_index_status(space_key, metrics, Some(&msg))
                .await?;

            return Err(AppError::Internal(format!(
                "Indexing validation failed: {}",
                msg
            )));
        } else {
            // Success
            info!("Indexing completed successfully for space '{}'", space_key);
            self.update_index_status(space_key, metrics, None).await?;

            Ok(())
        }
    }

    pub async fn query_space(&self, space_key: &str, query: &str) -> Result<String, AppError> {
        // Verify the space exists
        let query_pipeline = {
            let pipelines = self.pipelines.read().await;
            let pipeline = pipelines
                .get(space_key)
                .ok_or_else(|| AppError::NotFound(format!("Space not found: {}", space_key)))?;

            // Build the query pipeline while we have the lock
            pipeline
                .build_query_pipeline()
                .await
                .map_err(|e| AppError::Internal(format!("Failed to build query pipeline: {}", e)))?
        }; // Lock released here

        // Execute the query
        let answer = query_pipeline
            .query(query)
            .await
            .map_err(|e| AppError::Internal(format!("Query failed: {}", e)))?;

        Ok(answer.answer().to_string())
    }

    async fn update_index_status(
        &self,
        space_key: &str,
        metrics: &PipelineMetrics,
        error: Option<&str>,
    ) -> Result<(), AppError> {
        use entity::space_index_status;
        use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

        // Find the space by key
        let space = space::Entity::find()
            .filter(space::Column::Key.eq(space_key))
            .one(&self.db)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))?
            .ok_or_else(|| AppError::NotFound(format!("Space not found: {}", space_key)))?;

        // Find the existing index status
        let status = space_index_status::Entity::find()
            .filter(space_index_status::Column::SpaceId.eq(space.id))
            .one(&self.db)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))?;

        if let Some(status) = status {
            let mut active_status: space_index_status::ActiveModel = status.into();

            active_status.indexing_in_progress = Set(Some(false));
            active_status.files_indexed =
                Set(Some(metrics.files_loaded.load(Ordering::Relaxed) as i32));
            active_status.chunks_stored =
                Set(Some(metrics.chunks_stored.load(Ordering::Relaxed) as i32));

            active_status.files_failed =
                Set(Some(metrics.files_failed.load(Ordering::Relaxed) as i32));

            if let Some(error_msg) = error {
                active_status.last_error = Set(Some(error_msg.to_string()));

                warn!(
                    space_key = %space_key,
                    files_loaded = metrics.files_loaded.load(Ordering::Relaxed),
                    files_failed = metrics.files_failed.load(Ordering::Relaxed),
                    failure_rate = %format!("{:.1}%", metrics.failure_rate() * 100.0),
                    error = %error_msg,
                    "Updated index status with failure"
                );
            } else {
                active_status.last_indexed = Set(Some(chrono::Utc::now().fixed_offset()));
                active_status.last_error = Set(None);

                info!(
                    space_key = %space_key,
                    files_indexed = metrics.files_loaded.load(Ordering::Relaxed),
                    chunks_stored = metrics.chunks_stored.load(Ordering::Relaxed),
                    files_skipped = metrics.files_skipped.load(Ordering::Relaxed),
                    "Updated index status with success"
                );
            }

            // Persist to database
            active_status
                .update(&self.db)
                .await
                .map_err(|e| AppError::Storage(Box::new(e)))?;
        } else {
            // No existing status record - create one
            warn!(
                space_key = %space_key,
                space_id = space.id,
                "No index status found, creating new record"
            );

            let new_status = space_index_status::ActiveModel {
                space_id: Set(space.id),
                last_indexed: Set(if error.is_none() {
                    Some(chrono::Utc::now().fixed_offset())
                } else {
                    None
                }),
                indexing_in_progress: Set(Some(false)),
                files_indexed: Set(Some(metrics.files_loaded.load(Ordering::Relaxed) as i32)),
                chunks_stored: Set(Some(metrics.chunks_stored.load(Ordering::Relaxed) as i32)),
                files_failed: Set(Some(metrics.files_failed.load(Ordering::Relaxed) as i32)),
                last_error: Set(error.map(|e| e.to_string())),
                ..Default::default()
            };

            new_status
                .insert(&self.db)
                .await
                .map_err(|e| AppError::Storage(Box::new(e)))?;
        }

        Ok(())
    }
}
