use std::{
    collections::{HashSet, VecDeque},
    io,
    net::SocketAddr,
    sync::{Arc, OnceLock},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use hickory_proto::{
    op::{Message, MessageType, ResponseCode},
    serialize::binary::{BinDecodable, BinEncodable},
};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use rustls::{ClientConfig, RootCertStore, pki_types::ServerName};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    time::{Instant, timeout},
};
use tokio_rustls::TlsConnector;
use tracing::{debug, info, warn};

use crate::{
    config::{LayerConfig, LayerType},
    local_dns, plugins,
    state::{RuntimeConfig, SharedState},
};

pub async fn serve(state: SharedState) -> Result<()> {
    let address = state.runtime.load().config.listener.address;
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
        let permit = match state.query_permits.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                debug!(%peer, "dropping UDP DNS query because the in-flight limit is reached");
                continue;
            }
        };
        tokio::spawn(async move {
            let _permit = permit;
            if let Some(response) = process_packet(&packet, &state).await {
                if let Err(error) = socket.send_to(&response, peer).await {
                    debug!(%peer, %error, "could not send UDP DNS response");
                }
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
        let permit = state
            .query_permits
            .clone()
            .acquire_owned()
            .await
            .expect("query semaphore is never closed");
        let Some(response) = process_packet(&packet, state).await else {
            return Ok(());
        };
        drop(permit);
        let response_length =
            u16::try_from(response.len()).context("DNS response exceeds TCP frame limit")?;
        stream.write_u16(response_length).await?;
        stream.write_all(&response).await?;
        stream.flush().await?;
    }
}

async fn process_packet(packet: &[u8], state: &SharedState) -> Option<Vec<u8>> {
    let request = match Message::from_bytes(packet) {
        Ok(request) if request.message_type() == MessageType::Query => request,
        Ok(_) => {
            debug!("discarding a DNS response received on the listener");
            return None;
        }
        Err(error) => {
            debug!(%error, "discarding malformed DNS request");
            return None;
        }
    };
    let runtime = state.runtime.load_full();
    let ranges = state.cloudflare_ranges.load_full();
    let response =
        match query_layers(packet, &request, runtime.as_ref(), ranges.as_slice(), state).await {
            Ok(response) => response,
            Err(error) => {
                warn!(%error, "all DNS layers failed");
                return server_failure(&request);
            }
        };
    match response.to_bytes() {
        Ok(bytes) => Some(bytes),
        Err(error) => {
            warn!(%error, "could not encode DNS response");
            server_failure(&request)
        }
    }
}

fn server_failure(request: &Message) -> Option<Vec<u8>> {
    let mut response = Message::error_msg(request.id(), request.op_code(), ResponseCode::ServFail);
    response.add_queries(request.queries().iter().cloned());
    response.set_recursion_available(true);
    response.to_bytes().ok()
}

async fn query_layers(
    packet: &[u8],
    request: &Message,
    runtime: &RuntimeConfig,
    ranges: &[ipnet::IpNet],
    state: &SharedState,
) -> Result<Message> {
    let domain = (request.queries().len() == 1).then(|| request.queries()[0].name().to_utf8());
    let mut current = Some(runtime.config.select_layer(domain.as_deref()).to_owned());
    let deadline = Instant::now() + Duration::from_millis(runtime.config.request_timeout_ms);
    let mut interceptors = Vec::new();
    let mut last_error = None;

    while let Some(tag) = current {
        let layer =
            runtime.config.layer(&tag).cloned().ok_or_else(|| {
                anyhow!("layer {tag:?} disappeared from a validated configuration")
            })?;
        current = layer.fallback.clone();

        match layer.kind {
            LayerType::Interceptor => {
                interceptors.push(layer);
            }
            LayerType::Udp
            | LayerType::Tcp
            | LayerType::Doh
            | LayerType::Dot
            | LayerType::Local => {
                let result = query_network_layer(packet, request, &layer, state, deadline).await;
                match result {
                    Ok(mut response) => {
                        for interceptor in interceptors.iter().rev() {
                            let plugin_tag = interceptor
                                .plugin
                                .as_deref()
                                .expect("validated interceptor has a plugin");
                            let plugin = runtime
                                .config
                                .plugin(plugin_tag)
                                .expect("validated interceptor references a plugin");
                            let changed = plugins::intercept_response(
                                plugin,
                                &mut response,
                                ranges,
                                runtime.preferred(plugin_tag),
                            );
                            if changed {
                                debug!(layer = %interceptor.tag, plugin = %plugin_tag, "rewrote DNS response through interceptor");
                            }
                        }
                        return Ok(response);
                    }
                    Err(error) => {
                        debug!(layer = %layer.tag, %error, "DNS layer failed; trying fallback");
                        last_error = Some(error);
                    }
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("no network DNS layer is reachable from entry")))
}

async fn query_network_layer(
    packet: &[u8],
    request: &Message,
    layer: &LayerConfig,
    state: &SharedState,
    deadline: Instant,
) -> Result<Message> {
    match layer.kind {
        LayerType::Udp => {
            query_udp_layer(
                packet,
                request,
                layer.address(),
                layer_deadline(layer, deadline)?,
            )
            .await
        }
        LayerType::Tcp => {
            let bytes = tcp_exchange(
                packet,
                layer.address(),
                duration_until(layer_deadline(layer, deadline)?)?,
            )
            .await?;
            validate_upstream_response(request, &bytes)
        }
        LayerType::Doh => {
            let bytes = doh_exchange(
                packet,
                layer,
                state,
                duration_until(layer_deadline(layer, deadline)?)?,
            )
            .await?;
            validate_upstream_response(request, &bytes)
        }
        LayerType::Dot => {
            let bytes = dot_exchange(
                packet,
                layer,
                duration_until(layer_deadline(layer, deadline)?)?,
            )
            .await?;
            validate_upstream_response(request, &bytes)
        }
        LayerType::Local => local_exchange(packet, request, layer, state, deadline).await,
        LayerType::Interceptor => bail!("interceptor is not a network layer"),
    }
}

async fn query_udp_layer(
    packet: &[u8],
    request: &Message,
    address: SocketAddr,
    deadline: Instant,
) -> Result<Message> {
    let bytes = udp_exchange(packet, address, duration_until(deadline)?).await?;
    let response = validate_upstream_response(request, &bytes)?;
    if !response.truncated() {
        return Ok(response);
    }

    let bytes = tcp_exchange(packet, address, duration_until(deadline)?)
        .await
        .context("retry truncated UDP response over TCP")?;
    validate_upstream_response(request, &bytes)
        .context("truncated UDP response's TCP retry was invalid")
}

async fn local_exchange(
    packet: &[u8],
    request: &Message,
    layer: &LayerConfig,
    state: &SharedState,
    deadline: Instant,
) -> Result<Message> {
    let deadline = layer_deadline(layer, deadline)?;
    let observed = state.local_resolvers();
    let mut candidates: VecDeque<SocketAddr> = observed.addresses().iter().copied().collect();
    let mut known: HashSet<SocketAddr> = candidates.iter().copied().collect();
    let mut refreshed = false;
    let mut last_error = None;

    if candidates.is_empty() {
        let discovered = refresh_local_after_failure(state, &observed, deadline)
            .await
            .context("local resolver cache is empty")?;
        extend_local_candidates(&mut candidates, &mut known, discovered.addresses());
        refreshed = true;
    }

    while let Some(address) = candidates.pop_front() {
        let endpoint_deadline = allocated_endpoint_deadline(deadline, candidates.len() + 1);
        match query_udp_layer(packet, request, address, endpoint_deadline).await {
            Ok(response) => return Ok(response),
            Err(error) => {
                last_error = Some(anyhow!("local DNS server {address} failed: {error}"));
                if !refreshed {
                    refreshed = true;
                    match refresh_local_after_failure(state, &observed, deadline).await {
                        Ok(discovered) => {
                            extend_local_candidates(
                                &mut candidates,
                                &mut known,
                                discovered.addresses(),
                            );
                        }
                        Err(refresh_error) => {
                            debug!(%refresh_error, "could not rediscover system DNS after local resolver failure");
                        }
                    }
                }
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| anyhow!("system DNS discovery returned no usable local resolver")))
}

async fn refresh_local_after_failure(
    state: &SharedState,
    observed: &Arc<crate::state::LocalResolvers>,
    deadline: Instant,
) -> Result<Arc<crate::state::LocalResolvers>> {
    timeout(
        duration_until(deadline)?,
        local_dns::refresh_after_failure(state, observed),
    )
    .await
    .context("system DNS rediscovery timed out")?
}

fn extend_local_candidates(
    candidates: &mut VecDeque<SocketAddr>,
    known: &mut HashSet<SocketAddr>,
    discovered: &[SocketAddr],
) {
    for address in discovered {
        if known.insert(*address) {
            candidates.push_back(*address);
        }
    }
}

fn layer_deadline(layer: &LayerConfig, deadline: Instant) -> Result<Instant> {
    let now = Instant::now();
    let remaining = deadline.saturating_duration_since(now);
    if remaining.is_zero() {
        bail!("DNS request deadline expired");
    }
    Ok((now + Duration::from_millis(layer.timeout_ms())).min(deadline))
}

fn allocated_endpoint_deadline(deadline: Instant, targets_remaining: usize) -> Instant {
    let now = Instant::now();
    let remaining = deadline.saturating_duration_since(now);
    let divisor = u32::try_from(targets_remaining).unwrap_or(u32::MAX).max(1);
    now + remaining / divisor
}

fn duration_until(deadline: Instant) -> Result<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        bail!("DNS request deadline expired");
    }
    Ok(remaining)
}

fn validate_upstream_response(request: &Message, packet: &[u8]) -> Result<Message> {
    let response = Message::from_bytes(packet).context("decode upstream DNS response")?;
    if response.id() != request.id() {
        bail!(
            "upstream DNS response ID {} does not match request ID {}",
            response.id(),
            request.id()
        );
    }
    if response.message_type() != MessageType::Response {
        bail!("upstream returned a DNS query instead of a response");
    }
    if response.op_code() != request.op_code() {
        bail!("upstream DNS response opcode does not match the request");
    }
    if response.queries() != request.queries() {
        bail!("upstream DNS response question does not match the request");
    }
    Ok(response)
}

async fn udp_exchange(packet: &[u8], address: SocketAddr, duration: Duration) -> Result<Vec<u8>> {
    let bind_address = match address {
        SocketAddr::V4(_) => "0.0.0.0:0",
        SocketAddr::V6(_) => "[::]:0",
    };
    let response = timeout(duration, async {
        let socket = UdpSocket::bind(bind_address).await?;
        socket.connect(address).await?;
        socket.send(packet).await?;
        let mut response = vec![0_u8; u16::MAX as usize];
        let length = socket.recv(&mut response).await?;
        Ok::<_, io::Error>(response[..length].to_vec())
    })
    .await
    .context("UDP layer request timed out")??;
    Ok(response)
}

async fn tcp_exchange(packet: &[u8], address: SocketAddr, duration: Duration) -> Result<Vec<u8>> {
    let packet_length = u16::try_from(packet.len()).context("DNS query exceeds TCP frame limit")?;
    let response = timeout(duration, async {
        let mut stream = TcpStream::connect(address).await?;
        stream.write_u16(packet_length).await?;
        stream.write_all(packet).await?;
        stream.flush().await?;
        let response_length = stream.read_u16().await? as usize;
        let mut response = vec![0_u8; response_length];
        stream.read_exact(&mut response).await?;
        Ok::<_, io::Error>(response)
    })
    .await
    .context("TCP layer request timed out")??;
    Ok(response)
}

async fn dot_exchange(packet: &[u8], layer: &LayerConfig, duration: Duration) -> Result<Vec<u8>> {
    let packet_length = u16::try_from(packet.len()).context("DNS query exceeds TCP frame limit")?;
    let server_name = layer
        .server_name
        .as_ref()
        .context("DoT layer has no server_name")?;
    let server_name = ServerName::try_from(server_name.clone())
        .with_context(|| format!("invalid DoT server_name {server_name:?}"))?;
    let response = timeout(duration, async {
        let stream = TcpStream::connect(layer.address()).await?;
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
    .context("DoT layer request timed out")??;
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
    layer: &LayerConfig,
    state: &SharedState,
    duration: Duration,
) -> Result<Vec<u8>> {
    let client = state.doh_client(layer)?;
    let endpoint = layer.url.as_deref().context("DoH layer has no url")?;
    timeout(duration, async {
        let response = client
            .post(endpoint)
            .header(ACCEPT, "application/dns-message")
            .header(CONTENT_TYPE, "application/dns-message")
            .body(packet.to_vec())
            .send()
            .await
            .context("send DoH request")?;
        if !response.status().is_success() {
            bail!("DoH layer returned HTTP status {}", response.status());
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok());
        if !content_type.is_some_and(is_dns_message_content_type) {
            bail!("DoH layer response has no application/dns-message content type");
        }
        let body = response.bytes().await.context("read DoH response body")?;
        if body.is_empty() {
            bail!("DoH layer returned an empty DNS message");
        }
        Ok(body.to_vec())
    })
    .await
    .context("DoH layer request timed out")?
}

fn is_dns_message_content_type(value: &str) -> bool {
    value.split(';').next().is_some_and(|media_type| {
        media_type
            .trim()
            .eq_ignore_ascii_case("application/dns-message")
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use hickory_proto::{
        op::Query,
        rr::{Name, RecordType},
    };
    use tokio::net::UdpSocket;

    use super::*;
    use crate::{
        config::{FileConfig, KeywordMatch, LayerConfig},
        state::AppState,
    };

    fn request() -> Message {
        let mut message = Message::new();
        message.set_id(42);
        message.add_query(Query::query(
            Name::from_str("www.example.test.").unwrap(),
            RecordType::A,
        ));
        message
    }

    fn response_for(request: &Message) -> Message {
        let mut response = Message::new();
        response.set_id(request.id());
        response.set_message_type(MessageType::Response);
        response.add_queries(request.queries().iter().cloned());
        response
    }

    fn udp_layer(tag: &str, address: SocketAddr, fallback: Option<&str>) -> LayerConfig {
        LayerConfig {
            tag: tag.to_owned(),
            kind: LayerType::Udp,
            fallback: fallback.map(str::to_owned),
            matcher: KeywordMatch::default(),
            address: Some(address),
            timeout_ms: Some(500),
            refresh_secs: None,
            url: None,
            server_name: None,
            plugin: None,
        }
    }

    fn local_layer(tag: &str, fallback: Option<&str>) -> LayerConfig {
        LayerConfig {
            tag: tag.to_owned(),
            kind: LayerType::Local,
            fallback: fallback.map(str::to_owned),
            matcher: KeywordMatch::default(),
            address: None,
            timeout_ms: Some(500),
            refresh_secs: Some(30),
            url: None,
            server_name: None,
            plugin: None,
        }
    }

    fn two_layer_config(first: SocketAddr, second: SocketAddr) -> FileConfig {
        FileConfig {
            request_timeout_ms: 1_500,
            entry: "first".to_owned(),
            layers: vec![
                udp_layer("first", first, Some("second")),
                udp_layer("second", second, None),
            ],
            ..FileConfig::default()
        }
    }

    #[test]
    fn validates_matched_upstream_response() {
        let request = request();
        let response = response_for(&request).to_bytes().unwrap();
        assert!(validate_upstream_response(&request, &response).is_ok());
    }

    #[test]
    fn rejects_wrong_dns_response_id() {
        let request = request();
        let mut response = response_for(&request);
        response.set_id(43);
        assert!(validate_upstream_response(&request, &response.to_bytes().unwrap()).is_err());
    }

    #[test]
    fn rejects_dns_query_as_response() {
        let request = request();
        assert!(validate_upstream_response(&request, &request.to_bytes().unwrap()).is_err());
    }

    #[test]
    fn accepts_doh_dns_message_content_type() {
        assert!(is_dns_message_content_type("application/dns-message"));
        assert!(is_dns_message_content_type(
            "Application/Dns-Message; charset=binary"
        ));
        assert!(!is_dns_message_content_type("application/json"));
    }

    #[tokio::test]
    async fn falls_back_after_a_malformed_udp_response() {
        let first = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let first_address = first.local_addr().unwrap();
        let first_task = tokio::spawn(async move {
            let mut buffer = [0_u8; 512];
            let (_, peer) = first.recv_from(&mut buffer).await.unwrap();
            first.send_to(&[0_u8, 1], peer).await.unwrap();
        });

        let second = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let second_address = second.local_addr().unwrap();
        let second_task = tokio::spawn(async move {
            let mut buffer = [0_u8; 512];
            let (size, peer) = second.recv_from(&mut buffer).await.unwrap();
            let request = Message::from_bytes(&buffer[..size]).unwrap();
            second
                .send_to(&response_for(&request).to_bytes().unwrap(), peer)
                .await
                .unwrap();
        });

        let request = request();
        let packet = request.to_bytes().unwrap();
        let state = AppState::new(two_layer_config(first_address, second_address), Vec::new());
        let runtime = state.runtime.load_full();
        let ranges = state.cloudflare_ranges.load_full();
        let response = query_layers(
            &packet,
            &request,
            runtime.as_ref(),
            ranges.as_slice(),
            &state,
        )
        .await
        .unwrap();

        assert_eq!(response.id(), request.id());
        first_task.await.unwrap();
        second_task.await.unwrap();
    }

    #[tokio::test]
    async fn queries_the_cached_dynamic_local_resolver() {
        let upstream = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let upstream_address = upstream.local_addr().unwrap();
        let upstream_task = tokio::spawn(async move {
            let mut buffer = [0_u8; 512];
            let (size, peer) = upstream.recv_from(&mut buffer).await.unwrap();
            let request = Message::from_bytes(&buffer[..size]).unwrap();
            upstream
                .send_to(&response_for(&request).to_bytes().unwrap(), peer)
                .await
                .unwrap();
        });

        let config = FileConfig {
            request_timeout_ms: 1_500,
            entry: "local".to_owned(),
            layers: vec![local_layer("local", None)],
            ..FileConfig::default()
        };
        let state = AppState::new(config, Vec::new());
        state.replace_local_resolvers(vec![upstream_address]);
        let request = request();
        let packet = request.to_bytes().unwrap();
        let runtime = state.runtime.load_full();
        let ranges = state.cloudflare_ranges.load_full();
        let response = query_layers(
            &packet,
            &request,
            runtime.as_ref(),
            ranges.as_slice(),
            &state,
        )
        .await
        .unwrap();

        assert_eq!(response.id(), request.id());
        upstream_task.await.unwrap();
    }

    #[tokio::test]
    async fn valid_servfail_does_not_fall_back_to_the_next_layer() {
        let first = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let first_address = first.local_addr().unwrap();
        let first_task = tokio::spawn(async move {
            let mut buffer = [0_u8; 512];
            let (size, peer) = first.recv_from(&mut buffer).await.unwrap();
            let request = Message::from_bytes(&buffer[..size]).unwrap();
            let mut response = response_for(&request);
            response.set_response_code(ResponseCode::ServFail);
            first
                .send_to(&response.to_bytes().unwrap(), peer)
                .await
                .unwrap();
        });

        let second = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let second_address = second.local_addr().unwrap();
        let request = request();
        let packet = request.to_bytes().unwrap();
        let state = AppState::new(two_layer_config(first_address, second_address), Vec::new());
        let runtime = state.runtime.load_full();
        let ranges = state.cloudflare_ranges.load_full();
        let response = query_layers(
            &packet,
            &request,
            runtime.as_ref(),
            ranges.as_slice(),
            &state,
        )
        .await
        .unwrap();

        assert_eq!(response.response_code(), ResponseCode::ServFail);
        let mut buffer = [0_u8; 512];
        assert!(
            timeout(Duration::from_millis(100), second.recv_from(&mut buffer))
                .await
                .is_err()
        );
        first_task.await.unwrap();
    }
}
