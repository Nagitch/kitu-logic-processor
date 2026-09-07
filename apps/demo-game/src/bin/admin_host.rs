//! Standalone scheduler and HTTP entry point for the shared Arena host.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    kitu_demo_game::host::serve_from_environment().await
}
