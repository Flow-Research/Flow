use sea_orm_migration::{
    prelude::*,
    schema::{pk_auto, string, timestamp_with_time_zone, integer, text, boolean},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(KnowledgeNode::Table)
                    .if_not_exists()
                    .col(pk_auto(KnowledgeNode::Id))
                    .col(string(KnowledgeNode::NodeId))
                    .col(string(KnowledgeNode::NodeType))
                    .col(string(KnowledgeNode::Title))
                    .col(text(KnowledgeNode::Content))
                    .col(string(KnowledgeNode::SourceFile))
                    .col(string(KnowledgeNode::SourceSpace))
                    .col(timestamp_with_time_zone(KnowledgeNode::CreatedAt))
                    .col(timestamp_with_time_zone(KnowledgeNode::UpdatedAt))
                    .col(integer(KnowledgeNode::ConfidenceScore))
                    .col(text(KnowledgeNode::Metadata))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(KnowledgeEdge::Table)
                    .if_not_exists()
                    .col(pk_auto(KnowledgeEdge::Id))
                    .col(string(KnowledgeEdge::FromNodeId))
                    .col(string(KnowledgeEdge::ToNodeId))
                    .col(string(KnowledgeEdge::RelationshipType))
                    .col(integer(KnowledgeEdge::Weight))
                    .col(timestamp_with_time_zone(KnowledgeEdge::CreatedAt))
                    .col(text(KnowledgeEdge::Metadata))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(KnowledgeEdge::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(KnowledgeNode::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum KnowledgeNode {
    Table,
    Id,
    NodeId,
    NodeType,
    Title,
    Content,
    SourceFile,
    SourceSpace,
    CreatedAt,
    UpdatedAt,
    ConfidenceScore,
    Metadata,
}

#[derive(DeriveIden)]
enum KnowledgeEdge {
    Table,
    Id,
    FromNodeId,
    ToNodeId,
    RelationshipType,
    Weight,
    CreatedAt,
    Metadata,
}
