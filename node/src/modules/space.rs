use std::fs;
use std::path::Path;

use chrono::Utc;
use errors::AppError;
use log::{info, warn};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};

use entity::space;
use sha2::{Digest, Sha256};
use space::Entity as Space;

pub async fn new_space(db: &DatabaseConnection, dir: &str) -> Result<(), AppError> {
    info!("Setting up space in directory: {}", dir);

    let space_key = generate_space_key(dir)?;
    info!("Generated space key: {}", space_key);

    let path = Path::new(dir);
    if path.exists() {
        match Space::find()
            .filter(entity::space::Column::Key.eq(&space_key))
            .one(db)
            .await
        {
            Ok(Some(_existing_space)) => {
                info!(
                    "Space already exists at directory: {} (key: {})",
                    dir, space_key
                );
                return Ok(());
            }
            Ok(None) => {
                warn!(
                    "Directory exists but no space record found. Creating space record for: {}",
                    dir
                );
            }
            Err(e) => {
                return Err(AppError::Storage(Box::new(e)));
            }
        }
    } else {
        info!("Creating directory: {}", dir);
        fs::create_dir_all(path).map_err(|e| AppError::IO(e))?;
    }

    let canonical_location = path
        .canonicalize()
        .map_err(|e| AppError::IO(e))?
        .to_str()
        .ok_or_else(|| AppError::Config("Directory path contains invalid UTF-8".to_owned()))?
        .to_owned();

    let new_space = space::ActiveModel {
        key: Set(space_key.clone()),
        location: Set(canonical_location.clone()),
        time_created: Set(Utc::now().into()),
        ..Default::default()
    };

    match new_space.insert(db).await {
        Ok(space_model) => {
            info!(
                "Successfully created space with ID: {}, Key: {}, Location: {}",
                space_model.id, space_model.key, space_model.location
            );
            Ok(())
        }
        Err(e) => Err(AppError::Storage(Box::new(e))),
    }
}

fn generate_space_key(dir: &str) -> Result<String, AppError> {
    let path = Path::new(dir).canonicalize().map_err(|e| AppError::IO(e))?;

    let path_str = path
        .to_str()
        .ok_or_else(|| AppError::Config("Directory path contains invalid UTF-8".to_owned()))?;

    let mut hasher = Sha256::new();
    hasher.update(path_str.as_bytes());
    let hash = hasher.finalize();

    // Convert to hex string
    Ok(format!("{:x}", hash))
}

//////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_space_key_deterministic() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_str().unwrap();

        let key1 = generate_space_key(path).unwrap();
        let key2 = generate_space_key(path).unwrap();

        assert_eq!(key1, key2, "Same path should generate same key");
        assert_eq!(key1.len(), 64, "SHA-256 hex should be 64 characters");
    }

    #[test]
    fn test_generate_space_key_different_paths() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        let key1 = generate_space_key(temp_dir1.path().to_str().unwrap()).unwrap();
        let key2 = generate_space_key(temp_dir2.path().to_str().unwrap()).unwrap();

        assert_ne!(key1, key2, "Different paths should generate different keys");
    }

    #[test]
    fn test_generate_space_key_invalid_path() {
        let result = generate_space_key("/this/path/definitely/does/not/exist/nowhere");
        assert!(result.is_err(), "Non-existent path should return error");
    }
}
