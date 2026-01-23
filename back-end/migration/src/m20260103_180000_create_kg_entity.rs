use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(KgEntity::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(KgEntity::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(KgEntity::Cid)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(KgEntity::SpaceId).integer().not_null())
                    .col(
                        ColumnDef::new(KgEntity::EntityType)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(KgEntity::Name).string().not_null())
                    .col(ColumnDef::new(KgEntity::Properties).json())
                    .col(ColumnDef::new(KgEntity::Embedding).binary())
                    .col(
                        ColumnDef::new(KgEntity::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(KgEntity::CreatedBy).string().not_null())
                    .col(ColumnDef::new(KgEntity::SourceChunkId).string())
                    .foreign_key(
                        ForeignKey::create()
                            .from(KgEntity::Table, KgEntity::SpaceId)
                            .to(Space::Table, Space::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_kg_entities_space")
                    .table(KgEntity::Table)
                    .col(KgEntity::SpaceId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_kg_entities_type")
                    .table(KgEntity::Table)
                    .col(KgEntity::EntityType)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_kg_entities_name")
                    .table(KgEntity::Table)
                    .col(KgEntity::Name)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(KgEntity::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum KgEntity {
    Table,
    Id,
    Cid,
    SpaceId,
    EntityType,
    Name,
    Properties,
    Embedding,
    CreatedAt,
    CreatedBy,
    SourceChunkId,
}

#[derive(DeriveIden)]
enum Space {
    Table,
    Id,
}
