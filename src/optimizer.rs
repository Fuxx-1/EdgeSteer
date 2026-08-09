use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use ipnet::IpNet;
use rand::Rng;
use rustls::{ClientConfig, RootCertStore, pki_types::ServerName};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    task::JoinSet,
    time::{sleep, timeout},
};
use tokio_rustls::TlsConnector;
use tracing::{debug, info, warn};

use crate::{
    config::OptimizerConfig,
    state::{PreferredIps, SharedState},
};

#[derive(Debug)]
struct ProbeResult {
    address: IpAddr,
    latency: Duration,
}

pub async fn run_loop(state: SharedState) {
    loop {
        let config = state.config.load_full();
        let settings = config.optimizer.clone();
        if settings.enabled {
            let ranges = state.cloudflare_ranges.load_full();
            match choose_preferred(&settings, ranges.as_slice()).await {
                Ok(selection) if !selection.is_empty() => {
                    let mut next = state.preferred_ips.load_full().as_ref().clone();
                    if selection.ipv4.is_some() {
                        next.ipv4 = selection.ipv4;
                    }
                    if selection.ipv6.is_some() {
                        next.ipv6 = selection.ipv6;
                    }
                    state.preferred_ips.store(Arc::new(next.clone()));
                    info!(preferred = ?next, "updated preferred Cloudflare IPs from the integrated probe");
                }
                Ok(_) => warn!("Cloudflare probe did not produce a usable preferred IP"),
                Err(error) => {
                    warn!(%error, "Cloudflare probe round failed; retaining previous preferred IPs")
                }
            }
        }

        tokio::select! {
            _ = sleep(Duration::from_secs(settings.interval_secs.max(1))) => {}
            _ = state.config_changed.notified() => {}
        }
    }
}

async fn choose_preferred(settings: &OptimizerConfig, ranges: &[IpNet]) -> Result<PreferredIps> {
    let candidates = expand_candidates(settings, ranges)?;
    let connector = build_tls_connector();
    let worker_count = settings.concurrency.min(candidates.len());
    let mut pending = candidates.into_iter();
    let mut tasks = JoinSet::new();

    for _ in 0..worker_count {
        if let Some(address) = pending.next() {
            tasks.spawn(probe_candidate(
                address,
                settings.clone(),
                connector.clone(),
            ));
        }
    }

    let mut best_ipv4: Option<ProbeResult> = None;
    let mut best_ipv6: Option<ProbeResult> = None;
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(probe)) => match probe.address {
                IpAddr::V4(_) if is_faster(&probe, best_ipv4.as_ref()) => best_ipv4 = Some(probe),
                IpAddr::V6(_) if is_faster(&probe, best_ipv6.as_ref()) => best_ipv6 = Some(probe),
                _ => {}
            },
            Ok(Err(error)) => debug!(%error, "Cloudflare candidate did not pass the probe"),
            Err(error) => warn!(%error, "Cloudflare probe task failed"),
        }
        if let Some(address) = pending.next() {
            tasks.spawn(probe_candidate(
                address,
                settings.clone(),
                connector.clone(),
            ));
        }
    }

    Ok(PreferredIps {
        ipv4: best_ipv4.and_then(|result| match result.address {
            IpAddr::V4(address) => Some(address),
            IpAddr::V6(_) => None,
        }),
        ipv6: best_ipv6.and_then(|result| match result.address {
            IpAddr::V4(_) => None,
            IpAddr::V6(address) => Some(address),
        }),
    })
}

fn is_faster(candidate: &ProbeResult, current: Option<&ProbeResult>) -> bool {
    current.is_none_or(|current| candidate.latency < current.latency)
}

fn build_tls_connector() -> TlsConnector {
    crate::install_rustls_crypto_provider();
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

async fn probe_candidate(
    address: IpAddr,
    settings: OptimizerConfig,
    connector: TlsConnector,
) -> Result<ProbeResult> {
    let timeout_duration = Duration::from_millis(settings.timeout_ms);
    let started_at = Instant::now();
    let stream = timeout(
        timeout_duration,
        TcpStream::connect(SocketAddr::new(address, settings.test_port)),
    )
    .await
    .context("TCP connection timed out")??;
    let server_name = ServerName::try_from(settings.test_host.clone())
        .context("optimizer.test_host is not a valid TLS server name")?;
    let mut stream = timeout(timeout_duration, connector.connect(server_name, stream))
        .await
        .context("TLS handshake timed out")??;
    let request = probe_request(&settings);
    timeout(timeout_duration, stream.write_all(request.as_bytes()))
        .await
        .context("HTTP probe write timed out")??;
    timeout(timeout_duration, stream.flush())
        .await
        .context("HTTP probe flush timed out")??;

    let mut header = Vec::with_capacity(1024);
    let mut buffer = [0_u8; 1024];
    while header.len() < 8 * 1024 && !header.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = timeout(timeout_duration, stream.read(&mut buffer))
            .await
            .context("HTTP probe read timed out")??;
        if read == 0 {
            break;
        }
        header.extend_from_slice(&buffer[..read]);
    }
    validate_cloudflare_probe_response(&header)?;

    Ok(ProbeResult {
        address,
        latency: started_at.elapsed(),
    })
}

fn probe_request(settings: &OptimizerConfig) -> String {
    format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: edgesteer/0.1\r\nConnection: close\r\n\r\n",
        settings.test_path, settings.test_host
    )
}

fn validate_cloudflare_probe_response(header: &[u8]) -> Result<()> {
    let header = String::from_utf8_lossy(header).to_ascii_lowercase();
    if !(header.starts_with("http/1.1 2") || header.starts_with("http/1.0 2")) {
        bail!("Cloudflare probe did not return a successful HTTP status");
    }
    if !header
        .lines()
        .any(|line| line.trim() == "server: cloudflare")
    {
        bail!("Cloudflare probe response lacks server: cloudflare");
    }
    Ok(())
}

fn expand_candidates(settings: &OptimizerConfig, ranges: &[IpNet]) -> Result<Vec<IpAddr>> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut rng = rand::rng();

    'candidates: for candidate in &settings.candidates {
        let remaining = settings.max_candidates.saturating_sub(result.len());
        if remaining == 0 {
            break;
        }
        let values = if let Ok(address) = candidate.parse::<IpAddr>() {
            vec![address]
        } else {
            let network = IpNet::from_str(candidate)
                .with_context(|| format!("parse optimizer candidate {candidate:?}"))?;
            random_addresses_in_network(network, settings.samples_per_cidr.min(remaining), &mut rng)
        };

        for address in values {
            if !ranges.iter().any(|range| range.contains(&address)) {
                debug!(%address, "ignoring optimizer candidate outside the Cloudflare ranges");
                continue;
            }
            if seen.insert(address) {
                result.push(address);
                if result.len() == settings.max_candidates {
                    break 'candidates;
                }
            }
        }
    }

    if result.is_empty() {
        bail!("no optimizer candidates are in the active Cloudflare IP ranges");
    }
    Ok(result)
}

fn random_addresses_in_network(
    network: IpNet,
    sample_count: usize,
    rng: &mut impl Rng,
) -> Vec<IpAddr> {
    let target = sample_count.min(selectable_address_count(network));
    let mut values = Vec::with_capacity(target);
    let mut seen = HashSet::with_capacity(target);
    while values.len() < target {
        let address = random_address_in_network(network, rng);
        if seen.insert(address) {
            values.push(address);
        }
    }
    values
}

fn selectable_address_count(network: IpNet) -> usize {
    let count = match network {
        IpNet::V4(network) => {
            let host_bits = 32 - network.prefix_len();
            let total = 1_u128 << host_bits;
            if host_bits <= 1 { total } else { total - 2 }
        }
        IpNet::V6(network) => {
            let host_bits = 128 - network.prefix_len();
            if host_bits == 128 {
                return usize::MAX;
            }
            1_u128 << host_bits
        }
    };
    usize::try_from(count).unwrap_or(usize::MAX)
}

fn random_address_in_network(network: IpNet, rng: &mut impl Rng) -> IpAddr {
    match network {
        IpNet::V4(network) => {
            let prefix_len = network.prefix_len();
            let host_mask = if prefix_len == 32 {
                0
            } else {
                u32::MAX >> prefix_len
            };
            let host = random_ipv4_host(host_mask, rng);
            let network_address = u32::from(network.network());
            IpAddr::V4(Ipv4Addr::from(network_address | host))
        }
        IpNet::V6(network) => {
            let prefix_len = network.prefix_len();
            let host_mask = if prefix_len == 128 {
                0
            } else {
                u128::MAX >> prefix_len
            };
            let network_address = u128::from(network.network());
            IpAddr::V6(Ipv6Addr::from(
                network_address | (rng.random::<u128>() & host_mask),
            ))
        }
    }
}

fn random_ipv4_host(mask: u32, rng: &mut impl Rng) -> u32 {
    if mask <= 1 {
        rng.random::<u32>() & mask
    } else {
        rng.random_range(1..mask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> OptimizerConfig {
        OptimizerConfig {
            enabled: true,
            candidates: vec!["104.16.0.0/24".to_owned(), "198.51.100.1".to_owned()],
            samples_per_cidr: 4,
            max_candidates: 8,
            ..OptimizerConfig::default()
        }
    }

    #[test]
    fn expands_only_cloudflare_candidates() {
        let ranges = vec![IpNet::from_str("104.16.0.0/13").unwrap()];
        let candidates = expand_candidates(&settings(), &ranges).expect("candidates expand");
        assert_eq!(candidates.len(), 4);
        assert!(
            candidates
                .iter()
                .all(|candidate| ranges[0].contains(candidate))
        );
    }

    #[test]
    fn does_not_duplicate_cidr_samples() {
        let mut settings = settings();
        settings.candidates = vec!["104.16.0.0/30".to_owned()];
        settings.samples_per_cidr = 4;
        let ranges = vec![IpNet::from_str("104.16.0.0/13").unwrap()];

        let candidates = expand_candidates(&settings, &ranges).expect("candidates expand");

        assert_eq!(candidates.len(), 2);
        assert_ne!(candidates[0], candidates[1]);
    }

    #[test]
    fn validates_cloudflare_probe_header() {
        assert!(
            validate_cloudflare_probe_response(b"HTTP/1.1 200 OK\r\nServer: cloudflare\r\n\r\n")
                .is_ok()
        );
        assert!(
            validate_cloudflare_probe_response(b"HTTP/1.1 200 OK\r\nServer: nginx\r\n\r\n")
                .is_err()
        );
    }

    #[test]
    fn trace_probe_uses_get() {
        let settings = OptimizerConfig::default();
        assert!(probe_request(&settings).starts_with("GET /cdn-cgi/trace HTTP/1.1\r\n"));
    }
}
