use log::error;

#[tokio::main]
async fn main() {
    env_logger::init();

    if let Err(e) = node::runner::run().await {
        error!("Application failed to start: {}", e);
        std::process::exit(1);
    }
}
