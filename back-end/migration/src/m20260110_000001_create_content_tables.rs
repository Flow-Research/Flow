//! Migration to create published_content and remote_content tables.
//!
//! These tables track content published locally and discovered from
//! the network via GossipSub announcements.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create published_content table
        manager
            .create_table(
                Table::create()
                    .table(PublishedContent::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PublishedContent::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(PublishedContent::Cid)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(PublishedContent::Path).string())
                    .col(
                        ColumnDef::new(PublishedContent::Size)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(PublishedContent::BlockCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(PublishedContent::ContentType).string())
                    .col(ColumnDef::new(PublishedContent::Title).string())
                    .col(ColumnDef::new(PublishedContent::QdrantCollection).string())
                    .col(ColumnDef::new(PublishedContent::QdrantPointIds).json())
                    .col(
                        ColumnDef::new(PublishedContent::PublishedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Create index on published_at for sorting
        manager
            .create_index(
                Index::create()
                    .name("idx_published_content_published_at")
                    .table(PublishedContent::Table)
                    .col(PublishedContent::PublishedAt)
                    .to_owned(),
            )
            .await?;

        // Create remote_content table
        manager
            .create_table(
                Table::create()
                    .table(RemoteContent::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RemoteContent::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(RemoteContent::Cid)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(RemoteContent::PublisherPeerId)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(RemoteContent::PublisherDid).string())
                    .col(ColumnDef::new(RemoteContent::Metadata).json())
                    .col(ColumnDef::new(RemoteContent::ContentType).string())
                    .col(ColumnDef::new(RemoteContent::Title).string())
                    .col(
                        ColumnDef::new(RemoteContent::Size)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(RemoteContent::BlockCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(RemoteContent::DiscoveredAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(RemoteContent::Cached)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(RemoteContent::Indexed)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(RemoteContent::IndexError).string())
                    .col(ColumnDef::new(RemoteContent::QdrantCollection).string())
                    .col(ColumnDef::new(RemoteContent::EmbeddingCount).integer())
                    .col(ColumnDef::new(RemoteContent::IndexedAt).timestamp_with_time_zone())
                    .to_owned(),
            )
            .await?;

        // Create indexes for remote_content queries
        manager
            .create_index(
                Index::create()
                    .name("idx_remote_content_discovered_at")
                    .table(RemoteContent::Table)
                    .col(RemoteContent::DiscoveredAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_remote_content_indexed")
                    .table(RemoteContent::Table)
                    .col(RemoteContent::Indexed)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_remote_content_publisher_peer_id")
                    .table(RemoteContent::Table)
                    .col(RemoteContent::PublisherPeerId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_remote_content_publisher_did")
                    .table(RemoteContent::Table)
                    .col(RemoteContent::PublisherDid)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RemoteContent::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(PublishedContent::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PublishedContent {
    Table,
    Id,
    Cid,
    Path,
    Size,
    BlockCount,
    ContentType,
    Title,
    QdrantCollection,
    QdrantPointIds,
    PublishedAt,
}

#[derive(DeriveIden)]
enum RemoteContent {
    Table,
    Id,
    Cid,
    PublisherPeerId,
    PublisherDid,
    Metadata,
    ContentType,
    Title,
    Size,
    BlockCount,
    DiscoveredAt,
    Cached,
    Indexed,
    IndexError,
    QdrantCollection,
    EmbeddingCount,
    IndexedAt,
}
