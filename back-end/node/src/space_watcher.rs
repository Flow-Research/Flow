use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{mpsc, Mutex}; 
use tokio::time::{sleep, Duration};
use walkdir::WalkDir;
use log::{info, warn, error};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc as StdArc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceEvent {
    pub space_key: String,
    pub event_type: SpaceEventType,
    pub path: PathBuf,
    pub timestamp: SystemTime,
    pub size: Option<u64>,
    pub hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SpaceEventType {
    Created,
    Modified,
    Deleted,
    Moved { old_path: PathBuf },
}

#[derive(Debug)]
pub struct SpaceWatcher {
    space_path: PathBuf,
    space_key: String,
    event_tx: mpsc::UnboundedSender<SpaceEvent>,
    is_watching: Arc<std::sync::atomic::AtomicBool>,
    last_scan: Arc<Mutex<HashMap<PathBuf, (SystemTime, u64)>>>,
    scan_interval: Duration,
}

impl SpaceWatcher {
    pub fn new(
        space_path: PathBuf,
        space_key: String,
        event_tx: mpsc::UnboundedSender<SpaceEvent>,
    ) -> Self {
        Self {
            space_path,
            space_key,
            event_tx,
            is_watching: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            last_scan: Arc::new(Mutex::new(HashMap::new())),
            scan_interval: Duration::from_millis(300),
        }
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.scan_interval = interval;
        self
    }

    pub async fn start_watching(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("[SpaceWatcher] Starting watcher for space: {}", self.space_key);
        self.is_watching.store(true, std::sync::atomic::Ordering::Relaxed);
        
        info!("🔍 [SpaceWatcher] Running initial scan...");
        // Initial scan of the space
        self.initial_scan().await?;
        info!("[SpaceWatcher] Initial scan completed");
        
        // Start periodic scanning (simple approach for now)
        let space_path = self.space_path.clone();
        let event_tx = self.event_tx.clone();
        let is_watching = self.is_watching.clone();
        let space_key = self.space_key.clone();
        
        let scan_interval = self.scan_interval;
        let last_scan_ref = self.last_scan.clone();
        tokio::spawn(async move {
            while is_watching.load(std::sync::atomic::Ordering::Relaxed) {
                if let Err(e) = Self::periodic_scan_locked(
                    &space_path,
                    &space_key,
                    &event_tx,
                    &last_scan_ref,
                )
                .await
                {
                    error!("Error during periodic scan: {}", e);
                }
                sleep(scan_interval).await;
            }
        });
        
        Ok(())
    }

    pub fn stop_watching(&self) {
        self.is_watching.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    async fn initial_scan(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Performing initial scan of space: {}", self.space_path.display());
        
        if !self.space_path.exists() {
            warn!("Space path does not exist: {}", self.space_path.display());
            return Ok(());
        }

        for entry in WalkDir::new(&self.space_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path().to_path_buf();
            let metadata = std::fs::metadata(&path)?;
            let size = metadata.len();
            let modified = metadata.modified()?;
            
            let event = SpaceEvent {
                space_key: self.space_key.clone(),
                event_type: SpaceEventType::Created,
                path: path.clone(),
                timestamp: modified,
                size: Some(size),
                hash: None, // TODO: Calculate hash
            };
            
            if let Err(e) = self.event_tx.send(event) {
                warn!("Failed to send initial scan event: {}", e);
            }
        }
        
        info!("Initial scan completed for space: {}", self.space_path.display());
        Ok(())
    }

    pub async fn tick_once(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Self::periodic_scan_locked(
            &self.space_path,
            &self.space_key,
            &self.event_tx,
            &self.last_scan,
        )
        .await
    }

    async fn periodic_scan(
        space_path: &Path,
        space_key: &str,
        event_tx: &mpsc::UnboundedSender<SpaceEvent>,
        last_scan: &mut std::collections::HashMap<PathBuf, (SystemTime, u64)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("[SpaceWatcher] Starting periodic scan for: {}", space_path.display());
        
        if !space_path.exists() {
            info!("[SpaceWatcher] Space path does not exist: {}", space_path.display());
            return Ok(());
        }
    
        let mut current_files = std::collections::HashMap::new();
        
        // Scan current files
        info!("[SpaceWatcher] Scanning directory: {}", space_path.display());
        let mut file_count = 0;
        for entry in WalkDir::new(space_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path().to_path_buf();
            info!("[SpaceWatcher] Found file: {}", path.display());
            if let Ok(metadata) = std::fs::metadata(&path) {
                if let Ok(modified) = metadata.modified() {
                    current_files.insert(path.clone(), (modified, metadata.len()));
                    file_count += 1;
                    info!("[SpaceWatcher] Added file to scan: {} (size: {}, modified: {:?})", 
                        path.display(), metadata.len(), modified);
                } else {
                    info!("[SpaceWatcher] Failed to get modified time for: {}", path.display());
                }
            } else {
                info!("[SpaceWatcher] Failed to get metadata for: {}", path.display());
            }
        }
    
        info!("[SpaceWatcher] Scan complete: {} files found, {} files in previous scan", 
            current_files.len(), last_scan.len());
    
        // Check for new and modified files
        let mut events_sent = 0;
        for (path, (modified, size)) in &current_files {
            match last_scan.get(path) {
                Some((last_modified, last_size)) => {
                    if modified > last_modified || size != last_size {
                        info!("[SpaceWatcher] File modified: {}", path.display());
                        let event = SpaceEvent {
                            space_key: space_key.to_string(),
                            event_type: SpaceEventType::Modified,
                            path: path.clone(),
                            timestamp: *modified,
                            size: Some(*size),
                            hash: None,
                        };
                        if let Err(e) = event_tx.send(event) {
                            info!("[SpaceWatcher] Failed to send modified event: {}", e);
                        } else {
                            events_sent += 1;
                            info!("[SpaceWatcher] Sent modified event for: {}", path.display());
                        }
                    }
                }
                None => {
                    info!("[SpaceWatcher] New file: {}", path.display());
                    let event = SpaceEvent {
                        space_key: space_key.to_string(),
                        event_type: SpaceEventType::Created,
                        path: path.clone(),
                        timestamp: *modified,
                        size: Some(*size),
                        hash: None,
                    };
                    if let Err(e) = event_tx.send(event) {
                        info!("[SpaceWatcher] Failed to send created event: {}", e);
                    } else {
                        events_sent += 1;
                        info!("[SpaceWatcher] Sent created event for: {}", path.display());
                    }
                }
            }
        }
    
        // Check for deleted files
        for (path, (last_modified, last_size)) in last_scan.iter() {
            if !current_files.contains_key(path) {
                info!("[SpaceWatcher] File deleted: {}", path.display());
                let event = SpaceEvent {
                    space_key: space_key.to_string(),
                    event_type: SpaceEventType::Deleted,
                    path: path.clone(),
                    timestamp: *last_modified,
                    size: Some(*last_size),
                    hash: None,
                };
                if let Err(e) = event_tx.send(event) {
                    info!("[SpaceWatcher] Failed to send deleted event: {}", e);
                } else {
                    events_sent += 1;
                    info!("[SpaceWatcher] Sent deleted event for: {}", path.display());
                }
            }
        }
    
        *last_scan = current_files;
        info!("[SpaceWatcher] Scan completed: {} events sent", events_sent);
        Ok(())
    }

    async fn periodic_scan_locked(
        space_path: &Path,
        space_key: &str,
        event_tx: &mpsc::UnboundedSender<SpaceEvent>,
        last_scan_ref: &Arc<Mutex<HashMap<PathBuf, (SystemTime, u64)>>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut guard = last_scan_ref
            .lock().await;
        Self::periodic_scan(space_path, space_key, event_tx, &mut *guard).await
    }
}
