use std::{
    collections::{HashMap, HashSet},
    fs,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::{Path, PathBuf},
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
    #[serde(default)]
    pub rule_sets: Vec<RuleSetConfig>,
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
            rule_sets: Vec::new(),
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

        let mut rule_set_tags = HashSet::new();
        for rule_set in &self.rule_sets {
            if rule_set.tag.trim().is_empty() {
                bail!("rule set tag cannot be empty");
            }
            if !rule_set_tags.insert(rule_set.tag.as_str()) {
                bail!("duplicate rule set tag {:?}", rule_set.tag);
            }
            rule_set.validate()?;
        }

        let mut layer_tags = HashSet::new();
        for layer in &self.layers {
            if layer.tag.trim().is_empty() {
                bail!("layer tag cannot be empty");
            }
            if !layer_tags.insert(layer.tag.as_str()) {
                bail!("duplicate layer tag {:?}", layer.tag);
            }
            layer.validate(self.listener.address, &plugin_tags, &rule_set_tags)?;
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

    pub fn rule_set(&self, tag: &str) -> Option<&RuleSetConfig> {
        self.rule_sets.iter().find(|rule_set| rule_set.tag == tag)
    }

    /// For a single-question request, the first declared matching layer is
    /// selected directly. Otherwise the configured entry layer is used.
    pub fn select_layer(&self, domain: Option<&str>) -> &str {
        self.select_layer_with_rule_sets(domain, |_, _| false)
    }

    /// For a single-question request, the first declared layer whose keyword
    /// or loaded rule-set matcher succeeds is selected directly. Otherwise
    /// the configured entry layer is used.
    pub fn select_layer_with_rule_sets<F>(&self, domain: Option<&str>, rule_set_matches: F) -> &str
    where
        F: Fn(&str, &str) -> bool,
    {
        domain
            .and_then(|domain| {
                self.layers
                    .iter()
                    .find(|layer| {
                        layer
                            .matcher
                            .matches_with_rule_sets(domain, &rule_set_matches)
                    })
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
    parse_config_text(&contents)
        .with_context(|| format!("parse JSON configuration file {}", path.display()))
}

/// Parse and validate a JSON configuration without reading a path. This is
/// shared by the native UI so it has the exact same validation boundary as
/// the DNS service.
pub fn parse_config_text(contents: &str) -> Result<FileConfig> {
    let config: FileConfig = serde_json::from_str(contents).context("parse JSON configuration")?;
    config.validate()?;
    Ok(config)
}

/// Replace a configuration file only after it passes the normal parser and
/// validator. Writing a sibling file and renaming it keeps file-watch reloads
/// from observing a partially written JSON document.
pub fn write_config_atomically(path: &Path, contents: &str) -> Result<()> {
    parse_config_text(contents).context("validate configuration before saving")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        bail!(
            "configuration directory {} does not exist",
            parent.display()
        );
    }
    let file_name = path
        .file_name()
        .context("configuration path has no file name")?;
    let mut temporary_name = file_name.to_os_string();
    temporary_name.push(format!(".{}.tmp", std::process::id()));
    let temporary_path = parent.join(temporary_name);

    fs::write(&temporary_path, contents)
        .with_context(|| format!("write temporary configuration {}", temporary_path.display()))?;
    if let Err(error) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error).with_context(|| {
            format!(
                "replace configuration {} with {}",
                path.display(),
                temporary_path.display()
            )
        });
    }
    Ok(())
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
pub struct RuleSetConfig {
    pub tag: String,
    #[serde(rename = "type")]
    pub kind: RuleSetType,
    /// Local `.srs` source path.
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// HTTPS source URL for a remote `.srs` rule set.
    #[serde(default)]
    pub url: Option<String>,
    /// Refresh period. The default is 24 hours for remote sources and 60
    /// seconds for local sources.
    #[serde(default)]
    pub update_interval_secs: Option<u64>,
    /// Per-download deadline for remote sources.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl RuleSetConfig {
    fn validate(&self) -> Result<()> {
        if self.update_interval_secs.is_some_and(|value| value == 0) {
            bail!(
                "rule set {:?} update_interval_secs must be greater than zero",
                self.tag
            );
        }
        match self.kind {
            RuleSetType::Local => {
                if self.url.is_some() || self.timeout_ms.is_some() {
                    bail!(
                        "local rule set {:?} only accepts path and update_interval_secs",
                        self.tag
                    );
                }
                if self
                    .path
                    .as_ref()
                    .is_none_or(|path| path.as_os_str().is_empty())
                {
                    bail!("local rule set {:?} requires a non-empty path", self.tag);
                }
            }
            RuleSetType::Remote => {
                if self.path.is_some() {
                    bail!(
                        "remote rule set {:?} only accepts url, update_interval_secs, and timeout_ms",
                        self.tag
                    );
                }
                self.endpoint()?;
                if self.timeout_ms() == 0 {
                    bail!("remote rule set {:?} has a zero timeout", self.tag);
                }
            }
        }
        Ok(())
    }

    pub fn update_interval_secs(&self) -> u64 {
        self.update_interval_secs.unwrap_or(match self.kind {
            RuleSetType::Local => 60,
            RuleSetType::Remote => 24 * 60 * 60,
        })
    }

    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms.unwrap_or(10_000)
    }

    pub fn local_path(&self) -> &Path {
        self.path.as_deref().expect("validated local rule set path")
    }

    pub fn endpoint(&self) -> Result<Url> {
        let raw_url = self
            .url
            .as_deref()
            .context("remote rule set requires url")?;
        let endpoint =
            Url::parse(raw_url).with_context(|| format!("parse rule set URL {raw_url:?}"))?;
        if endpoint.scheme() != "https" {
            bail!("rule set URL must use https: {raw_url:?}");
        }
        if endpoint.host_str().is_none() {
            bail!("rule set URL must include a hostname: {raw_url:?}");
        }
        if !endpoint.username().is_empty() || endpoint.password().is_some() {
            bail!("rule set URL must not contain credentials");
        }
        if endpoint.fragment().is_some() {
            bail!("rule set URL must not contain a fragment");
        }
        Ok(endpoint)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleSetType {
    Local,
    Remote,
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
    /// Refresh interval for a dynamically discovered local resolver layer.
    #[serde(default)]
    pub refresh_secs: Option<u64>,
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

    pub fn refresh_secs(&self) -> u64 {
        self.refresh_secs.unwrap_or_else(default_local_refresh_secs)
    }

    fn validate(
        &self,
        listener: SocketAddr,
        plugin_tags: &HashSet<&str>,
        rule_set_tags: &HashSet<&str>,
    ) -> Result<()> {
        self.matcher.validate(&self.tag, rule_set_tags)?;
        match self.kind {
            LayerType::Udp | LayerType::Tcp => {
                self.validate_network(listener)?;
                if self.refresh_secs.is_some()
                    || self.url.is_some()
                    || self.server_name.is_some()
                    || self.plugin.is_some()
                {
                    bail!(
                        "layer {:?} is {:?}; refresh_secs, url, server_name, and plugin are not valid",
                        self.tag,
                        self.kind
                    );
                }
            }
            LayerType::Dot => {
                self.validate_network(listener)?;
                if self.refresh_secs.is_some() || self.url.is_some() || self.plugin.is_some() {
                    bail!(
                        "DoT layer {:?} must not set refresh_secs, url, or plugin",
                        self.tag
                    );
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
                if self.refresh_secs.is_some()
                    || self.server_name.is_some()
                    || self.plugin.is_some()
                {
                    bail!(
                        "DoH layer {:?} derives SNI from url and must not set refresh_secs, server_name, or plugin",
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
                    || self.refresh_secs.is_some()
                    || self.url.is_some()
                    || self.server_name.is_some()
                {
                    bail!(
                        "interceptor layer {:?} only accepts plugin, fallback, and match",
                        self.tag
                    );
                }
            }
            LayerType::Local => {
                if self.address.is_some()
                    || self.url.is_some()
                    || self.server_name.is_some()
                    || self.plugin.is_some()
                {
                    bail!(
                        "local layer {:?} only accepts timeout_ms, refresh_secs, fallback, and match",
                        self.tag
                    );
                }
                if self.timeout_ms() == 0 {
                    bail!("local layer {:?} has a zero timeout", self.tag);
                }
                if self.refresh_secs() == 0 {
                    bail!(
                        "local layer {:?} refresh_secs must be greater than zero",
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

const fn default_local_refresh_secs() -> u64 {
    30
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LayerType {
    Udp,
    Tcp,
    Doh,
    Dot,
    Local,
    Interceptor,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct KeywordMatch {
    #[serde(default)]
    pub mode: KeywordMatchMode,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// sing-box SRS rule-set tags. A matching rule set and a matching keyword
    /// are alternatives, so either can select this layer.
    #[serde(default)]
    pub rule_sets: Vec<String>,
}

impl KeywordMatch {
    fn validate(&self, layer_tag: &str, rule_set_tags: &HashSet<&str>) -> Result<()> {
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
        if self
            .rule_sets
            .iter()
            .any(|rule_set| rule_set.trim().is_empty())
        {
            bail!("layer {layer_tag:?} match.rule_sets cannot contain an empty value");
        }
        let mut referenced = HashSet::new();
        for rule_set in &self.rule_sets {
            if !rule_set_tags.contains(rule_set.as_str()) {
                bail!("layer {layer_tag:?} match references unknown rule set {rule_set:?}");
            }
            if !referenced.insert(rule_set.as_str()) {
                bail!("layer {layer_tag:?} match references rule set {rule_set:?} more than once");
            }
        }
        Ok(())
    }

    pub fn matches(&self, domain: &str) -> bool {
        self.matches_with_rule_sets(domain, &|_, _| false)
    }

    pub fn matches_with_rule_sets(
        &self,
        domain: &str,
        rule_set_matches: &impl Fn(&str, &str) -> bool,
    ) -> bool {
        let domain = domain.trim_end_matches('.').to_ascii_lowercase();
        let keyword_matches = match self.mode {
            KeywordMatchMode::Label => self.keywords.iter().any(|keyword| {
                domain
                    .split('.')
                    .any(|label| label.eq_ignore_ascii_case(keyword.trim()))
            }),
            KeywordMatchMode::Contains => self
                .keywords
                .iter()
                .any(|keyword| domain.contains(&keyword.trim().to_ascii_lowercase())),
        };
        keyword_matches
            || self
                .rule_sets
                .iter()
                .any(|rule_set| rule_set_matches(rule_set, &domain))
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
    pub probes_per_candidate: usize,
    pub compatibility_hosts: Vec<String>,
    /// Candidate IPs/CIDRs that must never be selected. This is useful for
    /// known EIV-restricted Cloudflare subnets when a wider CIDR is sampled.
    pub excluded_candidates: Vec<String>,
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
            probes_per_candidate: 3,
            compatibility_hosts: Vec::new(),
            excluded_candidates: Vec::new(),
            max_candidates: 128,
            candidates: Vec::new(),
        }
    }
}

impl OptimizerConfig {
    /// A compatibility host makes rewriting opt-in: an address must be
    /// actively proven compatible before EdgeSteer is allowed to hand it out.
    pub fn requires_compatibility_gate(&self) -> bool {
        self.enabled && !self.compatibility_hosts.is_empty()
    }

    pub fn excluded_networks(&self) -> Result<Vec<ipnet::IpNet>> {
        self.excluded_candidates
            .iter()
            .map(|candidate| {
                if let Ok(address) = candidate.parse::<IpAddr>() {
                    Ok(ipnet::IpNet::from(address))
                } else {
                    candidate.parse::<ipnet::IpNet>().with_context(|| {
                        format!(
                            "invalid optimizer excluded candidate {candidate:?}; use an IP address or CIDR"
                        )
                    })
                }
            })
            .collect()
    }

    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.candidates.is_empty() {
            bail!("optimizer.candidates cannot be empty when optimizer.enabled = true");
        }
        if self.compatibility_hosts.is_empty() {
            bail!(
                "optimizer.compatibility_hosts cannot be empty when optimizer.enabled = true; strict host validation is required to prevent Cloudflare 1034"
            );
        }
        if self.interval_secs == 0 || self.timeout_ms == 0 {
            bail!("optimizer.interval_secs and optimizer.timeout_ms must be greater than zero");
        }
        if self.concurrency == 0
            || self.samples_per_cidr == 0
            || self.probes_per_candidate == 0
            || self.max_candidates == 0
        {
            bail!(
                "optimizer.concurrency, optimizer.samples_per_cidr, optimizer.probes_per_candidate, and optimizer.max_candidates must be greater than zero"
            );
        }
        if self.test_host.trim().is_empty()
            || !self.test_path.starts_with('/')
            || self.test_port == 0
        {
            bail!("optimizer test_host, test_path, or test_port is invalid");
        }
        let mut compatibility_hosts = HashSet::new();
        for host in &self.compatibility_hosts {
            let normalized = host.trim().trim_end_matches('.');
            if normalized.is_empty() || normalized.parse::<IpAddr>().is_ok() {
                bail!(
                    "invalid optimizer compatibility host {host:?}; use a DNS hostname without whitespace or a trailing dot"
                );
            }
            if host != normalized {
                bail!(
                    "invalid optimizer compatibility host {host:?}; remove whitespace and the trailing dot"
                );
            }
            ServerName::try_from(normalized.to_owned())
                .with_context(|| format!("invalid optimizer compatibility host {normalized:?}"))?;
            if !compatibility_hosts.insert(normalized.to_ascii_lowercase()) {
                bail!("duplicate optimizer compatibility host {normalized:?}");
            }
        }
        validate_optimizer_candidates(&self.candidates, "candidate")?;
        self.excluded_networks()?;
        Ok(())
    }
}

fn validate_optimizer_candidates(candidates: &[String], label: &str) -> Result<()> {
    for candidate in candidates {
        if candidate.parse::<IpAddr>().is_err() && candidate.parse::<ipnet::IpNet>().is_err() {
            bail!("invalid optimizer {label} {candidate:?}; use an IP address or CIDR");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
        {
          "listener": { "address": "127.0.0.1:53535" },
          "entry": "preferred",
          "rule_sets": [
            {
              "tag": "cn",
              "type": "remote",
              "url": "https://example.com/geosite-cn.srs"
            },
            {
              "tag": "overseas",
              "type": "remote",
              "url": "https://example.com/geosite-geolocation-not-cn.srs"
            }
          ],
          "plugins": [{
            "tag": "cloudflare-preferred",
            "type": "cloudflare_preferred"
          }],
          "layers": [
            {
              "tag": "local-keyword",
              "type": "local",
              "refresh_secs": 30,
              "match": {
                "mode": "contains",
                "keywords": ["b2c", "mi", "local"]
              }
            },
            {
              "tag": "cn-preferred",
              "type": "interceptor",
              "plugin": "cloudflare-preferred",
              "fallback": "tencent",
              "match": { "rule_sets": ["cn"] }
            },
            {
              "tag": "overseas-preferred",
              "type": "interceptor",
              "plugin": "cloudflare-preferred",
              "fallback": "cf",
              "match": { "rule_sets": ["overseas"] }
            },
            {
              "tag": "preferred",
              "type": "interceptor",
              "plugin": "cloudflare-preferred",
              "fallback": "tencent"
            },
            {
              "tag": "tencent",
              "type": "doh",
              "address": "120.53.53.53:443",
              "url": "https://doh.pub/dns-query",
              "fallback": "cf"
            },
            {
              "tag": "cf",
              "type": "doh",
              "address": "1.1.1.1:443",
              "url": "https://cloudflare-dns.com/dns-query",
              "fallback": "local-fallback"
            },
            {
              "tag": "local-fallback",
              "type": "local",
              "refresh_secs": 30
            }
          ]
        }
    "#;

    fn layer_mut<'config>(config: &'config mut FileConfig, tag: &str) -> &'config mut LayerConfig {
        config
            .layers
            .iter_mut()
            .find(|layer| layer.tag == tag)
            .expect("fixture layer exists")
    }

    #[test]
    fn parses_the_json_layer_chain() {
        let config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        config.validate().expect("configuration validates");
        assert_eq!(
            config.select_layer(Some("printer.b2c.example.")),
            "local-keyword"
        );
        assert_eq!(
            config.select_layer_with_rule_sets(Some("www.example.cn."), |tag, domain| {
                tag == "cn" && domain == "www.example.cn"
            }),
            "cn-preferred"
        );
        assert_eq!(
            config.select_layer_with_rule_sets(Some("www.example.com."), |tag, domain| {
                tag == "overseas" && domain == "www.example.com"
            }),
            "overseas-preferred"
        );
        assert_eq!(config.select_layer(Some("www.example.test.")), "preferred");
        assert_eq!(
            config.select_layer(Some("www.cloudflare.com.")),
            "preferred"
        );
    }

    #[test]
    fn rejects_a_fallback_cycle() {
        let mut config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        layer_mut(&mut config, "local-fallback").fallback = Some("preferred".to_owned());
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_doh_with_a_mismatched_bootstrap_port() {
        let mut config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        layer_mut(&mut config, "cf").address = Some("1.1.1.1:8443".parse().unwrap());
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_an_empty_keyword() {
        let mut config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        layer_mut(&mut config, "local-keyword")
            .matcher
            .keywords
            .push(" ".to_owned());
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_a_fixed_address_for_a_local_layer() {
        let mut config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        layer_mut(&mut config, "local-fallback").address = Some("10.0.0.53:53".parse().unwrap());
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_a_zero_local_refresh_interval() {
        let mut config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        layer_mut(&mut config, "local-fallback").refresh_secs = Some(0);
        assert!(config.validate().is_err());
    }

    #[test]
    fn supports_explicit_literal_contains_matching() {
        let mut config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        let matcher = &mut layer_mut(&mut config, "local-keyword").matcher;
        matcher.mode = KeywordMatchMode::Contains;
        matcher.keywords = vec!["video".to_owned()];

        assert_eq!(
            config.select_layer(Some("my-video.example.")),
            "local-keyword"
        );
    }

    #[test]
    fn selects_the_first_matching_sing_box_rule_set_layer() {
        let config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        assert_eq!(
            config.select_layer_with_rule_sets(Some("www.example.cn."), |tag, domain| {
                tag == "cn" && domain == "www.example.cn"
            }),
            "cn-preferred"
        );
    }

    #[test]
    fn rejects_a_layer_that_references_an_unknown_rule_set() {
        let mut config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        layer_mut(&mut config, "local-keyword")
            .matcher
            .rule_sets
            .push("missing".to_owned());

        assert!(config.validate().is_err());
    }

    #[test]
    fn example_configuration_routes_regions_through_the_interceptor() {
        let config: FileConfig = serde_json::from_str(include_str!("../config.example.json"))
            .expect("valid example JSON");
        config.validate().expect("example configuration validates");

        assert_eq!(
            config.select_layer(Some("work.be.mi.com.")),
            "local-keyword"
        );
        assert_eq!(
            config.select_layer_with_rule_sets(Some("www.qq.com."), |tag, _| tag == "geosite-cn"),
            "cn-preferred"
        );
        assert_eq!(
            config.select_layer_with_rule_sets(Some("www.google.com."), |tag, _| {
                tag == "geosite-geolocation-not-cn"
            }),
            "overseas-preferred"
        );
        assert_eq!(
            config
                .layer("cn-preferred")
                .and_then(|layer| layer.fallback.as_deref()),
            Some("tencent-doh")
        );
        assert_eq!(
            config
                .layer("overseas-preferred")
                .and_then(|layer| layer.fallback.as_deref()),
            Some("cloudflare-doh")
        );
        assert_eq!(
            config
                .layer("preferred")
                .and_then(|layer| layer.fallback.as_deref()),
            Some("tencent-doh")
        );
        let optimizer = &config
            .plugin("cloudflare-preferred")
            .expect("example plugin exists")
            .optimizer;
        assert_eq!(optimizer.candidates.len(), 26);
        assert_eq!(optimizer.samples_per_cidr, 40);
        assert_eq!(optimizer.max_candidates, 1040);
        assert_eq!(optimizer.excluded_candidates, ["172.64.228.0/24"]);
    }

    #[test]
    fn rejects_non_canonical_compatibility_hosts() {
        let mut config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        let optimizer = &mut config.plugins[0].optimizer;
        optimizer.enabled = true;
        optimizer.candidates = vec!["104.16.0.0/13".to_owned()];
        optimizer.compatibility_hosts = vec![" blog.qoop.top ".to_owned()];

        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_enabled_optimizer_without_compatibility_hosts() {
        let mut config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        let optimizer = &mut config.plugins[0].optimizer;
        optimizer.enabled = true;
        optimizer.candidates = vec!["104.16.0.0/13".to_owned()];

        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_invalid_excluded_candidate() {
        let mut config: FileConfig = serde_json::from_str(CONFIG).expect("valid JSON");
        let optimizer = &mut config.plugins[0].optimizer;
        optimizer.enabled = true;
        optimizer.candidates = vec!["104.16.0.0/13".to_owned()];
        optimizer.compatibility_hosts = vec!["blog.qoop.top".to_owned()];
        optimizer.excluded_candidates = vec!["not-an-address".to_owned()];

        assert!(config.validate().is_err());
    }
}
