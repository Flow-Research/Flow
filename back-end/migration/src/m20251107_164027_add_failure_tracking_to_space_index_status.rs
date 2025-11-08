use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let _ = manager
            .alter_table(
                Table::alter()
                    .table(SpaceIndexStatus::Table)
                    .add_column(
                        ColumnDef::new(SpaceIndexStatus::FilesFailed)
                            .integer()
                            .null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await;

        manager
            .alter_table(
                Table::alter()
                    .table(SpaceIndexStatus::Table)
                    .add_column(ColumnDef::new(SpaceIndexStatus::LastError).text().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let _ = manager
            .alter_table(
                Table::alter()
                    .table(SpaceIndexStatus::Table)
                    .drop_column(SpaceIndexStatus::FilesFailed)
                    .to_owned(),
            )
            .await;

        manager
            .alter_table(
                Table::alter()
                    .table(SpaceIndexStatus::Table)
                    .drop_column(SpaceIndexStatus::LastError)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum SpaceIndexStatus {
    Table,
    FilesFailed,
    LastError,
}
