pub use sea_orm_migration::prelude::*;

mod m20250811_140008_create_space;
mod m20251001_170115_create_user;
mod m20251001_171250_create_passkey;
mod m20251107_113838_create_space_index;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250811_140008_create_space::Migration),
            Box::new(m20251001_170115_create_user::Migration),
            Box::new(m20251001_171250_create_passkey::Migration),
            Box::new(m20251107_113838_create_space_index::Migration),
        ]
    }
}
