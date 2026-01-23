use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(KgEdge::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(KgEdge::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(KgEdge::Cid)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(KgEdge::SpaceId).integer().not_null())
                    .col(ColumnDef::new(KgEdge::EdgeType).string().not_null())
                    .col(ColumnDef::new(KgEdge::SourceId).integer().not_null())
                    .col(ColumnDef::new(KgEdge::TargetId).integer().not_null())
                    .col(ColumnDef::new(KgEdge::Properties).json())
                    .col(
                        ColumnDef::new(KgEdge::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(KgEdge::CreatedBy).string().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_kg_edge_space")
                            .from(KgEdge::Table, KgEdge::SpaceId)
                            .to(Space::Table, Space::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_kg_edge_source")
                            .from(KgEdge::Table, KgEdge::SourceId)
                            .to(KgEntity::Table, KgEntity::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_kg_edge_target")
                            .from(KgEdge::Table, KgEdge::TargetId)
                            .to(KgEntity::Table, KgEntity::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_kg_edges_source")
                    .table(KgEdge::Table)
                    .col(KgEdge::SourceId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_kg_edges_target")
                    .table(KgEdge::Table)
                    .col(KgEdge::TargetId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_kg_edges_type")
                    .table(KgEdge::Table)
                    .col(KgEdge::EdgeType)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(KgEdge::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum KgEdge {
    Table,
    Id,
    Cid,
    SpaceId,
    EdgeType,
    SourceId,
    TargetId,
    Properties,
    CreatedAt,
    CreatedBy,
}

#[derive(DeriveIden)]
enum Space {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum KgEntity {
    Table,
    Id,
}
