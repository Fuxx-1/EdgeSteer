use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use tracing::info;

pub mod config;
pub mod dns;
pub mod local_dns;
pub mod optimizer;
pub mod plugins;
pub mod ranges;
pub mod rule_sets;
pub mod state;
pub mod watcher;

#[derive(Debug, Parser)]
#[command(
    name = "edgesteer",
    about = "Adaptive DNS steering for preferred Cloudflare edge IPs"
)]
pub struct Args {
    /// JSON configuration file.
    #[arg(short, long, default_value = "edgesteer.json")]
    pub config: PathBuf,

    /// Validate the configuration and exit.
    #[arg(long)]
    pub check_config: bool,
}

pub async fn run(args: Args) -> Result<()> {
    install_rustls_crypto_provider();
    let config = config::load_config(&args.config)?;
    let cloudflare_ranges = ranges::fallback_ranges()?;
    watcher::validate_preferred_ranges(&config, &cloudflare_ranges)?;
    if args.check_config {
        println!("configuration is valid: {}", args.config.display());
        return Ok(());
    }

    let listener = config.listener.address;
    let state = state::AppState::new(config, cloudflare_ranges);
    let _watcher = watcher::start(args.config.clone(), state.clone())?;
    info!(%listener, config = %args.config.display(), "starting EdgeSteer");

    let dns_task = tokio::spawn(dns::serve(state.clone()));
    let _local_dns_task = tokio::spawn(local_dns::refresh_loop(state.clone()));
    let _range_task = tokio::spawn(ranges::refresh_loop(state.clone()));
    let _rule_set_task = tokio::spawn(rule_sets::refresh_loop(state.clone()));
    let _optimizer_task = tokio::spawn(optimizer::run_loop(state));

    tokio::select! {
        result = dns_task => {
            result.context("DNS server task failed")??;
        }
        result = tokio::signal::ctrl_c() => {
            result.context("wait for shutdown signal")?;
            info!("shutdown signal received");
        }
    }
    Ok(())
}

pub(crate) fn install_rustls_crypto_provider() {
    // An error only means another provider was installed earlier in this process.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_a_rustls_crypto_provider() {
        install_rustls_crypto_provider();
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }
}
