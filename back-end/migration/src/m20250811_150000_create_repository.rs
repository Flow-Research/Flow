use sea_orm_migration::{
    prelude::*,
    schema::{pk_auto, string, timestamp_with_time_zone, integer, text},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Repository::Table)
                    .if_not_exists()
                    .col(pk_auto(Repository::Id))
                    .col(string(Repository::SpaceKey))
                    .col(string(Repository::FilePath))
                    .col(string(Repository::EventType))
                    .col(timestamp_with_time_zone(Repository::Timestamp))
                    .col(integer(Repository::FileSize))
                    .col(string(Repository::FileHash))
                    .col(text(Repository::Metadata))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Repository::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Repository {
    Table,
    Id,
    SpaceKey,
    FilePath,
    EventType,
    Timestamp,
    FileSize,
    FileHash,
    Metadata,
}
