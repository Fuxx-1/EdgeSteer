use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use edgesteer::Args;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();
    edgesteer::run(Args::parse()).await
}
