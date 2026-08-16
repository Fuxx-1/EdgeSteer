use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use arc_swap::ArcSwap;
use ipnet::IpNet;
use reqwest::{Client, redirect::Policy};
use tokio::sync::{Notify, Semaphore};

use crate::{
    config::{FileConfig, LayerConfig, PluginConfig, PreferredConfig},
    optimizer,
    rule_sets::RuleSetStore,
};

pub const MAX_IN_FLIGHT_QUERIES: usize = 128;
const MAX_IN_FLIGHT_COMPATIBILITY_PROBES: usize = 16;
const MAX_COMPATIBILITY_CACHE_ENTRIES: usize = 4_096;

#[derive(Debug, Hash, PartialEq, Eq)]
struct DohClientKey {
    endpoint: String,
    bootstrap_address: SocketAddr,
    timeout_ms: u64,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct CompatibilityProbeKey {
    plugin_tag: String,
    host: String,
    address: IpAddr,
}

#[derive(Debug, Clone, Copy)]
enum CompatibilityProbeStatus {
    Pending,
    VerifiedUntil(Instant),
    RejectedUntil(Instant),
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
            let unchanged_plugin = previous
                .config
                .plugin(&plugin.tag)
                .is_some_and(|previous_plugin| previous_plugin == plugin);
            let preferred = if unchanged_plugin {
                previous
                    .preferred(&plugin.tag)
                    .cloned()
                    .unwrap_or_else(|| preferred_from_plugin(plugin))
            } else {
                preferred_from_plugin(plugin)
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
        .map(|plugin| (plugin.tag.clone(), preferred_from_plugin(plugin)))
        .collect()
}

fn preferred_from_plugin(plugin: &PluginConfig) -> PreferredIps {
    if plugin.optimizer.requires_compatibility_gate() {
        PreferredIps::default()
    } else {
        PreferredIps::from_config(&plugin.preferred)
    }
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
    compatibility_probe_cache: Arc<Mutex<HashMap<CompatibilityProbeKey, CompatibilityProbeStatus>>>,
    compatibility_probe_permits: Arc<Semaphore>,
    compatibility_probe_epoch: Arc<AtomicU64>,
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
            compatibility_probe_cache: Arc::new(Mutex::new(HashMap::new())),
            compatibility_probe_permits: Arc::new(Semaphore::new(
                MAX_IN_FLIGHT_COMPATIBILITY_PROBES,
            )),
            compatibility_probe_epoch: Arc::new(AtomicU64::new(0)),
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
        self.clear_compatibility_probes();
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
        if plugin.optimizer.requires_compatibility_gate() {
            *preferred = selected.clone();
        } else {
            if selected.ipv4.is_some() {
                preferred.ipv4 = selected.ipv4;
            }
            if selected.ipv6.is_some() {
                preferred.ipv6 = selected.ipv6;
            }
        }
        let preferred = preferred.clone();
        self.runtime.store(Arc::new(next));
        self.clear_compatibility_probes();
        Some(preferred)
    }

    /// Strict compatibility mode must never keep an old address after a
    /// complete failed round. Returning no rewrite is safer than serving an
    /// address that is no longer known to be compatible with the configured
    /// Cloudflare zones.
    pub fn clear_preferred(&self, expected_plugin: &PluginConfig) -> Option<PreferredIps> {
        let _update_guard = lock_or_recover(&self.runtime_update_lock);
        let current = self.runtime.load_full();
        let plugin = current.config.plugin(&expected_plugin.tag)?;
        if plugin != expected_plugin
            || !plugin.optimizer.enabled
            || !plugin.optimizer.requires_compatibility_gate()
        {
            return None;
        }

        let mut next = current.as_ref().clone();
        let preferred = next.preferred_mut(&expected_plugin.tag);
        if preferred.is_empty() {
            self.clear_compatibility_probes();
            return Some(preferred.clone());
        }
        *preferred = PreferredIps::default();
        self.runtime.store(Arc::new(next));
        self.clear_compatibility_probes();
        Some(PreferredIps::default())
    }

    /// For strict plugins, an optimized address is handed to a client only
    /// after the requested hostname itself has passed a fresh SNI/Host probe.
    /// Cache lifetime is capped by the rewritten DNS TTL, so a cached proof
    /// cannot outlive the answer that relies on it.
    pub fn compatible_preferred(
        &self,
        plugin: &PluginConfig,
        domain: Option<&str>,
        preferred: &PreferredIps,
    ) -> PreferredIps {
        if !plugin.optimizer.requires_compatibility_gate() {
            return preferred.clone();
        }
        let Some(host) = domain
            .map(|domain| domain.trim().trim_end_matches('.').to_ascii_lowercase())
            .filter(|domain| !domain.is_empty())
        else {
            return PreferredIps::default();
        };

        PreferredIps {
            ipv4: preferred.ipv4.filter(|address| {
                self.compatibility_is_verified_or_scheduled(plugin, &host, IpAddr::V4(*address))
            }),
            ipv6: preferred.ipv6.filter(|address| {
                self.compatibility_is_verified_or_scheduled(plugin, &host, IpAddr::V6(*address))
            }),
        }
    }

    fn compatibility_is_verified_or_scheduled(
        &self,
        plugin: &PluginConfig,
        host: &str,
        address: IpAddr,
    ) -> bool {
        let host = host.to_owned();
        let key = CompatibilityProbeKey {
            plugin_tag: plugin.tag.clone(),
            host: host.clone(),
            address,
        };
        let now = Instant::now();
        let mut cache = lock_or_recover(&self.compatibility_probe_cache);
        match cache.get(&key).copied() {
            Some(CompatibilityProbeStatus::VerifiedUntil(expires_at)) if expires_at > now => {
                return true;
            }
            Some(CompatibilityProbeStatus::Pending) => {
                return false;
            }
            Some(CompatibilityProbeStatus::RejectedUntil(expires_at)) if expires_at > now => {
                return false;
            }
            Some(_) => {
                cache.remove(&key);
            }
            None => {}
        }

        cache.retain(|_, status| match status {
            CompatibilityProbeStatus::Pending => true,
            CompatibilityProbeStatus::VerifiedUntil(expires_at)
            | CompatibilityProbeStatus::RejectedUntil(expires_at) => *expires_at > now,
        });
        if cache.len() >= MAX_COMPATIBILITY_CACHE_ENTRIES {
            return false;
        }
        let Ok(permit) = self.compatibility_probe_permits.clone().try_acquire_owned() else {
            return false;
        };
        cache.insert(key.clone(), CompatibilityProbeStatus::Pending);
        drop(cache);

        let cache = self.compatibility_probe_cache.clone();
        let settings = plugin.optimizer.clone();
        let valid_for = Duration::from_secs(u64::from(plugin.rewrite_ttl_secs));
        let probe_epoch = self.compatibility_probe_epoch.load(Ordering::Acquire);
        let current_epoch = self.compatibility_probe_epoch.clone();
        tokio::spawn(async move {
            let verified = optimizer::verify_compatibility(address, &host, &settings)
                .await
                .is_ok();
            let now = Instant::now();
            let status = if verified {
                CompatibilityProbeStatus::VerifiedUntil(now + valid_for)
            } else {
                CompatibilityProbeStatus::RejectedUntil(
                    now + valid_for.min(Duration::from_secs(30)),
                )
            };
            if current_epoch.load(Ordering::Acquire) == probe_epoch {
                lock_or_recover(&cache).insert(key, status);
            }
            drop(permit);
        });
        false
    }

    fn clear_compatibility_probes(&self) {
        self.compatibility_probe_epoch
            .fetch_add(1, Ordering::AcqRel);
        lock_or_recover(&self.compatibility_probe_cache).clear();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::OptimizerConfig, config::PluginType};

    fn strict_plugin(hosts: &[&str]) -> PluginConfig {
        PluginConfig {
            tag: "cloudflare-preferred".to_owned(),
            kind: PluginType::CloudflarePreferred,
            rewrite_ttl_secs: 60,
            preferred: PreferredConfig {
                ipv4: Some(Ipv4Addr::new(172, 67, 231, 26)),
                ipv6: None,
            },
            optimizer: OptimizerConfig {
                enabled: true,
                compatibility_hosts: hosts.iter().map(|host| (*host).to_owned()).collect(),
                candidates: vec!["172.67.0.0/16".to_owned()],
                ..OptimizerConfig::default()
            },
        }
    }

    fn config(plugin: PluginConfig) -> FileConfig {
        FileConfig {
            plugins: vec![plugin],
            ..FileConfig::default()
        }
    }

    #[test]
    fn strict_optimizer_ignores_static_preferred_addresses() {
        let plugin = strict_plugin(&["blog.qoop.top"]);
        let runtime = RuntimeConfig::new(config(plugin));

        assert!(
            runtime
                .preferred("cloudflare-preferred")
                .is_some_and(PreferredIps::is_empty)
        );
    }

    #[test]
    fn compatibility_policy_change_drops_dynamic_preferred_addresses() {
        let plugin = strict_plugin(&["blog.qoop.top"]);
        let mut previous = RuntimeConfig::new(config(plugin.clone()));
        previous.preferred_ips.insert(
            plugin.tag.clone(),
            PreferredIps {
                ipv4: Some(Ipv4Addr::new(172, 67, 231, 26)),
                ipv6: None,
            },
        );
        let replacement = strict_plugin(&["another-zone.example"]);

        let reloaded = RuntimeConfig::with_reloaded_config(config(replacement), &previous);

        assert!(
            reloaded
                .preferred("cloudflare-preferred")
                .is_some_and(PreferredIps::is_empty)
        );
    }

    #[test]
    fn strict_optimizer_clears_old_families_after_a_failed_round() {
        let plugin = strict_plugin(&["blog.qoop.top"]);
        let state = AppState::new(config(plugin.clone()), Vec::new());
        let selected = PreferredIps {
            ipv4: Some(Ipv4Addr::new(172, 67, 231, 26)),
            ipv6: Some(Ipv6Addr::LOCALHOST),
        };

        state
            .update_preferred(&plugin, &selected)
            .expect("strict probe selection is accepted");
        state
            .clear_preferred(&plugin)
            .expect("strict failed round clears the selection");

        assert!(
            state
                .runtime
                .load()
                .preferred("cloudflare-preferred")
                .is_some_and(PreferredIps::is_empty)
        );
    }
}
