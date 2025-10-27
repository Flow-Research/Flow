pub mod api;
pub mod bootstrap;
pub mod modules;
pub mod util;

#[cfg(test)]
#[ctor::ctor]
fn global_test_setup() {
    dotenvy::dotenv().ok();

    env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Debug)
        .format_timestamp_millis()
        .try_init()
        .ok();

    log::info!("✓ Global logger initialized");
}
