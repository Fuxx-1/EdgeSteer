use std::{str::FromStr, time::Duration};

use anyhow::{Context, Result, bail};
use ipnet::IpNet;
use tokio::time::sleep;
use tracing::{info, warn};

use crate::state::SharedState;

const FALLBACK_RANGES: &[&str] = &[
    "173.245.48.0/20",
    "103.21.244.0/22",
    "103.22.200.0/22",
    "103.31.4.0/22",
    "141.101.64.0/18",
    "108.162.192.0/18",
    "190.93.240.0/20",
    "188.114.96.0/20",
    "197.234.240.0/22",
    "198.41.128.0/17",
    "162.158.0.0/15",
    "104.16.0.0/13",
    "104.24.0.0/14",
    "172.64.0.0/13",
    "131.0.72.0/22",
    "2400:cb00::/32",
    "2606:4700::/32",
    "2803:f800::/32",
    "2405:b500::/32",
    "2405:8100::/32",
    "2a06:98c0::/29",
    "2c0f:f248::/32",
];

const RANGE_URLS: &[&str] = &[
    "https://www.cloudflare.com/ips-v4",
    "https://www.cloudflare.com/ips-v6",
];

pub fn fallback_ranges() -> Result<Vec<IpNet>> {
    parse_ranges(FALLBACK_RANGES.iter().copied())
}

pub async fn refresh_loop(state: SharedState) {
    let client = match reqwest::Client::builder()
        .user_agent("edgesteer/0.1")
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            warn!(%error, "could not initialize Cloudflare range refresh client");
            return;
        }
    };

    loop {
        match fetch_ranges(&client).await {
            Ok(ranges) => {
                let count = ranges.len();
                state.cloudflare_ranges.store(ranges.into());
                info!(
                    count,
                    "refreshed Cloudflare IP ranges from the official list"
                );
            }
            Err(error) => {
                warn!(%error, "could not refresh Cloudflare IP ranges; keeping the active list")
            }
        }

        let seconds = state.runtime.load().config.cloudflare.range_refresh_secs;
        tokio::select! {
            _ = sleep(Duration::from_secs(seconds)) => {}
            _ = state.config_changed.notified() => {}
        }
    }
}

pub async fn fetch_ranges(client: &reqwest::Client) -> Result<Vec<IpNet>> {
    let mut values = Vec::new();
    for url in RANGE_URLS {
        let response = client
            .get(*url)
            .send()
            .await
            .with_context(|| format!("request {url}"))?
            .error_for_status()
            .with_context(|| format!("read HTTP status from {url}"))?;
        let body = response
            .text()
            .await
            .with_context(|| format!("read {url}"))?;
        values.extend(body.split_whitespace().map(str::to_owned));
    }
    parse_ranges(values.iter().map(String::as_str))
}

fn parse_ranges<'a>(values: impl IntoIterator<Item = &'a str>) -> Result<Vec<IpNet>> {
    let ranges: Result<Vec<_>> = values
        .into_iter()
        .filter(|value| !value.is_empty() && !value.starts_with('#'))
        .map(|value| IpNet::from_str(value).with_context(|| format!("parse CIDR {value}")))
        .collect();
    let ranges = ranges?;
    if ranges.is_empty() {
        bail!("Cloudflare IP range list is empty");
    }
    Ok(ranges)
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use super::*;

    #[test]
    fn fallback_ranges_contain_current_cloudflare_examples() {
        let ranges = fallback_ranges().expect("fallback ranges parse");
        assert!(
            ranges
                .iter()
                .any(|range| range.contains(&IpAddr::V4(Ipv4Addr::new(104, 16, 1, 1))))
        );
        assert!(
            ranges
                .iter()
                .any(|range| range.contains(&IpAddr::V6(Ipv6Addr::from(
                    0x2606_4700_0000_0000_0000_0000_0000_1111u128
                ))))
        );
    }
}
