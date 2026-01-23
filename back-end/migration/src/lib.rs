pub use sea_orm_migration::prelude::*;

mod m20250811_140008_create_space;
mod m20251001_170115_create_user;
mod m20251001_171250_create_passkey;
mod m20251107_113838_create_space_index;
mod m20251107_164027_add_failure_tracking_to_space_index_status;
mod m20260103_180000_create_kg_entity;
mod m20260103_180001_create_kg_edge;
mod m20260103_180002_create_chunk;
mod m20260110_000000_add_space_name;
mod m20260110_000001_create_content_tables;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250811_140008_create_space::Migration),
            Box::new(m20251001_170115_create_user::Migration),
            Box::new(m20251001_171250_create_passkey::Migration),
            Box::new(m20251107_113838_create_space_index::Migration),
            Box::new(m20251107_164027_add_failure_tracking_to_space_index_status::Migration),
            Box::new(m20260103_180000_create_kg_entity::Migration),
            Box::new(m20260103_180001_create_kg_edge::Migration),
            Box::new(m20260103_180002_create_chunk::Migration),
            Box::new(m20260110_000000_add_space_name::Migration),
            Box::new(m20260110_000001_create_content_tables::Migration),
        ]
    }
}
