use std::{future::Future, path::PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use tracing::info;

#[cfg(feature = "gui")]
pub mod agent;
pub mod config;
pub mod dns;
pub mod integration;
pub mod local_dns;
pub mod optimizer;
pub mod plugins;
pub mod ranges;
pub mod rule_sets;
pub mod state;
#[cfg(feature = "gui")]
pub mod tray;
#[cfg(feature = "gui")]
pub mod ui;
pub mod watcher;

pub const DEFAULT_CONFIG_FILE_NAME: &str = "edgesteer.json";

/// The command-line binary and native App always use one user-owned
/// configuration file.
pub fn default_config_path() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|home| !home.is_empty()))
        .map(PathBuf::from)
        .map(|home| home.join(DEFAULT_CONFIG_FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_FILE_NAME))
}

#[derive(Debug, Parser)]
#[command(
    name = "edgesteer",
    about = "Adaptive DNS steering for preferred Cloudflare edge IPs"
)]
pub struct Args {
    /// Validate the configuration and exit.
    #[arg(long)]
    pub check_config: bool,
}

pub async fn run(args: Args) -> Result<()> {
    let config_path = default_config_path();
    if args.check_config {
        validate_config(&config_path)?;
        println!("configuration is valid: {}", config_path.display());
        return Ok(());
    }

    run_with_shutdown(config_path, async {
        tokio::signal::ctrl_c()
            .await
            .expect("wait for shutdown signal");
    })
    .await
}

/// Starts the DNS engine until `shutdown` resolves.
///
/// The command-line binary supplies Ctrl-C as the shutdown signal. The native
/// application's lightweight Agent owns the same engine and supplies its
/// lifecycle signal, so macOS never needs a second command-line DNS service
/// beside the app bundle.
pub async fn run_with_shutdown<F>(config_path: PathBuf, shutdown: F) -> Result<()>
where
    F: Future<Output = ()> + Send,
{
    install_rustls_crypto_provider();
    let config = config::load_config(&config_path)?;
    let cloudflare_ranges = ranges::fallback_ranges()?;
    watcher::validate_preferred_ranges(&config, &cloudflare_ranges)?;

    let listener = config.listener.address;
    let state = state::AppState::new(config, cloudflare_ranges);
    let _watcher = watcher::start(config_path.clone(), state.clone())?;
    info!(%listener, config = %config_path.display(), "starting EdgeSteer");

    let mut dns_task = tokio::spawn(dns::serve(state.clone()));
    let _local_dns_task = tokio::spawn(local_dns::refresh_loop(state.clone()));
    let _range_task = tokio::spawn(ranges::refresh_loop(state.clone()));
    let _rule_set_task = tokio::spawn(rule_sets::refresh_loop(state.clone()));
    let _optimizer_task = tokio::spawn(optimizer::run_loop(state));

    tokio::select! {
        result = &mut dns_task => {
            result.context("DNS server task failed")??;
        }
        _ = shutdown => {
            info!("shutdown signal received");
            dns_task.abort();
            let _ = dns_task.await;
        }
    }
    Ok(())
}

fn validate_config(config_path: &std::path::Path) -> Result<config::FileConfig> {
    let config = config::load_config(config_path)?;
    let cloudflare_ranges = ranges::fallback_ranges()?;
    watcher::validate_preferred_ranges(&config, &cloudflare_ranges)?;
    Ok(config)
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

    #[test]
    fn default_config_file_name_is_stable() {
        assert_eq!(
            default_config_path()
                .file_name()
                .and_then(|name| name.to_str()),
            Some(DEFAULT_CONFIG_FILE_NAME)
        );
    }
}
