use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{DbBackend, Statement};
use sha2::{Digest, Sha256};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Step 1: Add nullable name column to space table
        manager
            .alter_table(
                Table::alter()
                    .table(Space::Table)
                    .add_column(ColumnDef::new(Space::Name).string_len(255).null())
                    .to_owned(),
            )
            .await?;

        // Step 2: Backfill existing spaces with names derived from their paths
        let db = manager.get_connection();

        // Get all spaces that need names
        let spaces: Vec<(i32, String)> = db
            .query_all(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT id, location FROM space WHERE name IS NULL".to_owned(),
            ))
            .await?
            .iter()
            .filter_map(|row| {
                let id: Option<i32> = row.try_get("", "id").ok();
                let location: Option<String> = row.try_get("", "location").ok();
                match (id, location) {
                    (Some(id), Some(location)) => Some((id, location)),
                    _ => None,
                }
            })
            .collect();

        // Update each space with a derived name
        for (id, location) in spaces {
            let name = derive_name_from_path(&location);
            db.execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE space SET name = $1 WHERE id = $2",
                [name.into(), id.into()],
            ))
            .await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Space::Table)
                    .drop_column(Space::Name)
                    .to_owned(),
            )
            .await
    }
}

/// Derives a human-readable name from a filesystem path.
///
/// # Examples
/// - `/Users/julian/Documents/specs` → `specs`
/// - `/path/to/folder/` → `folder` (trailing slash handled)
/// - `/` → `<8-char hash>` (fallback for edge cases)
fn derive_name_from_path(location: &str) -> String {
    // Handle trailing slashes by trimming them first
    let trimmed = location.trim_end_matches('/');
    let path = std::path::Path::new(trimmed);

    // Try to extract the last path segment
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let trimmed_name = name.trim();
        if !trimmed_name.is_empty() {
            return trimmed_name.to_owned();
        }
    }

    // Fallback: use first 8 chars of SHA-256 hash
    let mut hasher = Sha256::new();
    hasher.update(location.as_bytes());
    let hash = hasher.finalize();
    format!("{:x}", hash)[..8].to_owned()
}

#[derive(DeriveIden)]
enum Space {
    Table,
    Name,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_name_normal_path() {
        assert_eq!(
            derive_name_from_path("/Users/julian/Documents/specs"),
            "specs"
        );
    }

    #[test]
    fn test_derive_name_trailing_slash() {
        assert_eq!(
            derive_name_from_path("/Users/julian/Documents/specs/"),
            "specs"
        );
    }

    #[test]
    fn test_derive_name_single_segment() {
        assert_eq!(derive_name_from_path("docs"), "docs");
    }

    #[test]
    fn test_derive_name_root_path() {
        // Root path should return hash prefix
        let result = derive_name_from_path("/");
        assert_eq!(result.len(), 8);
    }

    #[test]
    fn test_derive_name_unicode() {
        assert_eq!(derive_name_from_path("/path/to/文档"), "文档");
    }

    #[test]
    fn test_derive_name_special_chars() {
        assert_eq!(
            derive_name_from_path("/path/to/my docs (2024)"),
            "my docs (2024)"
        );
    }
}
