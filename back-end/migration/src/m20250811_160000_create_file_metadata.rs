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
                    .table(FileMetadata::Table)
                    .if_not_exists()
                    .col(pk_auto(FileMetadata::Id))
                    .col(string(FileMetadata::SpaceKey))
                    .col(string(FileMetadata::FilePath))
                    .col(string(FileMetadata::FileName))
                    .col(string(FileMetadata::FileExtension))
                    .col(integer(FileMetadata::FileSize))
                    .col(string(FileMetadata::FileHash))
                    .col(string(FileMetadata::MimeType))
                    .col(boolean(FileMetadata::IsDirectory))
                    .col(timestamp_with_time_zone(FileMetadata::LastModified))
                    .col(timestamp_with_time_zone(FileMetadata::CreatedAt))
                    .col(text(FileMetadata::ContentPreview))
                    .col(text(FileMetadata::Tags))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(FileMetadata::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum FileMetadata {
    Table,
    Id,
    SpaceKey,
    FilePath,
    FileName,
    FileExtension,
    FileSize,
    FileHash,
    MimeType,
    IsDirectory,
    LastModified,
    CreatedAt,
    ContentPreview,
    Tags,
}
