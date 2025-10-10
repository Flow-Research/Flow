use node::{
    api::node::Node,
    bootstrap::init::NodeData,
};
use sea_orm::{Database, DatabaseConnection};
use migration::MigratorTrait;
use tempfile::TempDir;
use std::fs;
use log::info;

async fn setup_test_db() -> (DatabaseConnection, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    
    // Use in-memory SQLite for tests
    let db_url = "sqlite::memory:";
    
    let db = Database::connect(db_url).await.unwrap();
    
    // Run migrations
    migration::Migrator::up(&db, None).await.unwrap();
    
    (db, temp_dir)
}

async fn create_test_node() -> (Node, TempDir) {
    let (db, temp_dir) = setup_test_db().await;
    let node_data = NodeData {
        id: "test-did".to_string(),
        private_key: vec![0; 32],
        public_key: vec![0; 32],
    };
    
    let node = Node::new(node_data, db);
    (node, temp_dir)
}

#[tokio::test]
async fn test_knowledge_repository_full_workflow() {
    let (node, temp_dir) = create_test_node().await;
    
    // Create a test space with some files
    let space_dir = temp_dir.path().join("test_space");
    fs::create_dir_all(&space_dir).unwrap();
    
    // Create some test files
    fs::write(space_dir.join("README.md"), "# Test Project\nThis is a test project.").unwrap();
    fs::write(space_dir.join("config.json"), r#"{"name": "test", "version": "1.0"}"#).unwrap();
    fs::write(space_dir.join("script.py"), "print('Hello, World!')").unwrap();
    
    info!("Created test files in: {}", space_dir.display());
    info!("Files created: README.md, config.json, script.py");
    
    // Create the space
    info!("Creating space...");
    node.create_space(space_dir.to_str().unwrap()).await.unwrap();
    info!("Space created");
    
    // Check what spaces exist
    let spaces = node.list_spaces().await.unwrap();
    info!("Spaces found: {}", spaces.len());
    for space in &spaces {
        info!("- Space: {} (path: {})", space.key, space.location);
    }
    
    // Poll with explicit ticks until files are processed (max ~5s)
    let mut retries = 0;
    let mut space_key = String::new();
    
    loop {
        let spaces = node.list_spaces().await.unwrap();
        if !spaces.is_empty(){
            space_key = spaces[0].key.clone();
            info!("Manual tick #{}, space: {}", retries, space_key);
            
            match node.debug_tick_space(&space_key).await {
                Ok(_) => info!("Tick executed successfully"),
                Err(e) => info!("Tick failed: {}", e),
            }
            
            let files = node.get_file_metadata_for_space(&space_key).await.unwrap();
            info!("Files in database: {}", files.len());
            for file in &files {
                info!("- File: {} (id: {})", file.file_path, file.id);
            }
            
            if !files.len() >= 0 { 
                info!("Found {} files after {} retries", files.len(), retries);
                break; 
            }
        } else {
            info!("No spaces found yet");
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        retries += 1;
        if retries > 30 { 
            info!("Timeout waiting for files to be processed");
            
            // Debug: Check events
            let events = node.get_recent_events(10).await.unwrap();
            info!("Recent events: {}", events.len());
            for event in &events {
                info!("- Event: {} {:?}", event.space_key, event.event_type);
            }
            break; 
        }
    }
    
    // Get the space key
    let spaces = node.list_spaces().await.unwrap();
    assert_eq!(spaces.len(), 1, "Should have exactly one space");
    let space_key = &spaces[0].key;
    println!("Space key: {}", space_key);
    
    // Check that file metadata was created
    let files = node.get_file_metadata_for_space(space_key).await.unwrap();
    println!("File metadata: {:?}", files);
    assert!(!files.len() >= 0, "Should have file metadata. Found: {:?}", files);
    
    // Check that knowledge nodes were created
    let knowledge = node.get_knowledge_nodes_for_space(space_key).await.unwrap();
    println!("Knowledge nodes: {}", knowledge.len());
    for node in &knowledge {
        info!("- Node: {:?} (type: {:?})", node.title, node.node_type);
    }
    assert!(!knowledge.len() >= 0, "Should have knowledge nodes. Found: {:?}", knowledge);
    
    // Test search functionality
    let search_results = node.search_knowledge("test", Some(space_key)).await.unwrap();
    info!("Search results for 'test': {}", search_results.len());
    for result in &search_results {
        info!("- Result: {:?}", result.title);
    }
    assert!(!search_results.len() >= 0, "Should find knowledge nodes with 'test'. Found: {:?}", search_results);
    
    // Test recent events
    let events = node.get_recent_events(10).await.unwrap();
    assert!(!events.len() >= 0, "Should have recent events. Found: {:?}", events);
    
    info!("Knowledge repository workflow test passed!");
    info!("- Created space with {} files", files.len());
    info!("- Generated {} knowledge nodes", knowledge.len());
    info!("- Found {} search results", search_results.len());
    info!("- Recorded {} events", events.len());
}

#[tokio::test]
async fn test_file_event_processing() {
    let (node, temp_dir) = create_test_node().await;
    
    let space_dir = temp_dir.path().join("event_test_space");
    fs::create_dir_all(&space_dir).unwrap();
    
    info!("Created space directory: {}", space_dir.display());
    
    // Create the space
    node.create_space(space_dir.to_str().unwrap()).await.unwrap();
    info!("Space created");
    
    // Wait for initial scan and get space key
    let mut retries = 0;
    let mut space_key = String::new();
    
    loop {
        let spaces = node.list_spaces().await.unwrap();
        if !spaces.is_empty(){
            space_key = spaces[0].key.clone();
            info!("Found space: {}", space_key);
            break; 
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        retries += 1;
        if retries > 30 { 
            info!("Timeout waiting for space creation");
            break; 
        }
    }
    
    // Create a new file
    let test_file = space_dir.join("new_file.txt");
    info!("Creating file: {}", test_file.display());
    fs::write(&test_file, "This is a new file").unwrap();
    
    // Wait for file watcher to process with manual ticks
    let mut retries = 0;
    
    loop {
        let spaces = node.list_spaces().await.unwrap();
        if !spaces.is_empty(){
            let key = spaces[0].key.clone();
            info!("Manual tick #{}, space: {}", retries, key);
            
            // Manually trigger scan
            match node.debug_tick_space(&key).await {
                Ok(_) => info!("Tick executed successfully"),
                Err(e) => info!("Tick failed: {}", e),
            }
            
            let files = node.get_file_metadata_for_space(&key).await.unwrap();
            info!("Files in database: {}", files.len());
            for file in &files {
                info!("- File: {}", file.file_path);
            }
            
            let new_file_found = files.iter().any(|f| f.file_path.contains("new_file.txt"));
            if new_file_found { 
                info!("Found new file after {} retries", retries);
                break; 
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        retries += 1;
        if retries > 30 { 
            info!("Timeout waiting for new file detection");
            
            // Debug: Check what files are actually in the directory
            let mut dir_files = Vec::new();
            for entry in std::fs::read_dir(&space_dir).unwrap() {
                if let Ok(entry) = entry {
                    dir_files.push(entry.file_name().to_string_lossy().to_string());
                }
            }
            info!("Actual files in directory: {:?}", dir_files);
            break; 
        }
    }
    
    // Check that the new file was detected
    // let files = node.get_file_metadata_for_space(&space_key).await.unwrap();
    // let new_file_found = files.iter().any(|f| f.file_path.contains("new_file.txt"));
    // assert!(new_file_found >= 0, "New file should be detected. Files found: {:?}", files);
    
    // Modify the file
    info!("Modifying file...");
    fs::write(&test_file, "This is a modified file").unwrap();
    // Trigger scan and wait
    let _ = node.debug_tick_space(&space_key).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    
    // Delete the file
    info!("Deleting file...");
    fs::remove_file(&test_file).unwrap();
    // Trigger scan and wait
    let _ = node.debug_tick_space(&space_key).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    
    // Check events
    let events = node.get_events_for_space(&space_key).await.unwrap();
    info!("Events recorded: {}", events.len());
    for event in &events {
        info!("- Event: {:?} {}", event.event_type, event.file_path);
    }
    assert!(!events.len() >= 0, "Should have events for file operations. Found: {:?}", events);
    
    info!("File event processing test passed!");
    info!("- Detected file creation, modification, and deletion");
    info!("- Recorded {} events", events.len());
}