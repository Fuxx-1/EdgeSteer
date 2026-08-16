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
    config::{OptimizerConfig, PluginConfig},
    state::{PreferredIps, SharedState},
};

#[derive(Debug)]
struct ProbeResult {
    address: IpAddr,
    score: Duration,
    median_latency: Duration,
    worst_latency: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProbeTiming {
    score: Duration,
    median_latency: Duration,
    worst_latency: Duration,
}

pub async fn run_loop(state: SharedState) {
    loop {
        let runtime = state.runtime.load_full();
        let plugins: Vec<PluginConfig> = runtime
            .config
            .cloudflare_preferred_plugins()
            .cloned()
            .collect();
        let mut next_interval_secs = None;

        for plugin in &plugins {
            let settings = &plugin.optimizer;
            if !settings.enabled {
                continue;
            }
            next_interval_secs = Some(
                next_interval_secs
                    .unwrap_or(settings.interval_secs)
                    .min(settings.interval_secs),
            );
            let ranges = state.cloudflare_ranges.load_full();
            match choose_preferred(settings, ranges.as_slice()).await {
                Ok(selection) if !selection.is_empty() => {
                    if let Some(next) = state.update_preferred(plugin, &selection) {
                        info!(
                            plugin = %plugin.tag,
                            preferred = ?next,
                            "updated preferred Cloudflare IPs from the integrated probe"
                        );
                    }
                }
                Ok(_) => {
                    if settings.requires_compatibility_gate() {
                        state.clear_preferred(plugin);
                        warn!(plugin = %plugin.tag, "Cloudflare compatibility probe did not produce a usable IP; cleared strict preferred IPs");
                    } else {
                        warn!(plugin = %plugin.tag, "Cloudflare probe did not produce a usable preferred IP");
                    }
                }
                Err(error) => {
                    if settings.requires_compatibility_gate() {
                        state.clear_preferred(plugin);
                        warn!(plugin = %plugin.tag, %error, "Cloudflare compatibility probe round failed; cleared strict preferred IPs");
                    } else {
                        warn!(plugin = %plugin.tag, %error, "Cloudflare probe round failed; retaining previous preferred IPs");
                    }
                }
            }
        }

        tokio::select! {
            _ = sleep(Duration::from_secs(next_interval_secs.unwrap_or(60).max(1))) => {}
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
    current.is_none_or(|current| {
        candidate.score < current.score
            || (candidate.score == current.score
                && candidate.median_latency < current.median_latency)
            || (candidate.score == current.score
                && candidate.median_latency == current.median_latency
                && candidate.worst_latency < current.worst_latency)
    })
}

fn build_tls_connector() -> TlsConnector {
    TlsConnector::from(Arc::new(build_tls_client_config()))
}

fn build_tls_client_config() -> ClientConfig {
    crate::install_rustls_crypto_provider();
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    // Match the ALPN offered by normal HTTPS clients. Without it, an edge can
    // pass this probe but reset browser-like TLS handshakes after DNS rewrite.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    config
}

async fn probe_candidate(
    address: IpAddr,
    settings: OptimizerConfig,
    connector: TlsConnector,
) -> Result<ProbeResult> {
    let mut samples = Vec::with_capacity(settings.probes_per_candidate);
    for _ in 0..settings.probes_per_candidate {
        // A candidate must pass every probe. This rejects fast-but-flaky edges
        // instead of allowing one lucky response to win the round.
        samples.push(probe_once(address, &settings, connector.clone()).await?);
    }
    for host in &settings.compatibility_hosts {
        probe_compatibility_host(address, host, &settings, connector.clone())
            .await
            .with_context(|| format!("compatibility probe for {host:?} failed"))?;
    }
    let timing = score_probe_samples(&mut samples);

    Ok(ProbeResult {
        address,
        score: timing.score,
        median_latency: timing.median_latency,
        worst_latency: timing.worst_latency,
    })
}

async fn probe_once(
    address: IpAddr,
    settings: &OptimizerConfig,
    connector: TlsConnector,
) -> Result<Duration> {
    let (latency, header) = probe_http(
        address,
        settings.test_port,
        &settings.test_host,
        &settings.test_path,
        settings.timeout_ms,
        connector,
        false,
    )
    .await?;
    validate_cloudflare_probe_response(&header)?;
    Ok(latency)
}

async fn probe_compatibility_host(
    address: IpAddr,
    host: &str,
    settings: &OptimizerConfig,
    connector: TlsConnector,
) -> Result<()> {
    for _ in 0..settings.probes_per_candidate {
        let (_, response) = probe_http(
            address,
            settings.test_port,
            host,
            "/",
            settings.timeout_ms,
            connector.clone(),
            true,
        )
        .await?;
        validate_compatibility_probe_response(&response)?;
    }
    Ok(())
}

/// Verifies a single selected address for a query hostname. This is used by
/// the strict response gate and intentionally performs the same repeated
/// SNI/Host validation as the optimizer's configured compatibility hosts.
pub async fn verify_compatibility(
    address: IpAddr,
    host: &str,
    settings: &OptimizerConfig,
) -> Result<()> {
    probe_compatibility_host(address, host, settings, build_tls_connector()).await
}

async fn probe_http(
    address: IpAddr,
    port: u16,
    host: &str,
    path: &str,
    timeout_ms: u64,
    connector: TlsConnector,
    capture_body_prefix: bool,
) -> Result<(Duration, Vec<u8>)> {
    let timeout_duration = Duration::from_millis(timeout_ms);
    let started_at = Instant::now();
    let stream = timeout(
        timeout_duration,
        TcpStream::connect(SocketAddr::new(address, port)),
    )
    .await
    .context("TCP connection timed out")??;
    let server_name = ServerName::try_from(host.to_owned())
        .with_context(|| format!("optimizer probe host {host:?} is not a valid TLS server name"))?;
    let mut stream = timeout(timeout_duration, connector.connect(server_name, stream))
        .await
        .context("TLS handshake timed out")??;
    let request = probe_request(host, path);
    timeout(timeout_duration, stream.write_all(request.as_bytes()))
        .await
        .context("HTTP probe write timed out")??;
    timeout(timeout_duration, stream.flush())
        .await
        .context("HTTP probe flush timed out")??;

    let mut response = Vec::with_capacity(1024);
    let mut buffer = [0_u8; 1024];
    let mut header_latency = None;
    let mut body_prefix_limit = None;
    while response.len() < 12 * 1024 {
        let read = timeout(timeout_duration, stream.read(&mut buffer))
            .await
            .context("HTTP probe read timed out")??;
        if read == 0 {
            break;
        }
        response.extend_from_slice(&buffer[..read]);
        let Some(header_end) = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| index + 4)
        else {
            continue;
        };
        if header_latency.is_none() {
            header_latency = Some(started_at.elapsed());
            body_prefix_limit = Some(if capture_body_prefix {
                compatibility_body_prefix_limit(&response[..header_end], header_end)
            } else {
                header_end
            });
        }
        if response.len() >= body_prefix_limit.expect("header limit is initialized") {
            break;
        }
    }
    Ok((
        header_latency.unwrap_or_else(|| started_at.elapsed()),
        response,
    ))
}

fn compatibility_body_prefix_limit(header: &[u8], header_end: usize) -> usize {
    const MAX_BODY_PREFIX: usize = 4 * 1024;
    let header = String::from_utf8_lossy(header).to_ascii_lowercase();
    let content_length = header.lines().find_map(|line| {
        line.strip_prefix("content-length:")
            .and_then(|value| value.trim().parse::<usize>().ok())
    });
    let chunked = header
        .lines()
        .any(|line| line.trim() == "transfer-encoding: chunked");
    match (content_length, chunked) {
        (Some(length), _) => header_end.saturating_add(length.min(MAX_BODY_PREFIX)),
        (None, true) => header_end.saturating_add(MAX_BODY_PREFIX),
        // `Connection: close` makes an unframed body safe to read until EOF.
        // Capture a bounded prefix anyway: Cloudflare's 1034 marker is in the
        // HTML body and must not be missed merely because framing is absent.
        (None, false) => header_end.saturating_add(MAX_BODY_PREFIX),
    }
}

fn score_probe_samples(samples: &mut [Duration]) -> ProbeTiming {
    debug_assert!(!samples.is_empty());
    samples.sort_unstable();

    let lower_middle = samples[(samples.len() - 1) / 2];
    let upper_middle = samples[samples.len() / 2];
    let median_latency = lower_middle.saturating_add(upper_middle) / 2;
    let worst_latency = *samples.last().expect("samples cannot be empty");
    // Penalize a slow tail while retaining the responsiveness of the median.
    let score = median_latency.saturating_add(
        worst_latency
            .saturating_sub(median_latency)
            .checked_div(2)
            .unwrap_or_default(),
    );

    ProbeTiming {
        score,
        median_latency,
        worst_latency,
    }
}

fn probe_request(host: &str, path: &str) -> String {
    format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: edgesteer/0.1\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n",
        path, host
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

fn validate_compatibility_probe_response(response: &[u8]) -> Result<()> {
    let response = String::from_utf8_lossy(response);
    let lowered = response.to_ascii_lowercase();
    if ["error 1034", "error code: 1034", "edge ip restricted"]
        .iter()
        .any(|marker| lowered.contains(marker))
    {
        bail!("compatibility probe returned a Cloudflare edge-IP restriction");
    }
    let status = response
        .lines()
        .next()
        .and_then(|line| line.split_ascii_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .context("compatibility probe returned an invalid HTTP status line")?;
    if !(200..400).contains(&status) {
        bail!("compatibility probe returned HTTP status {status}");
    }
    Ok(())
}

fn expand_candidates(settings: &OptimizerConfig, ranges: &[IpNet]) -> Result<Vec<IpAddr>> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut rng = rand::rng();
    let excluded = settings.excluded_networks()?;

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
            if excluded.iter().any(|network| network.contains(&address)) {
                debug!(%address, "ignoring explicitly excluded optimizer candidate");
                continue;
            }
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
    fn excludes_configured_candidate_ranges() {
        let mut settings = settings();
        settings.candidates = vec!["104.16.0.0/30".to_owned()];
        settings.samples_per_cidr = 4;
        settings.excluded_candidates = vec!["104.16.0.1".to_owned()];
        let ranges = vec![IpNet::from_str("104.16.0.0/13").unwrap()];

        let candidates = expand_candidates(&settings, &ranges).expect("candidates expand");

        assert_eq!(candidates, vec![IpAddr::V4(Ipv4Addr::new(104, 16, 0, 2))]);
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
        let request = probe_request(&settings.test_host, &settings.test_path);
        assert!(request.starts_with("GET /cdn-cgi/trace HTTP/1.1\r\n"));
        assert!(request.contains("Accept-Encoding: identity\r\n"));
    }

    #[test]
    fn compatibility_probe_reads_an_unframed_body_prefix() {
        let header = b"HTTP/1.1 200 OK\r\n\r\n";

        assert_eq!(
            compatibility_body_prefix_limit(header, header.len()),
            header.len() + 4 * 1024
        );
    }

    #[test]
    fn trace_probe_advertises_http1_alpn() {
        assert_eq!(build_tls_client_config().alpn_protocols, vec![b"http/1.1"]);
    }

    #[test]
    fn stable_score_penalizes_slow_tail() {
        let mut stable = [
            Duration::from_millis(400),
            Duration::from_millis(410),
            Duration::from_millis(420),
        ];
        let mut spiky = [
            Duration::from_millis(190),
            Duration::from_millis(250),
            Duration::from_millis(1_970),
        ];

        let stable_timing = score_probe_samples(&mut stable);
        let spiky_timing = score_probe_samples(&mut spiky);

        assert_eq!(stable_timing.score, Duration::from_millis(415));
        assert_eq!(spiky_timing.score, Duration::from_millis(1_110));
        assert!(stable_timing.score < spiky_timing.score);
    }

    #[test]
    fn compatibility_probe_accepts_redirects_and_rejects_1034() {
        assert!(validate_compatibility_probe_response(b"HTTP/1.1 302 Found\r\n\r\n").is_ok());
        assert!(
            validate_compatibility_probe_response(
                b"HTTP/1.1 403 Forbidden\r\nServer: cloudflare\r\n\r\nerror code: 1034"
            )
            .is_err()
        );
        assert!(
            validate_compatibility_probe_response(
                b"HTTP/1.1 200 OK\r\nServer: cloudflare\r\n\r\nPlease enable cookies. Error 1034 Edge IP Restricted"
            )
            .is_err()
        );
    }
}
