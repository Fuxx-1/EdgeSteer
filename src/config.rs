use std::{
    fs,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::Path,
    str::FromStr,
};

use anyhow::{Context, Result, bail};
use reqwest::Url;
use rustls::pki_types::ServerName;
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct FileConfig {
    pub listener: ListenerConfig,
    pub cloudflare: CloudflareConfig,
    pub preferred: PreferredConfig,
    pub optimizer: OptimizerConfig,
    pub upstreams: Vec<UpstreamConfig>,
}

impl FileConfig {
    pub fn validate(&self) -> Result<()> {
        if self.upstreams.is_empty() {
            bail!("at least one [[upstreams]] entry is required");
        }
        if !self.listener.allow_remote && !self.listener.address.ip().is_loopback() {
            bail!(
                "listener.address {} is not loopback; set listener.allow_remote = true only for an intentional LAN DNS service",
                self.listener.address
            );
        }
        if self.cloudflare.rewrite_ttl_secs == 0 {
            bail!("cloudflare.rewrite_ttl_secs must be greater than zero");
        }
        if self.cloudflare.range_refresh_secs == 0 {
            bail!("cloudflare.range_refresh_secs must be greater than zero");
        }
        for upstream in &self.upstreams {
            if upstream.address == self.listener.address {
                bail!(
                    "upstream {} matches listener.address and would create a DNS forwarding loop",
                    upstream.address
                );
            }
            if upstream.timeout_ms == 0 {
                bail!("upstream {} has a zero timeout", upstream.address);
            }
            if upstream.address.port() == 0 {
                bail!("upstream {} has port zero", upstream.address);
            }
            upstream.validate_protocol()?;
        }
        self.optimizer.validate()
    }
}

pub fn load_config(path: &Path) -> Result<FileConfig> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("read configuration file {}", path.display()))?;
    let config: FileConfig = toml::from_str(&contents)
        .with_context(|| format!("parse TOML configuration file {}", path.display()))?;
    config.validate()?;
    Ok(config)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CloudflareConfig {
    pub rewrite_ttl_secs: u32,
    pub range_refresh_secs: u64,
}

impl Default for CloudflareConfig {
    fn default() -> Self {
        Self {
            rewrite_ttl_secs: 60,
            range_refresh_secs: 24 * 60 * 60,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PreferredConfig {
    pub ipv4: Option<Ipv4Addr>,
    pub ipv6: Option<Ipv6Addr>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UpstreamProtocol {
    #[default]
    Udp,
    Tcp,
    Doh,
    Dot,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct UpstreamConfig {
    /// UDP/TCP/DoT endpoint, or the fixed bootstrap address for a DoH URL.
    pub address: SocketAddr,
    #[serde(default)]
    pub protocol: UpstreamProtocol,
    #[serde(default = "default_upstream_timeout_ms")]
    pub timeout_ms: u64,
    /// Required for protocol = "doh". Its hostname provides TLS SNI and Host.
    #[serde(default)]
    pub url: Option<String>,
    /// Required for protocol = "dot" so certificate verification has an SNI name.
    #[serde(default)]
    pub server_name: Option<String>,
}

const fn default_upstream_timeout_ms() -> u64 {
    3_000
}

impl UpstreamConfig {
    fn validate_protocol(&self) -> Result<()> {
        match self.protocol {
            UpstreamProtocol::Udp | UpstreamProtocol::Tcp => {
                if self.url.is_some() || self.server_name.is_some() {
                    bail!(
                        "upstream {} uses protocol {:?}; url and server_name are only valid for DoH/DoT",
                        self.address,
                        self.protocol
                    );
                }
            }
            UpstreamProtocol::Dot => {
                if self.url.is_some() {
                    bail!("DoT upstream {} must not set url", self.address);
                }
                let server_name = self.server_name.as_ref().context(
                    "DoT upstream requires server_name for TLS certificate verification",
                )?;
                ServerName::try_from(server_name.clone())
                    .with_context(|| format!("invalid DoT server_name {server_name:?}"))?;
            }
            UpstreamProtocol::Doh => {
                if self.server_name.is_some() {
                    bail!(
                        "DoH upstream {} derives SNI from url; do not set server_name",
                        self.address
                    );
                }
                let endpoint = self.doh_endpoint()?;
                let endpoint_port = endpoint
                    .port_or_known_default()
                    .context("DoH URL has no usable HTTPS port")?;
                if endpoint_port != self.address.port() {
                    bail!(
                        "DoH URL port {endpoint_port} must match bootstrap address {}",
                        self.address
                    );
                }
            }
        }
        Ok(())
    }

    pub fn doh_endpoint(&self) -> Result<Url> {
        let raw_url = self.url.as_deref().context("DoH upstream requires url")?;
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

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
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
            test_host: "speed.cloudflare.com".to_owned(),
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
    fn validate(&self) -> Result<()> {
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

    #[test]
    fn parses_a_minimal_config() {
        let config: FileConfig = toml::from_str(
            r#"
                [[upstreams]]
                address = "1.1.1.1:53"
            "#,
        )
        .expect("valid config");

        config.validate().expect("configuration validates");
        assert_eq!(config.listener.address, "127.0.0.1:53".parse().unwrap());
        assert_eq!(config.upstreams[0].protocol, UpstreamProtocol::Udp);
    }

    #[test]
    fn refuses_non_loopback_listener_without_opt_in() {
        let config: FileConfig = toml::from_str(
            r#"
                [listener]
                address = "0.0.0.0:53"

                [[upstreams]]
                address = "1.1.1.1:53"
            "#,
        )
        .expect("valid TOML");

        assert!(config.validate().is_err());
    }

    #[test]
    fn validates_doh_and_dot_upstreams() {
        let config: FileConfig = toml::from_str(
            r#"
                [[upstreams]]
                protocol = "doh"
                address = "1.1.1.1:443"
                url = "https://cloudflare-dns.com/dns-query"

                [[upstreams]]
                protocol = "dot"
                address = "1.1.1.1:853"
                server_name = "cloudflare-dns.com"
            "#,
        )
        .expect("valid encrypted upstream TOML");

        config.validate().expect("encrypted upstreams validate");
        assert_eq!(config.upstreams[0].protocol, UpstreamProtocol::Doh);
        assert_eq!(config.upstreams[1].protocol, UpstreamProtocol::Dot);
    }

    #[test]
    fn rejects_doh_with_a_mismatched_bootstrap_port() {
        let config: FileConfig = toml::from_str(
            r#"
                [[upstreams]]
                protocol = "doh"
                address = "1.1.1.1:8443"
                url = "https://cloudflare-dns.com/dns-query"
            "#,
        )
        .expect("valid TOML");

        assert!(config.validate().is_err());
    }
}
