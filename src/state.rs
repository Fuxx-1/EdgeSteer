use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::{Arc, Mutex},
    time::Duration,
};

use arc_swap::ArcSwap;
use ipnet::IpNet;
use reqwest::{Client, redirect::Policy};
use tokio::sync::Notify;

use crate::config::{FileConfig, PreferredConfig, UpstreamConfig};

#[derive(Debug, Hash, PartialEq, Eq)]
struct DohClientKey {
    endpoint: String,
    bootstrap_address: std::net::SocketAddr,
    timeout_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreferredIps {
    pub ipv4: Option<Ipv4Addr>,
    pub ipv6: Option<Ipv6Addr>,
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

    pub fn contains(&self, address: IpAddr) -> bool {
        match address {
            IpAddr::V4(ipv4) => self.ipv4 == Some(ipv4),
            IpAddr::V6(ipv6) => self.ipv6 == Some(ipv6),
        }
    }
}

pub struct AppState {
    pub config: ArcSwap<FileConfig>,
    pub cloudflare_ranges: ArcSwap<Vec<IpNet>>,
    pub preferred_ips: ArcSwap<PreferredIps>,
    pub config_changed: Notify,
    doh_clients: Mutex<HashMap<DohClientKey, Client>>,
}

pub type SharedState = Arc<AppState>;

impl AppState {
    pub fn new(config: FileConfig, cloudflare_ranges: Vec<IpNet>) -> SharedState {
        let preferred_ips = PreferredIps::from_config(&config.preferred);
        Arc::new(Self {
            config: ArcSwap::from_pointee(config),
            cloudflare_ranges: ArcSwap::from_pointee(cloudflare_ranges),
            preferred_ips: ArcSwap::from_pointee(preferred_ips),
            config_changed: Notify::new(),
            doh_clients: Mutex::new(HashMap::new()),
        })
    }

    pub fn is_cloudflare_ip(&self, address: IpAddr) -> bool {
        self.cloudflare_ranges
            .load()
            .iter()
            .any(|network| network.contains(&address))
    }

    pub fn replace_preferred_with_config(&self, preferred: &PreferredConfig) {
        self.preferred_ips
            .store(Arc::new(PreferredIps::from_config(preferred)));
    }

    pub fn doh_client(&self, upstream: &UpstreamConfig) -> anyhow::Result<Client> {
        let endpoint = upstream.doh_endpoint()?;
        let host = endpoint
            .host_str()
            .expect("DoH endpoint hostname was validated")
            .to_owned();
        let key = DohClientKey {
            endpoint: endpoint.to_string(),
            bootstrap_address: upstream.address,
            timeout_ms: upstream.timeout_ms,
        };
        let mut clients = match self.doh_clients.lock() {
            Ok(clients) => clients,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(client) = clients.get(&key) {
            return Ok(client.clone());
        }

        let client = Client::builder()
            .timeout(Duration::from_millis(upstream.timeout_ms))
            .redirect(Policy::none())
            .resolve(&host, upstream.address)
            .user_agent("edgesteer/0.1")
            .build()?;
        clients.insert(key, client.clone());
        Ok(client)
    }

    pub fn clear_doh_clients(&self) {
        match self.doh_clients.lock() {
            Ok(mut clients) => clients.clear(),
            Err(poisoned) => poisoned.into_inner().clear(),
        }
    }
}
