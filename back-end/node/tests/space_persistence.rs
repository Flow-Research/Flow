use node::{
    api::node::Node,
    bootstrap::init::NodeData,
};
use sea_orm::{Database, DatabaseConnection};
use migration::MigratorTrait;
use tempfile::TempDir;

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
async fn test_create_space_idempotency() {
    let (node, temp_dir) = create_test_node().await;
    let test_dir = temp_dir.path().join("test_space");
    std::fs::create_dir_all(&test_dir).unwrap();
    
    let dir_str = test_dir.to_str().unwrap();
    
    // Create space first time
    node.create_space(dir_str).await.unwrap();
    
    // Verify it was created
    let spaces = node.list_spaces().await.unwrap();
    assert_eq!(spaces.len(), 1);
    assert_eq!(spaces[0].location, test_dir.canonicalize().unwrap().to_str().unwrap());
    
    // Create same space again - should be idempotent
    node.create_space(dir_str).await.unwrap();
    
    // Should still have only one space
    let spaces = node.list_spaces().await.unwrap();
    assert_eq!(spaces.len(), 1);
}

#[tokio::test]
async fn test_list_spaces_empty() {
    let (node, _temp_dir) = create_test_node().await;
    
    let spaces = node.list_spaces().await.unwrap();
    assert_eq!(spaces.len(), 0);
}

#[tokio::test]
async fn test_multiple_spaces() {
    let (node, temp_dir) = create_test_node().await;
    
    let space1 = temp_dir.path().join("space1");
    let space2 = temp_dir.path().join("space2");
    
    std::fs::create_dir_all(&space1).unwrap();
    std::fs::create_dir_all(&space2).unwrap();
    
    node.create_space(space1.to_str().unwrap()).await.unwrap();
    node.create_space(space2.to_str().unwrap()).await.unwrap();
    
    let spaces = node.list_spaces().await.unwrap();
    assert_eq!(spaces.len(), 2);
}

#[tokio::test]
async fn test_space_key_deterministic() {
    let (node, temp_dir) = create_test_node().await;
    let test_dir = temp_dir.path().join("deterministic_test");
    std::fs::create_dir_all(&test_dir).unwrap();
    
    let dir_str = test_dir.to_str().unwrap();
    
    // Create space
    node.create_space(dir_str).await.unwrap();
    
    let spaces = node.list_spaces().await.unwrap();
    let key1 = spaces[0].key.clone();
    
    // Create another node with same DB
    let node_data = NodeData {
        id: "test-did-2".to_string(),
        private_key: vec![1; 32],
        public_key: vec![1; 32],
    };
    let (db, _) = setup_test_db().await;
    let node2 = Node::new(node_data, db);
    
    // Create same space with different node
    node2.create_space(dir_str).await.unwrap();
    
    let spaces = node2.list_spaces().await.unwrap();
    let key2 = spaces[0].key.clone();
    
    // Keys should be the same (deterministic based on path)
    assert_eq!(key1, key2);
}
