use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use tokio::{task, time::sleep};
use tracing::{debug, info, warn};

use crate::{
    config::LayerType,
    state::{LocalResolvers, SharedState},
};

/// Refreshes the locally configured resolver addresses for every active
/// `local` layer. The source is the operating system configuration, never the
/// operating system resolver, so this does not itself perform a DNS lookup.
pub async fn refresh_loop(state: SharedState) {
    loop {
        let Some(refresh_secs) = configured_refresh_secs(&state) else {
            state.config_changed.notified().await;
            continue;
        };

        if let Err(error) = refresh(&state).await {
            warn!(%error, "could not refresh system DNS upstreams; keeping the active local resolver cache");
        }

        tokio::select! {
            _ = sleep(Duration::from_secs(refresh_secs)) => {}
            _ = state.config_changed.notified() => {}
        }
    }
}

pub async fn refresh(state: &SharedState) -> Result<Arc<LocalResolvers>> {
    let _refresh_guard = state.local_dns_refresh_lock.lock().await;
    refresh_locked(state).await
}

/// A failed local DNS transaction calls this path. If another transaction has
/// already refreshed the cache, it returns that newer snapshot instead of
/// reading the platform configuration again.
pub async fn refresh_after_failure(
    state: &SharedState,
    observed: &Arc<LocalResolvers>,
) -> Result<Arc<LocalResolvers>> {
    let _refresh_guard = state.local_dns_refresh_lock.lock().await;
    let current = state.local_resolvers();
    if !Arc::ptr_eq(&current, observed) {
        return Ok(current);
    }
    refresh_locked(state).await
}

async fn refresh_locked(state: &SharedState) -> Result<Arc<LocalResolvers>> {
    let listener = state.runtime.load().config.listener.address;
    let addresses = task::spawn_blocking(move || discover_system_dns(listener))
        .await
        .context("wait for system DNS discovery task")??;

    let previous = state.local_resolvers();
    let changed = previous.addresses() != addresses.as_slice();
    let current = state.replace_local_resolvers(addresses);
    if changed {
        info!(
            count = current.addresses().len(),
            "refreshed local DNS upstreams from system configuration"
        );
    } else {
        debug!(
            count = current.addresses().len(),
            "system DNS upstreams are unchanged"
        );
    }
    Ok(current)
}

fn configured_refresh_secs(state: &SharedState) -> Option<u64> {
    let runtime = state.runtime.load();
    runtime
        .config
        .layers
        .iter()
        .filter(|layer| layer.kind == LayerType::Local)
        .map(|layer| layer.refresh_secs())
        .min()
}

pub fn discover_system_dns(listener: SocketAddr) -> Result<Vec<SocketAddr>> {
    let addresses = platform_dns_servers()?;
    let addresses = normalize_dns_servers(addresses, listener);
    if addresses.is_empty() {
        bail!(
            "system DNS configuration has no usable upstream after filtering loopback, unspecified, multicast, link-local IPv6, and listener addresses"
        );
    }
    Ok(addresses)
}

fn normalize_dns_servers(addresses: Vec<IpAddr>, listener: SocketAddr) -> Vec<SocketAddr> {
    let mut seen = HashSet::new();
    addresses
        .into_iter()
        .filter(|address| usable_dns_server(*address, listener))
        .filter(|address| seen.insert(*address))
        .map(|address| SocketAddr::new(address, 53))
        .collect()
}

fn usable_dns_server(address: IpAddr, listener: SocketAddr) -> bool {
    if address.is_loopback() || address.is_unspecified() || address.is_multicast() {
        return false;
    }
    if matches!(address, IpAddr::V4(value) if value.is_broadcast())
        || matches!(address, IpAddr::V6(value) if value.is_unicast_link_local())
    {
        return false;
    }
    !socket_addresses_overlap(listener, SocketAddr::new(address, 53))
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

#[cfg(target_os = "macos")]
fn platform_dns_servers() -> Result<Vec<IpAddr>> {
    use core_foundation::{dictionary::CFDictionary, propertylist::CFPropertyList};
    use system_configuration::dynamic_store::SCDynamicStoreBuilder;

    let store = SCDynamicStoreBuilder::new("edgesteer")
        .build()
        .context("open macOS SystemConfiguration dynamic store")?;
    let dns_keys = store
        .get_keys("State:/Network/Service/.*/DNS")
        .context("list macOS DNS service configuration")?;
    let mut addresses = Vec::new();

    for dns_key in dns_keys.iter() {
        let Some(dns_settings) = store
            .get(dns_key.clone())
            .and_then(CFPropertyList::downcast_into::<CFDictionary>)
        else {
            continue;
        };
        let interface = macos_interface_name(&store, &dns_key, &dns_settings);
        if interface.as_deref().is_some_and(is_tunnel_interface) {
            debug!(
                ?interface,
                "ignoring DNS servers attached to a macOS tunnel interface"
            );
            continue;
        }
        addresses.extend(macos_dns_addresses(&dns_settings));
    }

    if addresses.is_empty() {
        bail!("macOS has no DNS servers on a non-tunnel network service");
    }
    Ok(addresses)
}

#[cfg(target_os = "macos")]
fn macos_interface_name(
    store: &system_configuration::dynamic_store::SCDynamicStore,
    dns_key: &core_foundation::string::CFString,
    dns_settings: &core_foundation::dictionary::CFDictionary,
) -> Option<String> {
    macos_dictionary_string(dns_settings, "InterfaceName")
        .or_else(|| macos_dictionary_string(dns_settings, "ConfirmedInterfaceName"))
        .or_else(|| {
            let path = dns_key.to_string();
            let service_id = path
                .strip_prefix("State:/Network/Service/")?
                .strip_suffix("/DNS")?;
            let ipv4_path = format!("State:/Network/Service/{service_id}/IPv4");
            let ipv4_settings = store.get(ipv4_path.as_str()).and_then(
                core_foundation::propertylist::CFPropertyList::downcast_into::<
                    core_foundation::dictionary::CFDictionary,
                >,
            )?;
            macos_dictionary_string(&ipv4_settings, "InterfaceName")
                .or_else(|| macos_dictionary_string(&ipv4_settings, "ConfirmedInterfaceName"))
        })
}

#[cfg(target_os = "macos")]
fn macos_dns_addresses(settings: &core_foundation::dictionary::CFDictionary) -> Vec<IpAddr> {
    use core_foundation::{
        array::CFArray,
        base::{CFType, TCFType, ToVoid},
        string::CFString,
    };

    let key = CFString::new("ServerAddresses");
    let Some(address_array) = settings
        .find(key.to_void())
        .map(|pointer| unsafe { CFType::wrap_under_get_rule(*pointer) })
        .and_then(CFType::downcast_into::<CFArray>)
    else {
        return Vec::new();
    };

    let mut addresses = Vec::with_capacity(address_array.len() as usize);
    for address_ptr in &address_array {
        let Some(address) =
            (unsafe { CFType::wrap_under_get_rule(*address_ptr) }).downcast_into::<CFString>()
        else {
            continue;
        };
        let value = address.to_string();
        match value.parse::<IpAddr>() {
            Ok(address) => addresses.push(address),
            Err(error) => debug!(%value, %error, "ignoring malformed macOS DNS server address"),
        }
    }
    addresses
}

#[cfg(target_os = "macos")]
fn macos_dictionary_string(
    settings: &core_foundation::dictionary::CFDictionary,
    name: &str,
) -> Option<String> {
    use core_foundation::{
        base::{CFType, TCFType, ToVoid},
        string::CFString,
    };

    let key = CFString::new(name);
    settings
        .find(key.to_void())
        .map(|pointer| unsafe { CFType::wrap_under_get_rule(*pointer) })
        .and_then(CFType::downcast_into::<CFString>)
        .map(|value| value.to_string())
}

#[cfg(target_os = "macos")]
fn is_tunnel_interface(interface: &str) -> bool {
    let interface = interface.to_ascii_lowercase();
    ["utun", "tun", "tap", "ppp", "ipsec"]
        .iter()
        .any(|prefix| interface.starts_with(prefix))
}

#[cfg(target_os = "linux")]
fn platform_dns_servers() -> Result<Vec<IpAddr>> {
    use std::{fs, io};

    const PATHS: [&str; 2] = ["/run/systemd/resolve/resolv.conf", "/etc/resolv.conf"];
    let mut last_error = None;
    for path in PATHS {
        match fs::read_to_string(path) {
            Ok(contents) => {
                let addresses = parse_resolv_conf(&contents);
                if !addresses.is_empty() {
                    return Ok(addresses);
                }
                last_error = Some(anyhow::anyhow!("{path} has no nameserver entries"));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => last_error = Some(error.into()),
        }
    }
    Err(last_error
        .unwrap_or_else(|| anyhow::anyhow!("could not find a resolver configuration file")))
}

#[cfg(windows)]
fn platform_dns_servers() -> Result<Vec<IpAddr>> {
    let mut adapters =
        ipconfig::get_adapters().context("read Windows network adapter DNS configuration")?;
    adapters.retain(|adapter| adapter.oper_status() == ipconfig::OperStatus::IfOperStatusUp);
    adapters.sort_by_key(|adapter| adapter.ipv4_metric().min(adapter.ipv6_metric()));
    Ok(adapters
        .into_iter()
        .flat_map(|adapter| adapter.dns_servers().to_vec())
        .collect())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn platform_dns_servers() -> Result<Vec<IpAddr>> {
    bail!("automatic system DNS discovery is not implemented for this operating system")
}

#[cfg(any(target_os = "linux", test))]
fn parse_resolv_conf(contents: &str) -> Vec<IpAddr> {
    contents
        .lines()
        .filter_map(|line| {
            let line = line.split('#').next().unwrap_or_default();
            let mut fields = line.split_whitespace();
            match (fields.next(), fields.next()) {
                (Some(keyword), Some(address)) if keyword.eq_ignore_ascii_case("nameserver") => {
                    address.parse::<IpAddr>().ok()
                }
                _ => None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, str::FromStr};

    use super::*;

    #[test]
    fn parses_nameservers_without_using_the_system_resolver() {
        let parsed = parse_resolv_conf(
            "# generated\nnameserver 10.0.0.53\nnameserver 2001:db8::53 # comment\nsearch example.test\n",
        );
        assert_eq!(
            parsed,
            vec![
                IpAddr::V4(Ipv4Addr::new(10, 0, 0, 53)),
                IpAddr::from_str("2001:db8::53").unwrap(),
            ]
        );
    }

    #[test]
    fn filters_loopback_listener_and_duplicate_addresses() {
        let listener = "127.0.0.1:53".parse().unwrap();
        let addresses = normalize_dns_servers(
            vec![
                "127.0.0.1".parse().unwrap(),
                "0.0.0.0".parse().unwrap(),
                "224.0.0.1".parse().unwrap(),
                "10.0.0.53".parse().unwrap(),
                "10.0.0.53".parse().unwrap(),
                "::1".parse().unwrap(),
            ],
            listener,
        );
        assert_eq!(addresses, vec!["10.0.0.53:53".parse().unwrap()]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recognizes_macos_tunnel_interface_names() {
        assert!(is_tunnel_interface("utun4"));
        assert!(is_tunnel_interface("ppp0"));
        assert!(!is_tunnel_interface("en7"));
    }
}
