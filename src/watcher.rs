use std::{
    net::IpAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use ipnet::IpNet;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::{sync::mpsc, time::sleep};
use tracing::{info, warn};

use crate::{
    config::{FileConfig, load_config},
    state::SharedState,
};

pub fn start(path: PathBuf, state: SharedState) -> Result<RecommendedWatcher> {
    let parent = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let (events, mut receiver) = mpsc::unbounded_channel();
    let mut watcher =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
            Ok(event)
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) =>
            {
                let _ = events.send(());
            }
            Ok(_) => {}
            Err(error) => warn!(%error, "configuration watcher error"),
        })
        .context("create configuration file watcher")?;
    watcher
        .watch(&parent, RecursiveMode::NonRecursive)
        .with_context(|| format!("watch configuration directory {}", parent.display()))?;

    tokio::spawn(async move {
        while receiver.recv().await.is_some() {
            sleep(Duration::from_millis(250)).await;
            while receiver.try_recv().is_ok() {}

            match load_config(&path).and_then(|config| apply_hot_reload(&state, config)) {
                Ok(listener_restart_needed) => {
                    if listener_restart_needed {
                        warn!(
                            "listener configuration changed; the existing listener remains active until restart"
                        );
                    }
                    info!("reloaded configuration; new upstream settings are active");
                }
                Err(error) => {
                    warn!(%error, "configuration reload rejected; keeping active settings")
                }
            }
        }
    });

    Ok(watcher)
}

pub fn apply_hot_reload(state: &SharedState, mut next: FileConfig) -> Result<bool> {
    let current = state.config.load_full();
    let listener_restart_needed = next.listener != current.listener;
    let preferred_changed = next.preferred != current.preferred;
    let upstreams_changed = next.upstreams != current.upstreams;
    if listener_restart_needed {
        next.listener = current.listener.clone();
    }
    next.validate()?;
    validate_preferred_ranges(&next, state.cloudflare_ranges.load().as_slice())?;
    if preferred_changed {
        state.replace_preferred_with_config(&next.preferred);
    }
    if upstreams_changed {
        state.clear_doh_clients();
    }
    state.config.store(Arc::new(next));
    state.config_changed.notify_waiters();
    Ok(listener_restart_needed)
}

pub fn validate_preferred_ranges(config: &FileConfig, ranges: &[IpNet]) -> Result<()> {
    if let Some(address) = config.preferred.ipv4
        && !ranges
            .iter()
            .any(|range| range.contains(&IpAddr::V4(address)))
    {
        bail!("preferred.ipv4 {address} is outside the active Cloudflare IP ranges");
    }
    if let Some(address) = config.preferred.ipv6
        && !ranges
            .iter()
            .any(|range| range.contains(&IpAddr::V6(address)))
    {
        bail!("preferred.ipv6 {address} is outside the active Cloudflare IP ranges");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, str::FromStr};

    use ipnet::IpNet;

    use super::*;
    use crate::{
        config::{ListenerConfig, UpstreamConfig},
        state::AppState,
    };

    fn config(upstream: &str) -> FileConfig {
        FileConfig {
            upstreams: vec![UpstreamConfig {
                address: upstream.parse().unwrap(),
                protocol: Default::default(),
                timeout_ms: 1_000,
                url: None,
                server_name: None,
            }],
            ..FileConfig::default()
        }
    }

    #[test]
    fn reload_replaces_upstreams_without_rebinding_listener() {
        let ranges = vec![IpNet::from_str("104.16.0.0/13").unwrap()];
        let state = AppState::new(config("1.1.1.1:53"), ranges);
        let changed = apply_hot_reload(&state, config("8.8.8.8:53")).unwrap();

        assert!(!changed);
        assert_eq!(
            state.config.load().upstreams[0].address,
            "8.8.8.8:53".parse().unwrap()
        );
    }

    #[test]
    fn reload_keeps_bound_listener_when_config_changes_it() {
        let ranges = vec![IpNet::from_str("104.16.0.0/13").unwrap()];
        let state = AppState::new(config("1.1.1.1:53"), ranges);
        let mut replacement = config("8.8.8.8:53");
        replacement.listener = ListenerConfig {
            address: "127.0.0.1:5353".parse().unwrap(),
            allow_remote: false,
        };

        assert!(apply_hot_reload(&state, replacement).unwrap());
        assert_eq!(state.config.load().listener, ListenerConfig::default());
    }

    #[test]
    fn rejects_non_cloudflare_static_preferred_ip() {
        let ranges = vec![IpNet::from_str("104.16.0.0/13").unwrap()];
        let state = AppState::new(config("1.1.1.1:53"), ranges);
        let mut replacement = config("8.8.8.8:53");
        replacement.preferred.ipv4 = Some(Ipv4Addr::new(198, 51, 100, 1));

        assert!(apply_hot_reload(&state, replacement).is_err());
    }

    #[test]
    fn reload_can_clear_static_preferred_ips() {
        let ranges = vec![IpNet::from_str("104.16.0.0/13").unwrap()];
        let mut initial = config("1.1.1.1:53");
        initial.preferred.ipv4 = Some(Ipv4Addr::new(104, 16, 99, 1));
        let state = AppState::new(initial, ranges);

        apply_hot_reload(&state, config("8.8.8.8:53")).unwrap();

        assert!(state.preferred_ips.load().is_empty());
    }
}
