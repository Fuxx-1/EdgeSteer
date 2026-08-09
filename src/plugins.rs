use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use hickory_proto::{
    op::Message,
    rr::{
        RData, Record, RecordType,
        rdata::{
            A, AAAA, HTTPS, SVCB,
            svcb::{IpHint, SvcParamValue},
        },
    },
};
use ipnet::IpNet;

use crate::{
    config::{PluginConfig, PluginType},
    state::PreferredIps,
};

#[derive(Clone, Copy)]
enum AddressFamily {
    Ipv4,
    Ipv6,
}

/// Applies a configured built-in response interceptor. The configuration only
/// names statically compiled plugins; it never loads executable code or a
/// dynamic library from the configuration file.
pub fn intercept_response(
    plugin: &PluginConfig,
    message: &mut Message,
    ranges: &[IpNet],
    preferred: Option<&PreferredIps>,
) -> bool {
    match plugin.kind {
        PluginType::CloudflarePreferred => preferred.is_some_and(|preferred| {
            rewrite_cloudflare_response(message, ranges, preferred, plugin.rewrite_ttl_secs)
        }),
    }
}

fn rewrite_cloudflare_response(
    message: &mut Message,
    ranges: &[IpNet],
    preferred: &PreferredIps,
    ttl: u32,
) -> bool {
    let mut changed = false;
    if let Some(ipv4) = preferred.ipv4 {
        if all_cloudflare_addresses(message.answers(), AddressFamily::Ipv4, ranges) {
            changed |= rewrite_ipv4_records(message.answers_mut(), ipv4, ttl);
        }
    }
    if let Some(ipv6) = preferred.ipv6 {
        if all_cloudflare_addresses(message.answers(), AddressFamily::Ipv6, ranges) {
            changed |= rewrite_ipv6_records(message.answers_mut(), ipv6, ttl);
        }
    }
    changed |= rewrite_svcb_records(message.answers_mut(), preferred, ranges, ttl);

    if changed {
        clear_dnssec_state(message);
    }
    changed
}

fn all_cloudflare_addresses(records: &[Record], family: AddressFamily, ranges: &[IpNet]) -> bool {
    let mut found = false;
    for record in records {
        let address = match (family, record.data()) {
            (AddressFamily::Ipv4, RData::A(A(address))) => IpAddr::V4(*address),
            (AddressFamily::Ipv6, RData::AAAA(AAAA(address))) => IpAddr::V6(*address),
            _ => continue,
        };
        if !contains_cloudflare_range(ranges, address) {
            return false;
        }
        found = true;
    }
    found
}

fn rewrite_ipv4_records(records: &mut [Record], preferred: Ipv4Addr, ttl: u32) -> bool {
    let mut changed = false;
    for record in records {
        let mut touched = false;
        if let RData::A(address) = record.data_mut() {
            if address.0 != preferred {
                *address = A::from(preferred);
                touched = true;
            }
            if record.ttl() != ttl {
                touched = true;
            }
            if touched {
                record.set_ttl(ttl);
                changed = true;
            }
        }
    }
    changed
}

fn rewrite_ipv6_records(records: &mut [Record], preferred: Ipv6Addr, ttl: u32) -> bool {
    let mut changed = false;
    for record in records {
        let mut touched = false;
        if let RData::AAAA(address) = record.data_mut() {
            if address.0 != preferred {
                *address = AAAA::from(preferred);
                touched = true;
            }
            if record.ttl() != ttl {
                touched = true;
            }
            if touched {
                record.set_ttl(ttl);
                changed = true;
            }
        }
    }
    changed
}

fn rewrite_svcb_records(
    records: &mut [Record],
    preferred: &PreferredIps,
    ranges: &[IpNet],
    ttl: u32,
) -> bool {
    let mut changed = false;
    for record in records {
        let replacement = match record.data() {
            RData::SVCB(svcb) => rewrite_svcb(svcb, preferred, ranges).map(RData::SVCB),
            RData::HTTPS(https) => rewrite_svcb(&https.0, preferred, ranges)
                .map(|replacement| RData::HTTPS(HTTPS(replacement))),
            _ => None,
        };
        if let Some(replacement) = replacement {
            let data_changed = record.data() != &replacement;
            let ttl_changed = record.ttl() != ttl;
            if data_changed {
                record.set_data(replacement);
            }
            if data_changed || ttl_changed {
                record.set_ttl(ttl);
                changed = true;
            }
        }
    }
    changed
}

fn rewrite_svcb(svcb: &SVCB, preferred: &PreferredIps, ranges: &[IpNet]) -> Option<SVCB> {
    let rewrite_ipv4 =
        preferred.ipv4.is_some() && all_cloudflare_svcb_hints(svcb, AddressFamily::Ipv4, ranges);
    let rewrite_ipv6 =
        preferred.ipv6.is_some() && all_cloudflare_svcb_hints(svcb, AddressFamily::Ipv6, ranges);
    if !rewrite_ipv4 && !rewrite_ipv6 {
        return None;
    }

    let params = svcb
        .svc_params()
        .iter()
        .map(|(key, value)| {
            let value = match value {
                SvcParamValue::Ipv4Hint(_) if rewrite_ipv4 => SvcParamValue::Ipv4Hint(IpHint(
                    vec![A::from(preferred.ipv4.expect("checked above"))],
                )),
                SvcParamValue::Ipv6Hint(_) if rewrite_ipv6 => SvcParamValue::Ipv6Hint(IpHint(
                    vec![AAAA::from(preferred.ipv6.expect("checked above"))],
                )),
                _ => value.clone(),
            };
            (*key, value)
        })
        .collect();
    Some(SVCB::new(
        svcb.svc_priority(),
        svcb.target_name().clone(),
        params,
    ))
}

fn all_cloudflare_svcb_hints(svcb: &SVCB, family: AddressFamily, ranges: &[IpNet]) -> bool {
    let mut found = false;
    for (_, value) in svcb.svc_params() {
        let addresses: Vec<IpAddr> = match (family, value) {
            (AddressFamily::Ipv4, SvcParamValue::Ipv4Hint(IpHint(addresses))) => addresses
                .iter()
                .map(|address| IpAddr::V4(address.0))
                .collect(),
            (AddressFamily::Ipv6, SvcParamValue::Ipv6Hint(IpHint(addresses))) => addresses
                .iter()
                .map(|address| IpAddr::V6(address.0))
                .collect(),
            _ => continue,
        };
        if addresses.is_empty()
            || addresses
                .iter()
                .any(|address| !contains_cloudflare_range(ranges, *address))
        {
            return false;
        }
        found = true;
    }
    found
}

fn contains_cloudflare_range(ranges: &[IpNet], address: IpAddr) -> bool {
    ranges.iter().any(|range| range.contains(&address))
}

fn clear_dnssec_state(message: &mut Message) {
    message.set_authentic_data(false);
    message
        .answers_mut()
        .retain(|record| record.record_type() != RecordType::RRSIG);
    message
        .name_servers_mut()
        .retain(|record| record.record_type() != RecordType::RRSIG);
    message
        .additionals_mut()
        .retain(|record| record.record_type() != RecordType::RRSIG);
    if let Some(edns) = message.extensions_mut().as_mut() {
        edns.set_dnssec_ok(false);
    }
}

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, str::FromStr};

    use hickory_proto::rr::{Name, rdata::svcb::SvcParamKey};

    use super::*;

    fn ranges() -> Vec<IpNet> {
        vec![IpNet::from_str("104.16.0.0/13").unwrap()]
    }

    fn plugin() -> PluginConfig {
        PluginConfig {
            tag: "preferred".to_owned(),
            kind: PluginType::CloudflarePreferred,
            rewrite_ttl_secs: 60,
            preferred: Default::default(),
            optimizer: Default::default(),
        }
    }

    #[test]
    fn rewrites_only_all_cloudflare_a_records() {
        let name = Name::from_str("example.com.").unwrap();
        let mut message = Message::new();
        message.add_answer(Record::from_rdata(
            name.clone(),
            300,
            RData::A(A::from(Ipv4Addr::new(104, 16, 1, 1))),
        ));
        message.add_answer(Record::from_rdata(
            name,
            300,
            RData::A(A::from(Ipv4Addr::new(104, 16, 1, 2))),
        ));
        let preferred = PreferredIps {
            ipv4: Some(Ipv4Addr::new(104, 16, 99, 1)),
            ipv6: None,
        };

        assert!(intercept_response(
            &plugin(),
            &mut message,
            &ranges(),
            Some(&preferred)
        ));
        for record in message.answers() {
            assert_eq!(record.ttl(), 60);
            assert!(matches!(
                record.data(),
                RData::A(A(address)) if *address == Ipv4Addr::new(104, 16, 99, 1)
            ));
        }
    }

    #[test]
    fn leaves_mixed_a_records_unchanged() {
        let name = Name::from_str("example.com.").unwrap();
        let mut message = Message::new();
        message.add_answer(Record::from_rdata(
            name.clone(),
            300,
            RData::A(A::from(Ipv4Addr::new(104, 16, 1, 1))),
        ));
        message.add_answer(Record::from_rdata(
            name,
            300,
            RData::A(A::from(Ipv4Addr::new(198, 51, 100, 1))),
        ));
        let preferred = PreferredIps {
            ipv4: Some(Ipv4Addr::new(104, 16, 99, 1)),
            ipv6: None,
        };

        assert!(!intercept_response(
            &plugin(),
            &mut message,
            &ranges(),
            Some(&preferred)
        ));
    }

    #[test]
    fn rewrites_https_ipv4_hints() {
        let name = Name::from_str("example.com.").unwrap();
        let svcb = SVCB::new(
            1,
            Name::root(),
            vec![(
                SvcParamKey::Ipv4Hint,
                SvcParamValue::Ipv4Hint(IpHint(vec![A::from(Ipv4Addr::new(104, 16, 1, 1))])),
            )],
        );
        let mut message = Message::new();
        message.add_answer(Record::from_rdata(name, 300, RData::HTTPS(HTTPS(svcb))));
        let preferred = PreferredIps {
            ipv4: Some(Ipv4Addr::new(104, 16, 99, 1)),
            ipv6: None,
        };

        assert!(intercept_response(
            &plugin(),
            &mut message,
            &ranges(),
            Some(&preferred)
        ));
        match message.answers()[0].data() {
            RData::HTTPS(https) => match &https.svc_params()[0].1 {
                SvcParamValue::Ipv4Hint(IpHint(addresses)) => {
                    assert_eq!(addresses.len(), 1);
                    assert_eq!(addresses[0].0, Ipv4Addr::new(104, 16, 99, 1));
                }
                other => panic!("unexpected SVCB value: {other:?}"),
            },
            other => panic!("unexpected record data: {other:?}"),
        }
    }

    #[test]
    fn no_preferred_address_is_a_successful_no_op() {
        let mut message = Message::new();
        assert!(!intercept_response(
            &plugin(),
            &mut message,
            &ranges(),
            None
        ));
    }
}
