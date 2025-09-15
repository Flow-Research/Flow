use crate::bootstrap::init::NodeData;
use crate::modules::space;
use errors::AppError;
use log::info;
use sea_orm::DatabaseConnection;

pub struct Node {
    node_data: NodeData,
    db: DatabaseConnection,
}

impl Node {
    pub fn new(node_data: NodeData, db: DatabaseConnection) -> Self {
        Node { node_data, db }
    }

    pub async fn create_space(&self, dir: &str) -> Result<(), AppError> {
        info!("Setting up space in Directory: {}", dir);
        space::new_space(&self.db, dir).await?;

        Ok(())
    }
}
