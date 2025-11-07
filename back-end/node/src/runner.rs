use crate::modules::ai_pipeline::PipelineManager;
use crate::modules::ai_pipeline::index::config::IndexingConfig;
use crate::{
    api::{
        node::Node,
        servers::{app_state::AppState, rest, websocket},
    },
    bootstrap::{self, config::Config},
    modules::ssi::webauthn::state::AuthState,
};
use errors::AppError;
use log::info;
use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectOptions, DatabaseConnection};
use sled::Db;

pub async fn run() -> Result<(), AppError> {
    let config = Config::from_env()?;
    init_tracing();

    info!("Configuration loaded. Initializing node...");

    // Initialize foundational services like logging here (if any).
    // Bootstrap the node identity, file system, etc.
    let node_data = bootstrap::init::initialize()?;
    info!("Node initialized successfully.");

    // Set up the database connection and run migrations.
    let db_conn = setup_database(&config).await?;
    info!("Database setup and migrations complete.");

    // Set up KV Store
    let kv = setup_kv_store(&config).await?;

    // Initialize Auth State
    let auth_state = AuthState::from_env()?;

    // Initialize pipeline manager
    let indexing_config = IndexingConfig::from_env()
        .map_err(|_| AppError::Internal("Failed to initialize IndexingConfig".to_string()))?;
    let pipeline_manager = PipelineManager::new(db_conn.clone(), indexing_config);

    // Restore all pipelines from database
    info!("Restoring pipelines for existing spaces...");
    pipeline_manager.initialize_from_database().await?;
    info!("Pipelines restored successfully.");

    let node = Node::new(node_data, db_conn, kv, auth_state, pipeline_manager);
    let app_state = AppState::new(node);

    info!("Starting servers...");

    // --- Application is now running ---
    // Start server, event loops, or other long-running
    // tasks, using the initialized objects.

    tokio::select! {
        result = rest::start(&app_state, &config) => {
            result?;
        }
        result = websocket::start(&app_state, &config) => {
            result?;
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Shutdown signal received");
        }
    }

    info!("Application running. Press Ctrl+C to exit.");

    Ok(())
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();
}

async fn setup_database(config: &Config) -> Result<DatabaseConnection, AppError> {
    info!("Setting up Database");

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

async fn setup_kv_store(config: &Config) -> Result<Db, AppError> {
    info!("Setting up KVStore");
    Ok(sled::open(config.kv.path.as_str()).unwrap())
}
