use chrono::{DateTime, Utc};
use errors::AppError;
use event::source::{EventListenerManager, EventSource};
use event::types::{Event, EventType};
use log::{error, info, warn};
use notify::{EventKind, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    error::Error,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

pub static FILE_CREATED: Lazy<EventType> = Lazy::new(|| EventType::new("flow.fs.file.created"));
pub static FILE_MODIFIED: Lazy<EventType> = Lazy::new(|| EventType::new("flow.fs.file.modified"));
pub static FILE_DELETED: Lazy<EventType> = Lazy::new(|| EventType::new("flow.fs.file.deleted"));
pub static FILE_RENAMED: Lazy<EventType> = Lazy::new(|| EventType::new("flow.fs.file.renamed"));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub size: u64,
    pub modified_time: DateTime<Utc>,
    pub is_directory: bool,
    pub permissions: Option<u32>,
}

pub struct FileSystemWatcher {
    space_key: String,
    watch_dir: PathBuf,
    event_manager: EventListenerManager,
    _watcher_handle: Option<std::thread::JoinHandle<()>>,
}

impl FileSystemWatcher {
    pub fn new(space_key: String, watch_dir: PathBuf) -> Result<Self, AppError> {
        Ok(Self {
            space_key,
            watch_dir,
            event_manager: EventListenerManager::new(),
            _watcher_handle: None,
        })
    }

    pub fn start_watching(&mut self) -> Result<(), Box<dyn Error>> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(tx)?;

        watcher.watch(Path::new(&self.watch_dir), RecursiveMode::Recursive)?;

        info!("Started watching directory: {:?}", self.watch_dir);

        let space_key = self.space_key.clone();
        let watch_path = self.watch_dir.clone();
        let event_manager = self.event_manager.clone(); // Clone the manager for thread sharing

        let handle = thread::spawn(move || {
            let _watcher = watcher;

            loop {
                match rx.recv() {
                    Ok(notify_event) => {
                        match Self::convert_and_publish_fs_event(
                            notify_event,
                            &space_key,
                            &watch_path,
                            &event_manager,
                        ) {
                            Ok(_) => {}
                            Err(e) => error!("Error processing file system event: {}", e),
                        }
                    }
                    Err(e) => {
                        error!("File watcher channel error: {}", e);
                        break;
                    }
                }
            }
        });

        self._watcher_handle = Some(handle);
        Ok(())
    }

    fn convert_and_publish_fs_event(
        notify_event: Result<notify::event::Event, notify::Error>,
        space_key: &str,
        base_path: &Path,
        event_manager: &EventListenerManager,
    ) -> Result<(), AppError> {
        let notify_event = match notify_event {
            Ok(event) => event,
            Err(e) => {
                warn!("File system notification error: {}", e);
                return Ok(());
            }
        };

        // Process each path in the notify event
        for path in notify_event.paths {
            let fs_event_type = Self::classify_notify_event(&notify_event.kind, &path);

            if let Some(event_type) = fs_event_type {
                let event = Self::create_flow_event(event_type, &path, base_path, space_key)?;

                // Publish the event to all listeners
                if let Err(e) = event_manager.publish(&event) {
                    error!("Failed to publish file system event: {}", e);
                } else {
                    info!(
                        "Published file system event: {} for path: {:?}",
                        event.event_type.as_str(),
                        path.strip_prefix(base_path).unwrap_or(&path)
                    );
                }
            }
        }

        Ok(())
    }

    fn classify_notify_event(kind: &EventKind, path: &Path) -> Option<EventType> {
        use notify::EventKind::*;

        match kind {
            Create(_) => {
                info!("File/directory created: {:?}", path);
                Some(FILE_CREATED.clone())
            }
            Modify(_) => {
                info!("File/directory modified: {:?}", path);
                Some(FILE_MODIFIED.clone())
            }
            Remove(_) => {
                info!("File/directory deleted: {:?}", path);
                Some(FILE_DELETED.clone())
            }
            Other => {
                // Handle other event types or ignore
                None
            }
            _ => {
                // Ignore access events and other noise
                None
            }
        }
    }

    fn create_flow_event(
        fs_event_type: EventType,
        path: &Path,
        base_path: &Path,
        space_key: &str,
    ) -> Result<event::types::Event, AppError> {
        // Get relative path from base
        let relative_path = path.strip_prefix(base_path).unwrap_or(path).to_path_buf();

        let metadata = if path.exists() {
            Self::get_file_metadata(path)?
        } else {
            None
        };

        // Create the Event
        let event = Event::new(fs_event_type, space_key.to_string())
            .with_property("absolute_path", json!(path))
            .with_property("relative_path", json!(relative_path))
            .with_property("space_key", json!(space_key))
            .with_object("metadata", &metadata);

        Ok(event)
    }

    fn get_file_metadata(path: &Path) -> Result<Option<FileMetadata>, AppError> {
        match std::fs::metadata(path) {
            Ok(metadata) => {
                let modified_time = metadata
                    .modified()
                    .map_err(|e| AppError::IO(e))?
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| AppError::Config(format!("Invalid file time: {}", e)))?;

                Ok(Some(FileMetadata {
                    size: metadata.len(),
                    modified_time: DateTime::from_timestamp(
                        modified_time.as_secs() as i64,
                        modified_time.subsec_nanos(),
                    )
                    .unwrap_or_else(|| Utc::now()),
                    is_directory: metadata.is_dir(),
                    permissions: Self::get_permissions(&metadata),
                }))
            }
            Err(_) => Ok(None), // File might have been deleted
        }
    }

    #[cfg(unix)]
    fn get_permissions(metadata: &std::fs::Metadata) -> Option<u32> {
        use std::os::unix::fs::PermissionsExt;
        Some(metadata.permissions().mode())
    }

    #[cfg(not(unix))]
    fn get_permissions(_metadata: &std::fs::Metadata) -> Option<u32> {
        None // Windows permissions are more complex, skip for now
    }

    pub fn stop_watching(&mut self) {
        if let Some(_handle) = self._watcher_handle.take() {
            info!("Stopping file system watcher for space: {}", self.space_key);
        }
    }

    pub fn is_watching(&self) -> bool {
        self._watcher_handle.is_some()
    }

    pub fn get_space_key(&self) -> &str {
        &self.space_key
    }

    pub fn get_watch_path(&self) -> &Path {
        &self.watch_dir
    }
}

impl Drop for FileSystemWatcher {
    fn drop(&mut self) {
        self.stop_watching();
    }
}

impl EventSource for FileSystemWatcher {
    fn event_manager(&self) -> &event::source::EventListenerManager {
        &self.event_manager
    }

    fn event_manager_mut(&mut self) -> &mut event::source::EventListenerManager {
        &mut self.event_manager
    }
}
