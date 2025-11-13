use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SpaceIndexStatus::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SpaceIndexStatus::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(SpaceIndexStatus::SpaceId)
                            .integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(SpaceIndexStatus::LastIndexed).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(SpaceIndexStatus::IndexingInProgress)
                            .boolean()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(SpaceIndexStatus::FilesIndexed)
                            .integer()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(SpaceIndexStatus::ChunksStored)
                            .integer()
                            .default(0),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(SpaceIndexStatus::Table, SpaceIndexStatus::SpaceId)
                            .to(Space::Table, Space::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SpaceIndexStatus::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum SpaceIndexStatus {
    Table,
    Id,
    SpaceId,
    LastIndexed,
    IndexingInProgress,
    FilesIndexed,
    ChunksStored,
}

#[derive(DeriveIden)]
enum Space {
    Table,
    Id,
}
