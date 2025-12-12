use tempfile::TempDir;

use crate::bootstrap::init::setup_test_env;

pub mod api;
pub mod bootstrap;
pub mod modules;
pub mod util;

#[cfg(test)]
#[ctor::ctor]
fn global_test_setup() {
    dotenvy::dotenv().ok();

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

/// Holds temp directories and manages env vars for a test node
pub struct TestNodeDirs {
    temp_dir: TempDir,
    name: String,
}

impl TestNodeDirs {
    fn new(name: &str) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        Self {
            temp_dir,
            name: name.to_string(),
        }
    }

    /// Set environment variables for this node's paths.
    fn set_env_vars(&self) {
        setup_test_env(&self.temp_dir, &self.name);
    }

    fn block_store_path(&self) -> std::path::PathBuf {
        self.temp_dir.path().join(format!("{}_blocks", self.name))
    }
}
