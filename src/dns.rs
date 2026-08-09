use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{Arc, OnceLock},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use hickory_proto::{
    op::{Message, ResponseCode},
    rr::{
        RData, Record, RecordType,
        rdata::{
            A, AAAA, HTTPS, SVCB,
            svcb::{IpHint, SvcParamValue},
        },
    },
    serialize::binary::{BinDecodable, BinEncodable},
};
use ipnet::IpNet;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use rustls::{ClientConfig, RootCertStore, pki_types::ServerName};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    time::timeout,
};
use tokio_rustls::TlsConnector;
use tracing::{debug, info, warn};

use crate::{
    config::{FileConfig, UpstreamConfig, UpstreamProtocol},
    state::{PreferredIps, SharedState},
};

#[derive(Clone, Copy)]
enum AddressFamily {
    Ipv4,
    Ipv6,
}

pub async fn serve(state: SharedState) -> Result<()> {
    let address = state.config.load().listener.address;
    let udp_socket = Arc::new(
        UdpSocket::bind(address)
            .await
            .with_context(|| format!("bind UDP listener on {address}"))?,
    );
    let tcp_listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("bind TCP listener on {address}"))?;

    info!(%address, "DNS proxy is listening on UDP and TCP");
    tokio::try_join!(
        serve_udp(udp_socket, state.clone()),
        serve_tcp(tcp_listener, state)
    )?;
    Ok(())
}

async fn serve_udp(socket: Arc<UdpSocket>, state: SharedState) -> Result<()> {
    let mut buffer = vec![0_u8; u16::MAX as usize];
    loop {
        let (size, peer) = socket.recv_from(&mut buffer).await?;
        let packet = buffer[..size].to_vec();
        let socket = socket.clone();
        let state = state.clone();
        tokio::spawn(async move {
            if let Some(response) = process_packet(&packet, &state).await
                && let Err(error) = socket.send_to(&response, peer).await
            {
                debug!(%peer, %error, "could not send UDP DNS response");
            }
        });
    }
}

async fn serve_tcp(listener: TcpListener, state: SharedState) -> Result<()> {
    loop {
        let (stream, peer) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_tcp_connection(stream, &state).await {
                debug!(%peer, %error, "TCP DNS connection ended with an error");
            }
        });
    }
}

async fn handle_tcp_connection(mut stream: TcpStream, state: &SharedState) -> Result<()> {
    loop {
        let packet_length = match stream.read_u16().await {
            Ok(length) => length as usize,
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if packet_length == 0 {
            return Ok(());
        }

        let mut packet = vec![0_u8; packet_length];
        stream.read_exact(&mut packet).await?;
        let Some(response) = process_packet(&packet, state).await else {
            return Ok(());
        };
        let response_length =
            u16::try_from(response.len()).context("DNS response exceeds TCP frame limit")?;
        stream.write_u16(response_length).await?;
        stream.write_all(&response).await?;
        stream.flush().await?;
    }
}

async fn process_packet(packet: &[u8], state: &SharedState) -> Option<Vec<u8>> {
    let request = match Message::from_bytes(packet) {
        Ok(request) => request,
        Err(error) => {
            debug!(%error, "discarding malformed DNS request");
            return None;
        }
    };
    let config = state.config.load_full();
    let raw_response = match query_upstreams(packet, &config, state).await {
        Ok(response) => response,
        Err(error) => {
            warn!(%error, "all DNS upstreams failed");
            return server_failure(&request);
        }
    };
    let mut response = match Message::from_bytes(&raw_response) {
        Ok(response) => response,
        Err(error) => {
            warn!(%error, "upstream returned an invalid DNS response; forwarding it unchanged");
            return Some(raw_response);
        }
    };

    let ranges = state.cloudflare_ranges.load_full();
    let preferred = state.preferred_ips.load_full();
    let changed = rewrite_response(
        &mut response,
        ranges.as_slice(),
        preferred.as_ref(),
        config.cloudflare.rewrite_ttl_secs,
    );
    if changed {
        debug!(preferred = ?preferred, "rewrote Cloudflare DNS response");
    }
    match response.to_bytes() {
        Ok(bytes) => Some(bytes),
        Err(error) => {
            warn!(%error, "could not encode rewritten DNS response; forwarding upstream response unchanged");
            Some(raw_response)
        }
    }
}

fn server_failure(request: &Message) -> Option<Vec<u8>> {
    let mut response = Message::error_msg(request.id(), request.op_code(), ResponseCode::ServFail);
    response.add_queries(request.queries().iter().cloned());
    response.set_recursion_available(true);
    response.to_bytes().ok()
}

async fn query_upstreams(
    packet: &[u8],
    config: &FileConfig,
    state: &SharedState,
) -> Result<Vec<u8>> {
    let mut last_error = None;
    for upstream in &config.upstreams {
        let result = match upstream.protocol {
            UpstreamProtocol::Udp => udp_exchange(packet, upstream).await,
            UpstreamProtocol::Tcp => tcp_exchange(packet, upstream).await,
            UpstreamProtocol::Doh => doh_exchange(packet, upstream, state).await,
            UpstreamProtocol::Dot => dot_exchange(packet, upstream).await,
        };
        match result {
            Ok(response) => {
                if upstream.protocol == UpstreamProtocol::Udp
                    && Message::from_bytes(&response).is_ok_and(|message| message.truncated())
                {
                    match tcp_exchange(packet, upstream).await {
                        Ok(response) => return Ok(response),
                        Err(error) => {
                            last_error =
                                Some(error.context("retry truncated UDP response over TCP"));
                            continue;
                        }
                    }
                }
                return Ok(response);
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("no DNS upstreams configured")))
}

async fn udp_exchange(packet: &[u8], upstream: &UpstreamConfig) -> Result<Vec<u8>> {
    let bind_address = match upstream.address {
        SocketAddr::V4(_) => "0.0.0.0:0",
        SocketAddr::V6(_) => "[::]:0",
    };
    let duration = Duration::from_millis(upstream.timeout_ms);
    let response = timeout(duration, async {
        let socket = UdpSocket::bind(bind_address).await?;
        socket.connect(upstream.address).await?;
        socket.send(packet).await?;
        let mut response = vec![0_u8; u16::MAX as usize];
        let length = socket.recv(&mut response).await?;
        Ok::<_, io::Error>(response[..length].to_vec())
    })
    .await
    .context("UDP upstream request timed out")??;
    Ok(response)
}

async fn tcp_exchange(packet: &[u8], upstream: &UpstreamConfig) -> Result<Vec<u8>> {
    let packet_length = u16::try_from(packet.len()).context("DNS query exceeds TCP frame limit")?;
    let duration = Duration::from_millis(upstream.timeout_ms);
    let response = timeout(duration, async {
        let mut stream = TcpStream::connect(upstream.address).await?;
        stream.write_u16(packet_length).await?;
        stream.write_all(packet).await?;
        stream.flush().await?;
        let response_length = stream.read_u16().await? as usize;
        let mut response = vec![0_u8; response_length];
        stream.read_exact(&mut response).await?;
        Ok::<_, io::Error>(response)
    })
    .await
    .context("TCP upstream request timed out")??;
    Ok(response)
}

async fn dot_exchange(packet: &[u8], upstream: &UpstreamConfig) -> Result<Vec<u8>> {
    let packet_length = u16::try_from(packet.len()).context("DNS query exceeds TCP frame limit")?;
    let server_name = upstream
        .server_name
        .as_ref()
        .context("DoT upstream has no server_name")?;
    let server_name = ServerName::try_from(server_name.clone())
        .with_context(|| format!("invalid DoT server_name {server_name:?}"))?;
    let duration = Duration::from_millis(upstream.timeout_ms);
    let response = timeout(duration, async {
        let stream = TcpStream::connect(upstream.address).await?;
        let mut stream = dot_tls_connector().connect(server_name, stream).await?;
        stream.write_u16(packet_length).await?;
        stream.write_all(packet).await?;
        stream.flush().await?;
        let response_length = stream.read_u16().await? as usize;
        let mut response = vec![0_u8; response_length];
        stream.read_exact(&mut response).await?;
        Ok::<_, anyhow::Error>(response)
    })
    .await
    .context("DoT upstream request timed out")??;
    Ok(response)
}

fn dot_tls_connector() -> TlsConnector {
    static CONNECTOR: OnceLock<TlsConnector> = OnceLock::new();
    CONNECTOR
        .get_or_init(|| {
            crate::install_rustls_crypto_provider();
            let mut roots = RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let config = ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            TlsConnector::from(Arc::new(config))
        })
        .clone()
}

async fn doh_exchange(
    packet: &[u8],
    upstream: &UpstreamConfig,
    state: &SharedState,
) -> Result<Vec<u8>> {
    let client = state.doh_client(upstream)?;
    let endpoint = upstream.url.as_deref().context("DoH upstream has no url")?;
    let response = client
        .post(endpoint)
        .header(ACCEPT, "application/dns-message")
        .header(CONTENT_TYPE, "application/dns-message")
        .body(packet.to_vec())
        .send()
        .await
        .context("send DoH request")?
        .error_for_status()
        .context("DoH upstream returned an error status")?;
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    if !content_type.is_some_and(is_dns_message_content_type) {
        bail!("DoH upstream response has no application/dns-message content type");
    }
    let body = response.bytes().await.context("read DoH response body")?;
    if body.is_empty() {
        bail!("DoH upstream returned an empty DNS message");
    }
    Ok(body.to_vec())
}

fn is_dns_message_content_type(value: &str) -> bool {
    value.split(';').next().is_some_and(|media_type| {
        media_type
            .trim()
            .eq_ignore_ascii_case("application/dns-message")
    })
}

fn rewrite_response(
    message: &mut Message,
    ranges: &[IpNet],
    preferred: &PreferredIps,
    ttl: u32,
) -> bool {
    let mut changed = false;
    if let Some(ipv4) = preferred.ipv4
        && all_cloudflare_addresses(message.answers(), AddressFamily::Ipv4, ranges)
    {
        changed |= rewrite_ipv4_records(message.answers_mut(), ipv4, ttl);
    }
    if let Some(ipv6) = preferred.ipv6
        && all_cloudflare_addresses(message.answers(), AddressFamily::Ipv6, ranges)
    {
        changed |= rewrite_ipv6_records(message.answers_mut(), ipv6, ttl);
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

        assert!(rewrite_response(&mut message, &ranges(), &preferred, 60));
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

        assert!(!rewrite_response(&mut message, &ranges(), &preferred, 60));
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

        assert!(rewrite_response(&mut message, &ranges(), &preferred, 60));
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
    fn accepts_doh_dns_message_content_type() {
        assert!(is_dns_message_content_type("application/dns-message"));
        assert!(is_dns_message_content_type(
            "Application/Dns-Message; charset=binary"
        ));
        assert!(!is_dns_message_content_type("application/json"));
    }
}
