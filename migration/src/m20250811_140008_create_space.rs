use sea_orm_migration::{
    prelude::*,
    schema::{pk_auto, string, timestamp_with_time_zone},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Space::Table)
                    .if_not_exists()
                    .col(pk_auto(Space::Id))
                    .col(string(Space::Key))
                    .col(string(Space::Location))
                    .col(timestamp_with_time_zone(Space::TimeCreated))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Space::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Space {
    Table,
    Id,
    Key,
    Location,
    TimeCreated,
}
