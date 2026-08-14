use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::Duration,
};

use arc_swap::ArcSwap;
use ipnet::IpNet;
use reqwest::{Client, redirect::Policy};
use tokio::sync::{Notify, Semaphore};

use crate::{
    config::{FileConfig, LayerConfig, PluginConfig, PreferredConfig},
    rule_sets::RuleSetStore,
};

pub const MAX_IN_FLIGHT_QUERIES: usize = 128;

#[derive(Debug, Hash, PartialEq, Eq)]
struct DohClientKey {
    endpoint: String,
    bootstrap_address: SocketAddr,
    timeout_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreferredIps {
    pub ipv4: Option<Ipv4Addr>,
    pub ipv6: Option<Ipv6Addr>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalResolvers {
    addresses: Vec<SocketAddr>,
}

impl LocalResolvers {
    pub fn new(addresses: Vec<SocketAddr>) -> Self {
        Self { addresses }
    }

    pub fn addresses(&self) -> &[SocketAddr] {
        &self.addresses
    }
}

impl PreferredIps {
    pub fn from_config(config: &PreferredConfig) -> Self {
        Self {
            ipv4: config.ipv4,
            ipv6: config.ipv6,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ipv4.is_none() && self.ipv6.is_none()
    }
}

/// A request always reads this as one snapshot, so plugin settings and the
/// preferred addresses chosen by their optimizers cannot get mixed between
/// configuration generations.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub config: FileConfig,
    preferred_ips: HashMap<String, PreferredIps>,
}

impl RuntimeConfig {
    pub fn new(config: FileConfig) -> Self {
        let preferred_ips = static_preferred_ips(&config);
        Self {
            config,
            preferred_ips,
        }
    }

    pub fn with_reloaded_config(config: FileConfig, previous: &Self) -> Self {
        let mut preferred_ips = HashMap::new();
        for plugin in config.cloudflare_preferred_plugins() {
            let unchanged_static_value = previous
                .config
                .plugin(&plugin.tag)
                .is_some_and(|previous_plugin| previous_plugin.preferred == plugin.preferred);
            let preferred = if unchanged_static_value {
                previous
                    .preferred(&plugin.tag)
                    .cloned()
                    .unwrap_or_else(|| PreferredIps::from_config(&plugin.preferred))
            } else {
                PreferredIps::from_config(&plugin.preferred)
            };
            preferred_ips.insert(plugin.tag.clone(), preferred);
        }
        Self {
            config,
            preferred_ips,
        }
    }

    pub fn preferred(&self, plugin_tag: &str) -> Option<&PreferredIps> {
        self.preferred_ips.get(plugin_tag)
    }

    fn preferred_mut(&mut self, plugin_tag: &str) -> &mut PreferredIps {
        self.preferred_ips.entry(plugin_tag.to_owned()).or_default()
    }
}

fn static_preferred_ips(config: &FileConfig) -> HashMap<String, PreferredIps> {
    config
        .cloudflare_preferred_plugins()
        .map(|plugin| {
            (
                plugin.tag.clone(),
                PreferredIps::from_config(&plugin.preferred),
            )
        })
        .collect()
}

pub struct AppState {
    pub runtime: ArcSwap<RuntimeConfig>,
    pub cloudflare_ranges: ArcSwap<Vec<IpNet>>,
    pub rule_sets: ArcSwap<RuleSetStore>,
    local_resolvers: ArcSwap<LocalResolvers>,
    pub config_changed: Notify,
    pub query_permits: Arc<Semaphore>,
    runtime_update_lock: Mutex<()>,
    doh_clients: Mutex<HashMap<DohClientKey, Client>>,
    pub(crate) local_dns_refresh_lock: tokio::sync::Mutex<()>,
}

pub type SharedState = Arc<AppState>;

impl AppState {
    pub fn new(config: FileConfig, cloudflare_ranges: Vec<IpNet>) -> SharedState {
        Arc::new(Self {
            runtime: ArcSwap::from_pointee(RuntimeConfig::new(config)),
            cloudflare_ranges: ArcSwap::from_pointee(cloudflare_ranges),
            rule_sets: ArcSwap::from_pointee(RuleSetStore::default()),
            local_resolvers: ArcSwap::from_pointee(LocalResolvers::default()),
            config_changed: Notify::new(),
            query_permits: Arc::new(Semaphore::new(MAX_IN_FLIGHT_QUERIES)),
            runtime_update_lock: Mutex::new(()),
            doh_clients: Mutex::new(HashMap::new()),
            local_dns_refresh_lock: tokio::sync::Mutex::new(()),
        })
    }

    pub fn is_cloudflare_ip(&self, address: IpAddr) -> bool {
        self.cloudflare_ranges
            .load()
            .iter()
            .any(|network| network.contains(&address))
    }

    pub fn replace_config(&self, config: FileConfig) {
        let _update_guard = lock_or_recover(&self.runtime_update_lock);
        let previous = self.runtime.load_full();
        self.runtime
            .store(Arc::new(RuntimeConfig::with_reloaded_config(
                config,
                previous.as_ref(),
            )));
    }

    /// Updates only the address families produced by a successful probe.
    /// If the plugin was removed or disabled while its probe was in flight,
    /// the result is intentionally discarded.
    pub fn update_preferred(
        &self,
        expected_plugin: &PluginConfig,
        selected: &PreferredIps,
    ) -> Option<PreferredIps> {
        let _update_guard = lock_or_recover(&self.runtime_update_lock);
        let current = self.runtime.load_full();
        let plugin = current.config.plugin(&expected_plugin.tag)?;
        if plugin != expected_plugin || !plugin.optimizer.enabled {
            return None;
        }

        let mut next = current.as_ref().clone();
        let preferred = next.preferred_mut(&expected_plugin.tag);
        if selected.ipv4.is_some() {
            preferred.ipv4 = selected.ipv4;
        }
        if selected.ipv6.is_some() {
            preferred.ipv6 = selected.ipv6;
        }
        let preferred = preferred.clone();
        self.runtime.store(Arc::new(next));
        Some(preferred)
    }

    pub fn doh_client(&self, layer: &LayerConfig) -> anyhow::Result<Client> {
        let endpoint = layer.doh_endpoint()?;
        let host = endpoint
            .host_str()
            .expect("DoH endpoint hostname was validated")
            .to_owned();
        let key = DohClientKey {
            endpoint: endpoint.to_string(),
            bootstrap_address: layer.address(),
            timeout_ms: layer.timeout_ms(),
        };
        let mut clients = lock_or_recover(&self.doh_clients);
        if let Some(client) = clients.get(&key) {
            return Ok(client.clone());
        }

        // The DoH bootstrap endpoint must be direct: inherited HTTP proxy
        // settings would otherwise invalidate the fixed-address guarantee.
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(layer.timeout_ms()))
            .redirect(Policy::none())
            .resolve(&host, layer.address())
            .user_agent("edgesteer/0.1")
            .build()?;
        clients.insert(key, client.clone());
        Ok(client)
    }

    pub fn clear_doh_clients(&self) {
        lock_or_recover(&self.doh_clients).clear();
    }

    pub fn replace_rule_sets(&self, rule_sets: RuleSetStore) {
        self.rule_sets.store(Arc::new(rule_sets));
    }

    pub fn local_resolvers(&self) -> Arc<LocalResolvers> {
        self.local_resolvers.load_full()
    }

    pub fn replace_local_resolvers(&self, addresses: Vec<SocketAddr>) -> Arc<LocalResolvers> {
        let resolvers = Arc::new(LocalResolvers::new(addresses));
        self.local_resolvers.store(resolvers.clone());
        resolvers
    }
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(value) => value,
        Err(poisoned) => poisoned.into_inner(),
    }
}
