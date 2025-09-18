use crate::{
    api::node::Node,
    bootstrap::{self, config::Config},
};
use errors::AppError;
use log::info;
use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectOptions, DatabaseConnection};

pub async fn run() -> Result<(), AppError> {
    let config = Config::from_env()?;

    info!("Configuration loaded. Initializing node...");

    // Initialize foundational services like logging here (if any).
    // Bootstrap the node identity, file system, etc.
    let node_data = bootstrap::init::initialize()?;
    info!("Node initialized successfully.");

    // Set up the database connection and run migrations.
    let db_conn = setup_database(&config).await?;
    info!("Database setup and migrations complete.");

    let _node = Node::new(node_data, db_conn);

    // --- Application is now running ---
    // Start server, event loops, or other long-running
    // tasks, using the initialized objects.
    info!("Application running. Press Ctrl+C to exit.");
    // For example, wait forever:
    tokio::signal::ctrl_c().await?;

    Ok(())
}

async fn setup_database(config: &Config) -> Result<DatabaseConnection, AppError> {
    let db_config = &config.db;
    let mut opt = ConnectOptions::new(&db_config.url);

    opt.max_connections(db_config.max_connections)
        .min_connections(db_config.min_connections)
        .connect_timeout(db_config.connect_timeout)
        .idle_timeout(db_config.idle_timeout)
        .max_lifetime(db_config.max_lifetime)
        .sqlx_logging(db_config.logging_enabled)
        .sqlx_logging_level(log::LevelFilter::Info); // #TODO: hard-coded right now, remember to externalize into a config

    let connection = sea_orm::Database::connect(opt)
        .await
        .map_err(|db_err| AppError::Storage(Box::new(db_err)))?;

    info!("Running database migrations...");
    Migrator::up(&connection, None)
        .await
        .map_err(|db_err| AppError::Migration(Box::new(db_err)))?;

    Ok(connection)
}
