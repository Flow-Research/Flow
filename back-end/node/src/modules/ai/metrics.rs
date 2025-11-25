use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use entity::space_index_status;
use tracing::{error, info, warn};

use super::config::IndexingConfig;

/// Metrics for tracking pipeline progress
#[derive(Clone, Default)]
pub struct PipelineMetrics {
    pub files_loaded: Arc<AtomicUsize>,
    pub files_skipped: Arc<AtomicUsize>,
    pub files_failed: Arc<AtomicUsize>,
    pub chunks_created: Arc<AtomicUsize>,
    pub chunks_stored: Arc<AtomicUsize>,
    pub critical_flag_set: Arc<AtomicBool>,
    pub last_check_at: Arc<AtomicUsize>,
}

impl PipelineMetrics {
    pub fn new() -> Self {
        Self {
            files_loaded: Arc::new(AtomicUsize::new(0)),
            files_skipped: Arc::new(AtomicUsize::new(0)),
            files_failed: Arc::new(AtomicUsize::new(0)),
            chunks_created: Arc::new(AtomicUsize::new(0)),
            chunks_stored: Arc::new(AtomicUsize::new(0)),
            critical_flag_set: Arc::new(AtomicBool::new(false)),
            last_check_at: Arc::new(AtomicUsize::new(0)),
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
            critical_flag_set: Arc::new(AtomicBool::new(false)),
            last_check_at: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn report(&self) {
        info!(
            "Pipeline Metrics: {} files loaded, {} skipped, {} failed, {} chunks created, {} chunks stored",
            self.files_loaded.load(Ordering::Relaxed),
            self.files_skipped.load(Ordering::Relaxed),
            self.files_failed.load(Ordering::Relaxed),
            self.chunks_created.load(Ordering::Relaxed),
            self.chunks_stored.load(Ordering::Relaxed),
        );
    }

    pub fn failure_rate(&self) -> f64 {
        let loaded = self.files_loaded.load(Ordering::Relaxed);
        let failed = self.files_failed.load(Ordering::Relaxed);
        let total = loaded + failed;

        if total == 0 {
            0.0
        } else {
            failed as f64 / total as f64
        }
    }

    pub fn total_processed(&self) -> usize {
        self.files_loaded.load(Ordering::Relaxed) + self.files_failed.load(Ordering::Relaxed)
    }

    pub fn validate_threshold(&self, config: &IndexingConfig) -> Result<(), String> {
        let total = self.total_processed();

        // Not enough samples yet
        if total < config.min_sample_size {
            return Ok(());
        }

        let rate = self.failure_rate();
        let failed = self.files_failed.load(Ordering::Relaxed);

        if rate > config.max_failure_rate {
            return Err(format!(
                "Failure rate {:.1}% ({}/{} files) exceeds threshold {:.1}%",
                rate * 100.0,
                failed,
                total,
                config.max_failure_rate * 100.0
            ));
        }

        // Also fail on zero success
        if self.files_loaded.load(Ordering::Relaxed) == 0 && failed > 0 {
            return Err(format!(
                "Zero files successfully indexed ({} failed)",
                failed
            ));
        }

        if self.is_critical() {
            let rate = self.failure_rate();
            let total = self.total_processed();
            let failed = self.files_failed.load(Ordering::Relaxed);

            return Err(format!(
                "Critical failure threshold exceeded during execution: {:.1}% ({}/{} files)",
                rate * 100.0,
                failed,
                total
            ));
        }

        Ok(())
    }

    pub fn check_realtime_threshold(
        &self,
        config: &IndexingConfig,
        check_interval: usize,
    ) -> ThresholdStatus {
        let total = self.total_processed();
        let last_check = self.last_check_at.load(Ordering::Relaxed);

        // Only check periodically
        if total - last_check < check_interval {
            return ThresholdStatus::Ok;
        }

        self.last_check_at.store(total, Ordering::Relaxed);

        // Need minimum samples
        if total < config.min_sample_size {
            return ThresholdStatus::InsufficientData;
        }

        let rate = self.failure_rate();

        // Three-tier system
        if rate > config.max_failure_rate * 1.5 {
            // 75%+ of threshold = Catastrophic
            self.critical_flag_set.store(true, Ordering::Relaxed);
            error!(
                "🚨 CATASTROPHIC: {:.1}% failure rate at {} files",
                rate * 100.0,
                total
            );
            ThresholdStatus::Catastrophic
        } else if rate > config.max_failure_rate {
            // Above threshold = Critical
            self.critical_flag_set.store(true, Ordering::Relaxed);
            error!(
                "🔴 CRITICAL: {:.1}% failure rate at {} files (threshold: {:.1}%)",
                rate * 100.0,
                total,
                config.max_failure_rate * 100.0
            );
            ThresholdStatus::Critical
        } else if rate > config.max_failure_rate * 0.5 {
            // 50% of threshold = Warning
            warn!(
                "🟡 WARNING: {:.1}% failure rate at {} files",
                rate * 100.0,
                total
            );
            ThresholdStatus::Warning
        } else {
            ThresholdStatus::Ok
        }
    }

    pub fn is_critical(&self) -> bool {
        self.critical_flag_set.load(Ordering::Relaxed)
    }
}

pub enum ThresholdStatus {
    Ok,
    InsufficientData,
    Warning,
    Critical,
    Catastrophic,
}
