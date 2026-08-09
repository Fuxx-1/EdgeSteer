use std::{
    collections::{HashMap, HashSet},
    fs,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::Path,
    str::FromStr,
};

use anyhow::{Context, Result, bail};
use reqwest::Url;
use rustls::pki_types::ServerName;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct FileConfig {
    pub listener: ListenerConfig,
    pub cloudflare: CloudflareConfig,
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
    pub entry: String,
    pub plugins: Vec<PluginConfig>,
    pub layers: Vec<LayerConfig>,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            listener: ListenerConfig::default(),
            cloudflare: CloudflareConfig::default(),
            request_timeout_ms: default_request_timeout_ms(),
            entry: String::new(),
            plugins: Vec::new(),
            layers: Vec::new(),
        }
    }
}

impl FileConfig {
    pub fn validate(&self) -> Result<()> {
        if self.layers.is_empty() {
            bail!("at least one layers entry is required");
        }
        if self.entry.trim().is_empty() {
            bail!("entry is required");
        }
        if !self.listener.allow_remote && !self.listener.address.ip().is_loopback() {
            bail!(
                "listener.address {} is not loopback; set listener.allow_remote = true only for an intentional LAN DNS service",
                self.listener.address
            );
        }
        if self.cloudflare.range_refresh_secs == 0 {
            bail!("cloudflare.range_refresh_secs must be greater than zero");
        }
        if self.request_timeout_ms == 0 {
            bail!("request_timeout_ms must be greater than zero");
        }

        let mut plugin_tags = HashSet::new();
        for plugin in &self.plugins {
            if plugin.tag.trim().is_empty() {
                bail!("plugin tag cannot be empty");
            }
            if !plugin_tags.insert(plugin.tag.as_str()) {
                bail!("duplicate plugin tag {:?}", plugin.tag);
            }
            plugin.validate()?;
        }

        let mut layer_tags = HashSet::new();
        for layer in &self.layers {
            if layer.tag.trim().is_empty() {
                bail!("layer tag cannot be empty");
            }
            if !layer_tags.insert(layer.tag.as_str()) {
                bail!("duplicate layer tag {:?}", layer.tag);
            }
            layer.validate(self.listener.address, &plugin_tags)?;
        }
        if !layer_tags.contains(self.entry.as_str()) {
            bail!("entry layer {:?} does not exist", self.entry);
        }

        let layers_by_tag: HashMap<&str, &LayerConfig> = self
            .layers
            .iter()
            .map(|layer| (layer.tag.as_str(), layer))
            .collect();
        for layer in &self.layers {
            if let Some(fallback) = layer.fallback.as_deref() {
                if !layers_by_tag.contains_key(fallback) {
                    bail!(
                        "layer {:?} fallback {:?} does not exist",
                        layer.tag,
                        fallback
                    );
                }
            }
        }
        validate_fallback_chains(&layers_by_tag)?;
        Ok(())
    }

    pub fn layer(&self, tag: &str) -> Option<&LayerConfig> {
        self.layers.iter().find(|layer| layer.tag == tag)
    }

    pub fn plugin(&self, tag: &str) -> Option<&PluginConfig> {
        self.plugins.iter().find(|plugin| plugin.tag == tag)
    }

    /// For a single-question request, the first declared matching layer is
    /// selected directly. Otherwise the configured entry layer is used.
    pub fn select_layer(&self, domain: Option<&str>) -> &str {
        domain
            .and_then(|domain| {
                self.layers
                    .iter()
                    .find(|layer| layer.matcher.matches(domain))
                    .map(|layer| layer.tag.as_str())
            })
            .unwrap_or(&self.entry)
    }

    pub fn cloudflare_preferred_plugins(&self) -> impl Iterator<Item = &PluginConfig> {
        self.plugins
            .iter()
            .filter(|plugin| plugin.kind == PluginType::CloudflarePreferred)
    }
}

fn validate_fallback_chains(layers_by_tag: &HashMap<&str, &LayerConfig>) -> Result<()> {
    for start in layers_by_tag.keys() {
        let mut visited = HashSet::new();
        let mut current = Some(*start);
        while let Some(tag) = current {
            if !visited.insert(tag) {
                bail!("fallback cycle detected at layer {tag:?}");
            }
            current = layers_by_tag
                .get(tag)
                .and_then(|layer| layer.fallback.as_deref());
        }
    }
    Ok(())
}

const fn default_request_timeout_ms() -> u64 {
    8_000
}

pub fn load_config(path: &Path) -> Result<FileConfig> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("read configuration file {}", path.display()))?;
    let config: FileConfig = serde_json::from_str(&contents)
        .with_context(|| format!("parse JSON configuration file {}", path.display()))?;
    config.validate()?;
    Ok(config)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ListenerConfig {
    pub address: SocketAddr,
    pub allow_remote: bool,
}

impl Default for ListenerConfig {
    fn default() -> Self {
        Self {
            address: SocketAddr::from_str("127.0.0.1:53").expect("valid default listener address"),
            allow_remote: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CloudflareConfig {
    pub range_refresh_secs: u64,
}

impl Default for CloudflareConfig {
    fn default() -> Self {
        Self {
            range_refresh_secs: 24 * 60 * 60,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PluginConfig {
    pub tag: String,
    #[serde(rename = "type")]
    pub kind: PluginType,
    #[serde(default = "default_rewrite_ttl_secs")]
    pub rewrite_ttl_secs: u32,
    #[serde(default)]
    pub preferred: PreferredConfig,
    #[serde(default)]
    pub optimizer: OptimizerConfig,
}

impl PluginConfig {
    fn validate(&self) -> Result<()> {
        if self.rewrite_ttl_secs == 0 {
            bail!(
                "plugin {:?} rewrite_ttl_secs must be greater than zero",
                self.tag
            );
        }
        self.optimizer.validate()
    }
}

const fn default_rewrite_ttl_secs() -> u32 {
    60
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    CloudflarePreferred,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct PreferredConfig {
    pub ipv4: Option<Ipv4Addr>,
    pub ipv6: Option<Ipv6Addr>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LayerConfig {
    pub tag: String,
    #[serde(rename = "type")]
    pub kind: LayerType,
    #[serde(default)]
    pub fallback: Option<String>,
    #[serde(default, rename = "match")]
    pub matcher: KeywordMatch,
    /// UDP/TCP/DoT endpoint, or the fixed bootstrap address for a DoH URL.
    #[serde(default)]
    pub address: Option<SocketAddr>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Required for a DoH layer. Its hostname provides TLS SNI and Host.
    #[serde(default)]
    pub url: Option<String>,
    /// Required for a DoT layer so certificate verification has an SNI name.
    #[serde(default)]
    pub server_name: Option<String>,
    /// Required for an interceptor layer.
    #[serde(default)]
    pub plugin: Option<String>,
}

impl LayerConfig {
    pub fn address(&self) -> SocketAddr {
        self.address.expect("validated network layer address")
    }

    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms.unwrap_or_else(default_layer_timeout_ms)
    }

    fn validate(&self, listener: SocketAddr, plugin_tags: &HashSet<&str>) -> Result<()> {
        self.matcher.validate(&self.tag)?;
        match self.kind {
            LayerType::Udp | LayerType::Tcp => {
                self.validate_network(listener)?;
                if self.url.is_some() || self.server_name.is_some() || self.plugin.is_some() {
                    bail!(
                        "layer {:?} is {:?}; url, server_name, and plugin are not valid",
                        self.tag,
                        self.kind
                    );
                }
            }
            LayerType::Dot => {
                self.validate_network(listener)?;
                if self.url.is_some() || self.plugin.is_some() {
                    bail!("DoT layer {:?} must not set url or plugin", self.tag);
                }
                let server_name = self
                    .server_name
                    .as_ref()
                    .context("DoT layer requires server_name for TLS certificate verification")?;
                ServerName::try_from(server_name.clone())
                    .with_context(|| format!("invalid DoT server_name {server_name:?}"))?;
            }
            LayerType::Doh => {
                self.validate_network(listener)?;
                if self.server_name.is_some() || self.plugin.is_some() {
                    bail!(
                        "DoH layer {:?} derives SNI from url and must not set server_name or plugin",
                        self.tag
                    );
                }
                let endpoint = self.doh_endpoint()?;
                let endpoint_port = endpoint
                    .port_or_known_default()
                    .context("DoH URL has no usable HTTPS port")?;
                if endpoint_port != self.address().port() {
                    bail!(
                        "DoH URL port {endpoint_port} must match bootstrap address {}",
                        self.address()
                    );
                }
            }
            LayerType::Interceptor => {
                if self.fallback.as_deref().is_none_or(str::is_empty) {
                    bail!("interceptor layer {:?} requires fallback", self.tag);
                }
                let plugin = self
                    .plugin
                    .as_deref()
                    .context("interceptor layer requires plugin")?;
                if !plugin_tags.contains(plugin) {
                    bail!(
                        "interceptor layer {:?} references unknown plugin {plugin:?}",
                        self.tag
                    );
                }
                if self.address.is_some()
                    || self.timeout_ms.is_some()
                    || self.url.is_some()
                    || self.server_name.is_some()
                {
                    bail!(
                        "interceptor layer {:?} only accepts plugin, fallback, and match",
                        self.tag
                    );
                }
            }
        }
        Ok(())
    }

    fn validate_network(&self, listener: SocketAddr) -> Result<()> {
        let address = self.address.context("network layer requires address")?;
        if socket_addresses_overlap(listener, address) {
            bail!(
                "layer {:?} address {address} overlaps listener.address {listener} and would create a DNS forwarding loop",
                self.tag
            );
        }
        if address.port() == 0 {
            bail!("layer {:?} address has port zero", self.tag);
        }
        if self.timeout_ms() == 0 {
            bail!("layer {:?} has a zero timeout", self.tag);
        }
        Ok(())
    }

    pub fn doh_endpoint(&self) -> Result<Url> {
        let raw_url = self.url.as_deref().context("DoH layer requires url")?;
        let endpoint = Url::parse(raw_url).with_context(|| format!("parse DoH URL {raw_url:?}"))?;
        if endpoint.scheme() != "https" {
            bail!("DoH URL must use https: {raw_url:?}");
        }
        if endpoint.host_str().is_none() {
            bail!("DoH URL must include a hostname: {raw_url:?}");
        }
        if !endpoint.username().is_empty() || endpoint.password().is_some() {
            bail!("DoH URL must not contain credentials");
        }
        if endpoint.fragment().is_some() {
            bail!("DoH URL must not contain a fragment");
        }
        Ok(endpoint)
    }
}

fn socket_addresses_overlap(listener: SocketAddr, endpoint: SocketAddr) -> bool {
    if listener.port() != endpoint.port() {
        return false;
    }
    match (listener.ip(), endpoint.ip()) {
        (IpAddr::V4(listener), IpAddr::V4(endpoint)) => {
            listener == endpoint || listener.is_unspecified() || endpoint.is_unspecified()
        }
        (IpAddr::V6(listener), IpAddr::V6(endpoint)) => {
            listener == endpoint || listener.is_unspecified() || endpoint.is_unspecified()
        }
        _ => listener.ip().is_unspecified() || endpoint.ip().is_unspecified(),
    }
}

const fn default_layer_timeout_ms() -> u64 {
    3_000
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LayerType {
    Udp,
    Tcp,
    Doh,
    Dot,
    Interceptor,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct KeywordMatch {
    #[serde(default)]
    pub mode: KeywordMatchMode,
    pub keywords: Vec<String>,
}

impl KeywordMatch {
    fn validate(&self, layer_tag: &str) -> Result<()> {
        if self
            .keywords
            .iter()
            .any(|keyword| keyword.trim().is_empty())
        {
            bail!("layer {layer_tag:?} match.keywords cannot contain an empty value");
        }
        if self.mode == KeywordMatchMode::Label
            && self.keywords.iter().any(|keyword| keyword.contains('.'))
        {
            bail!("layer {layer_tag:?} label keywords cannot contain a dot");
        }
        Ok(())
    }

    pub fn matches(&self, domain: &str) -> bool {
        let domain = domain.trim_end_matches('.').to_ascii_lowercase();
        match self.mode {
            KeywordMatchMode::Label => self.keywords.iter().any(|keyword| {
                domain
                    .split('.')
                    .any(|label| label.eq_ignore_ascii_case(keyword.trim()))
            }),
            KeywordMatchMode::Contains => self
                .keywords
                .iter()
                .any(|keyword| domain.contains(&keyword.trim().to_ascii_lowercase())),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeywordMatchMode {
    #[default]
    Label,
    Contains,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct OptimizerConfig {
    pub enabled: bool,
    pub interval_secs: u64,
    pub test_host: String,
    pub test_path: String,
    pub test_port: u16,
    pub timeout_ms: u64,
    pub concurrency: usize,
    pub samples_per_cidr: usize,
    pub max_candidates: usize,
    pub candidates: Vec<String>,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 6 * 60 * 60,
            test_host: "www.cloudflare.com".to_owned(),
            test_path: "/cdn-cgi/trace".to_owned(),
            test_port: 443,
            timeout_ms: 3_000,
            concurrency: 32,
            samples_per_cidr: 1,
            max_candidates: 128,
            candidates: Vec::new(),
        }
    }
}

impl OptimizerConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.candidates.is_empty() {
            bail!("optimizer.candidates cannot be empty when optimizer.enabled = true");
        }
        if self.interval_secs == 0 || self.timeout_ms == 0 {
            bail!("optimizer.interval_secs and optimizer.timeout_ms must be greater than zero");
        }
        if self.concurrency == 0 || self.samples_per_cidr == 0 || self.max_candidates == 0 {
            bail!(
                "optimizer.concurrency, optimizer.samples_per_cidr, and optimizer.max_candidates must be greater than zero"
            );
        }
        if self.test_host.trim().is_empty()
            || !self.test_path.starts_with('/')
            || self.test_port == 0
        {
            bail!("optimizer test_host, test_path, or test_port is invalid");
        }
        for candidate in &self.candidates {
            if candidate.parse::<IpAddr>().is_err() && candidate.parse::<ipnet::IpNet>().is_err() {
                bail!("invalid optimizer candidate {candidate:?}; use an IP address or CIDR");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
        {
          "listener": { "address": "127.0.0.1:53535" },
          "entry": "preferred",
          "plugins": [{
            "tag": "cloudflare-preferred",
            "type": "cloudflare_preferred"
          }],
          "layers": [
            {
              "tag": "preferred",
              "type": "interceptor",
              "plugin": "cloudflare-preferred",
              "fallback": "cf"
            },
            {
              "tag": "cf",
              "type": "doh",
              "address": "1.1.1.1:443",
              "url": "https://cloudflare-dns.com/dns-query",
              "fallback": "tencent"
            },
            {
              "tag": "tencent",
              "type": "doh",
              "address": "120.53.53.53:443",
              "url": "https://doh.pub/dns-query",
              "fallback": "local",
              "match": { "keywords": ["cn"] }
            },
            {
              "tag": "local",
              "type": "udp",
              "address": "127.0.0.1:53",
              "match": { "keywords": ["local", "lan"] }
            }
          ]
        }
    "#;

    #[test]
    fn parses_the_json_layer_chain() {
        let config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        config.validate().expect("configuration validates");
        assert_eq!(config.select_layer(Some("printer.local.")), "local");
        assert_eq!(config.select_layer(Some("www.example.cn.")), "tencent");
        assert_eq!(config.select_layer(Some("notlocal.example.")), "preferred");
        assert_eq!(
            config.select_layer(Some("www.cloudflare.com.")),
            "preferred"
        );
    }

    #[test]
    fn rejects_a_fallback_cycle() {
        let mut config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        config.layers[3].fallback = Some("preferred".to_owned());
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_doh_with_a_mismatched_bootstrap_port() {
        let mut config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        config.layers[1].address = Some("1.1.1.1:8443".parse().unwrap());
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_an_empty_keyword() {
        let mut config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        config.layers[3].matcher.keywords.push(" ".to_owned());
        assert!(config.validate().is_err());
    }

    #[test]
    fn supports_explicit_literal_contains_matching() {
        let mut config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        config.layers[2].matcher.mode = KeywordMatchMode::Contains;
        config.layers[2].matcher.keywords = vec!["video".to_owned()];

        assert_eq!(config.select_layer(Some("my-video.example.")), "tencent");
    }
}
