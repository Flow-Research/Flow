use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use entity::space_index_status;
use tracing::info;

/// Metrics for tracking pipeline progress
#[derive(Clone, Default)]
pub struct PipelineMetrics {
    pub files_loaded: Arc<AtomicUsize>,
    pub files_skipped: Arc<AtomicUsize>,
    pub files_failed: Arc<AtomicUsize>,
    pub chunks_created: Arc<AtomicUsize>,
    pub chunks_stored: Arc<AtomicUsize>,
}

impl PipelineMetrics {
    fn new() -> Self {
        Self {
            files_loaded: Arc::new(AtomicUsize::new(0)),
            files_skipped: Arc::new(AtomicUsize::new(0)),
            files_failed: Arc::new(AtomicUsize::new(0)),
            chunks_created: Arc::new(AtomicUsize::new(0)),
            chunks_stored: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn from_model(index_status: &space_index_status::Model) -> Self {
        Self {
            files_loaded: Arc::new(AtomicUsize::new(
                index_status.files_indexed.unwrap_or(0) as usize
            )),
            files_skipped: Arc::new(AtomicUsize::new(0)),
            files_failed: Arc::new(AtomicUsize::new(0)),
            chunks_created: Arc::new(AtomicUsize::new(0)),
            chunks_stored: Arc::new(AtomicUsize::new(
                index_status.chunks_stored.unwrap_or(0) as usize
            )),
        }
    }

    fn report(&self) {
        info!(
            "Pipeline Metrics: {} files loaded, {} skipped, {} failed, {} chunks created, {} chunks stored",
            self.files_loaded.load(Ordering::Relaxed),
            self.files_skipped.load(Ordering::Relaxed),
            self.files_failed.load(Ordering::Relaxed),
            self.chunks_created.load(Ordering::Relaxed),
            self.chunks_stored.load(Ordering::Relaxed),
        );
    }
}
