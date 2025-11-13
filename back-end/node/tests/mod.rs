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
