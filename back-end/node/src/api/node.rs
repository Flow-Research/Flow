use crate::bootstrap::init::NodeData;
use crate::modules::space;
use crate::repository_service::RepositoryService;
use crate::knowledge_service::KnowledgeService;
use crate::space_watcher::{SpaceWatcher, SpaceEvent};
use errors::AppError;
use log::info;
use sea_orm::{DatabaseConnection, EntityTrait};
use entity::space::Entity as Space;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct Node {
    _node_data: NodeData,
    db: DatabaseConnection,
    repository_service: RepositoryService,
    knowledge_service: KnowledgeService,
    space_watchers: Arc<std::sync::Mutex<HashMap<String, SpaceWatcher>>>,
    event_tx: mpsc::UnboundedSender<SpaceEvent>,
}

impl Node {
    pub fn new(node_data: NodeData, db: DatabaseConnection) -> Self {
        let repository_service = RepositoryService::new(db.clone());
        let knowledge_service = KnowledgeService::new(db.clone());
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<SpaceEvent>();
        let space_watchers = Arc::new(std::sync::Mutex::new(HashMap::new()));
        info!("[Node] Creating new Node with event channel");
        
        // Start event processing loop
        let repository_service_clone = repository_service.clone();
        let knowledge_service_clone = knowledge_service.clone();
        tokio::spawn(async move {
            info!("[Node] Starting event processing loop");
            while let Some(event) = event_rx.recv().await {
                info!("📨 [Node] Received event: {:?} for {}", event.event_type, event.path.display());
                let space_key = &event.space_key;
                if let Err(e) = repository_service_clone.store_event(space_key, &event).await {
                    log::error!("Failed to store event: {}", e);
                }
                
                if let Err(e) = knowledge_service_clone.process_file_event(space_key, &event).await {
                    log::error!("Failed to process knowledge event: {}", e);
                }
            }
            info!("[Node] Event processing loop ended");
        });
        
        Node { 
            _node_data: node_data, 
            db,
            repository_service,
            knowledge_service,
            space_watchers,
            event_tx,
        }
    }

    pub async fn create_space(&self, dir: &str) -> Result<(), AppError> {
        info!("Setting up space in Directory: {}", dir);
        space::new_space(&self.db, dir).await?;
        
        // Start watching the space
        self.start_watching_space(dir).await?;
        
        Ok(())
    }
    
    pub async fn start_watching_space(&self, dir: &str) -> Result<(), AppError> {
        let space_path = PathBuf::from(dir);
        let space_key = space::generate_space_key(dir)?;

        info!("[Node] Starting to watch space: {} (key: {})", dir, space_key);
        
        let watcher = SpaceWatcher::new(space_path, space_key.clone(), self.event_tx.clone())
            .with_interval(std::time::Duration::from_millis(150));

        info!("[Node] Starting SpaceWatcher...");
        watcher.start_watching().await
            .map_err(|e| AppError::Config(format!("Failed to start watching space: {}", e)))?;
        
        let mut watchers = self.space_watchers.lock().unwrap();
        watchers.insert(space_key, watcher);
        
        info!("Started watching space: {}", dir);
        info!("[Node] SpaceWatcher started successfully");
        Ok(())
    }

    // Test-only helper: trigger a manual tick on a watcher
    // #[cfg(any(test, feature = "test"))]
    pub async fn debug_tick_space(&self, space_key: &str) -> Result<(), AppError> {
        if let Some(watcher) = self.space_watchers.lock().unwrap().get(space_key) {
            watcher
                .tick_once()
                .await
                .map_err(|e| AppError::Config(format!("Tick failed: {}", e)))?
        }
        Ok(())
    }

    pub async fn list_spaces(&self) -> Result<Vec<entity::space::Model>, AppError> {
        Space::find()
            .all(&self.db)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))
    }

    pub async fn load_existing_spaces(&self) -> Result<(), AppError> {
        let spaces = self.list_spaces().await?;
        info!("Loaded {} existing spaces on startup", spaces.len());
        for space in spaces {
            info!("Space: key={}, location={}", space.key, space.location);
            // Start watching each existing space
            if let Err(e) = self.start_watching_space(&space.location).await {
                log::warn!("Failed to start watching space {}: {}", space.location, e);
            }
        }
        Ok(())
    }
    
    pub async fn get_recent_events(&self, limit: u64) -> Result<Vec<entity::repository::Model>, AppError> {
        self.repository_service.get_recent_events(limit).await
    }
    
    pub async fn get_events_for_space(&self, space_key: &str) -> Result<Vec<entity::repository::Model>, AppError> {
        self.repository_service.get_events_for_space(space_key).await
    }
    
    pub async fn get_file_metadata_for_space(&self, space_key: &str) -> Result<Vec<entity::file_metadata::Model>, AppError> {
        let result = self.knowledge_service.get_file_metadata_for_space(space_key).await;
        println!("🔍 [Node] File metadata for space: {:?}", result);
        result
    }
    
    pub async fn get_knowledge_nodes_for_space(&self, space_key: &str) -> Result<Vec<entity::knowledge_node::Model>, AppError> {
        self.knowledge_service.get_knowledge_nodes_for_space(space_key).await
    }
    
    pub async fn search_knowledge(&self, query: &str, space_key: Option<&str>) -> Result<Vec<entity::knowledge_node::Model>, AppError> {
        self.knowledge_service.search_knowledge(query, space_key).await
    }
}
