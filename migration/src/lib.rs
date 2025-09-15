pub use sea_orm_migration::prelude::*;

mod m20250811_140008_create_space;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20250811_140008_create_space::Migration)]
    }
}
