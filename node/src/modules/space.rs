use errors::AppError;
use sea_orm::DatabaseConnection;

use entity::space::Entity as Space;

pub fn new_space(db: &DatabaseConnection, dir: &str) -> Result<(), AppError> {
    // Check space directory already exists
    //
    // Persist new space entity

    let _ = (db, dir); // TEMP: Remove "unused variable" warning.
    Ok(())
}
