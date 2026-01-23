use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Chunk::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Chunk::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Chunk::Cid)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Chunk::DocumentId).integer().not_null())
                    .col(ColumnDef::new(Chunk::SpaceId).integer().not_null())
                    .col(ColumnDef::new(Chunk::PositionStart).integer())
                    .col(ColumnDef::new(Chunk::PositionEnd).integer())
                    .col(ColumnDef::new(Chunk::CharCount).integer())
                    .col(ColumnDef::new(Chunk::Metadata).json())
                    .col(
                        ColumnDef::new(Chunk::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_chunk_document")
                            .from(Chunk::Table, Chunk::DocumentId)
                            .to(KgEntity::Table, KgEntity::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_chunk_space")
                            .from(Chunk::Table, Chunk::SpaceId)
                            .to(Space::Table, Space::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_chunks_document")
                    .table(Chunk::Table)
                    .col(Chunk::DocumentId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_chunks_space")
                    .table(Chunk::Table)
                    .col(Chunk::SpaceId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Chunk::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Chunk {
    Table,
    Id,
    Cid,
    DocumentId,
    SpaceId,
    PositionStart,
    PositionEnd,
    CharCount,
    Metadata,
    CreatedAt,
}

#[derive(DeriveIden)]
enum KgEntity {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Space {
    Table,
    Id,
}
