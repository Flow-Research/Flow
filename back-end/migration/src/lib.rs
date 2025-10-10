pub use sea_orm_migration::prelude::*;

mod m20250811_140008_create_space;
mod m20250811_150000_create_repository;
mod m20250811_160000_create_file_metadata;
mod m20250811_170000_create_knowledge_graph;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250811_140008_create_space::Migration),
            Box::new(m20250811_150000_create_repository::Migration),
            Box::new(m20250811_160000_create_file_metadata::Migration),
            Box::new(m20250811_170000_create_knowledge_graph::Migration),
        ]
    }
}
