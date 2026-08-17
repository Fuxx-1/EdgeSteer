use std::{
    net::IpAddr,
    path::{Path, PathBuf},
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
                    info!("reloaded configuration; new layer settings are active");
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
    // Validate both the saved listener and the currently bound listener. A
    // listener change cannot be applied without a restart, so the second pass
    // prevents a newly saved local layer from forwarding back to the old socket.
    next.validate()?;
    let current = state.runtime.load_full();
    let listener_restart_needed = next.listener != current.config.listener;
    let layers_changed = next.layers != current.config.layers;
    if listener_restart_needed {
        next.listener = current.config.listener.clone();
        next.validate()?;
    }
    validate_preferred_ranges(&next, state.cloudflare_ranges.load().as_slice())?;

    state.replace_config(next);
    if layers_changed {
        state.clear_doh_clients();
    }
    state.config_changed.notify_waiters();
    Ok(listener_restart_needed)
}

pub fn validate_preferred_ranges(config: &FileConfig, ranges: &[IpNet]) -> Result<()> {
    for plugin in config.cloudflare_preferred_plugins() {
        if let Some(address) = plugin.preferred.ipv4 {
            if !ranges
                .iter()
                .any(|range| range.contains(&IpAddr::V4(address)))
            {
                bail!(
                    "plugin {:?} preferred.ipv4 {address} is outside the active Cloudflare IP ranges",
                    plugin.tag
                );
            }
        }
        if let Some(address) = plugin.preferred.ipv6 {
            if !ranges
                .iter()
                .any(|range| range.contains(&IpAddr::V6(address)))
            {
                bail!(
                    "plugin {:?} preferred.ipv6 {address} is outside the active Cloudflare IP ranges",
                    plugin.tag
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, str::FromStr};

    use ipnet::IpNet;

    use super::*;
    use crate::{
        config::{
            LayerConfig, LayerType, ListenerConfig, PluginConfig, PluginType, PreferredConfig,
        },
        state::AppState,
    };

    fn config(upstream: &str) -> FileConfig {
        FileConfig {
            entry: "local".to_owned(),
            layers: vec![LayerConfig {
                tag: "local".to_owned(),
                kind: LayerType::Udp,
                next: None,
                fallback: None,
                matcher: Default::default(),
                address: Some(upstream.parse().unwrap()),
                timeout_ms: Some(1_000),
                refresh_secs: None,
                url: None,
                server_name: None,
                plugin: None,
            }],
            ..FileConfig::default()
        }
    }

    fn preferred_plugin(ipv4: Option<Ipv4Addr>) -> PluginConfig {
        PluginConfig {
            tag: "preferred".to_owned(),
            kind: PluginType::CloudflarePreferred,
            rewrite_ttl_secs: 60,
            preferred: PreferredConfig { ipv4, ipv6: None },
            optimizer: Default::default(),
        }
    }

    #[test]
    fn reload_replaces_layers_without_rebinding_listener() {
        let ranges = vec![IpNet::from_str("104.16.0.0/13").unwrap()];
        let state = AppState::new(config("1.1.1.1:53"), ranges);
        let changed = apply_hot_reload(&state, config("8.8.8.8:53")).unwrap();

        assert!(!changed);
        assert_eq!(
            state.runtime.load().config.layers[0].address(),
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
        assert_eq!(
            state.runtime.load().config.listener,
            ListenerConfig::default()
        );
    }

    #[test]
    fn rejects_non_cloudflare_static_preferred_ip() {
        let ranges = vec![IpNet::from_str("104.16.0.0/13").unwrap()];
        let state = AppState::new(config("1.1.1.1:53"), ranges);
        let mut replacement = config("8.8.8.8:53");
        replacement
            .plugins
            .push(preferred_plugin(Some(Ipv4Addr::new(198, 51, 100, 1))));

        assert!(apply_hot_reload(&state, replacement).is_err());
    }

    #[test]
    fn reload_can_clear_static_preferred_ips() {
        let ranges = vec![IpNet::from_str("104.16.0.0/13").unwrap()];
        let mut initial = config("1.1.1.1:53");
        initial
            .plugins
            .push(preferred_plugin(Some(Ipv4Addr::new(104, 16, 99, 1))));
        let state = AppState::new(initial, ranges);

        apply_hot_reload(&state, config("8.8.8.8:53")).unwrap();

        assert!(state.runtime.load().preferred("preferred").is_none());
    }
}
