use crate::bootstrap::init::NodeData;
use crate::modules::ai::PipelineManager;
use crate::modules::ai::index::config::IndexingConfig;
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

struct InfrastructureServices {
    node_data: NodeData,
    db_conn: DatabaseConnection,
    kv: Db,
}

struct ApplicationServices {
    auth_state: AuthState,
    pipeline_manager: PipelineManager,
}

pub async fn run() -> Result<(), AppError> {
    init_tracing();

    let config = Config::from_env()?;
    info!("Configuration loaded. Initializing node...");

    let infra = init_infrastructure(&config).await?;
    let services = init_application_services(&config, &infra.db_conn).await?;
    let app_state = assemble_application(infra, services);

    run_servers(app_state, config).await
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

async fn init_infrastructure(config: &Config) -> Result<InfrastructureServices, AppError> {
    info!("Initializing infrastructure.");

    let (node_data, db_conn, kv) = tokio::try_join!(
        bootstrap::init::initialize(),
        setup_database(config),
        setup_kv_store(config)
    )?;

    info!("Infrastructure initialized successfully");
    Ok(InfrastructureServices {
        node_data,
        db_conn,
        kv,
    })
}

async fn init_application_services(
    _config: &Config,
    db_conn: &DatabaseConnection,
) -> Result<ApplicationServices, AppError> {
    info!("Initializing application services...");

    let auth_state = AuthState::from_env()?;
    let pipeline_manager = init_pipeline_manager(db_conn).await?;

    info!("Application services initialized successfully.");
    Ok(ApplicationServices {
        auth_state,
        pipeline_manager,
    })
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

async fn init_pipeline_manager(db_conn: &DatabaseConnection) -> Result<PipelineManager, AppError> {
    let indexing_config = IndexingConfig::from_env()
        .map_err(|_| AppError::Internal("Failed to initialize IndexingConfig".to_string()))?;

    let pipeline_manager = PipelineManager::new(db_conn.clone(), indexing_config);

    info!("Restoring pipelines for existing spaces...");
    pipeline_manager.initialize_from_database().await?;
    info!("Pipelines restored successfully.");

    Ok(pipeline_manager)
}

fn assemble_application(infra: InfrastructureServices, services: ApplicationServices) -> AppState {
    let node = Node::new(
        infra.node_data,
        infra.db_conn,
        infra.kv,
        services.auth_state,
        services.pipeline_manager,
    );
    AppState::new(node)
}

async fn run_servers(app_state: AppState, config: Config) -> Result<(), AppError> {
    info!("Starting servers...");

    tokio::select! {
        result = rest::start(&app_state, &config) => result?,
        result = websocket::start(&app_state, &config) => result?,
        _ = tokio::signal::ctrl_c() => info!("Shutdown signal received"),
    }

    info!("Application shutdown complete.");
    Ok(())
}
