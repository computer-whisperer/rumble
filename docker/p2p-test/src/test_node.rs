//! Test node for P2P NAT traversal testing.
//!
//! This node can act as a file sharer or fetcher and demonstrates:
//! - IPv6 direct connections (priority 1)
//! - QUIC UDP hole punching via DCUtR (priority 2)
//! - Relay/tunnel fallback (priority 3)
//!
//! Connection priority:
//! 1. IPv6 direct - no NAT traversal needed
//! 2. QUIC hole punch - UDP-based, better success with NAT
//! 3. Relay tunnel - always works as fallback

use std::{collections::HashMap, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::{Parser, Subcommand};
use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use libp2p::{
    dcutr, identify,
    identity::{self, Keypair},
    multiaddr::Protocol,
    noise, ping, relay,
    request_response::{self, Codec, ProtocolSupport},
    swarm::{dial_opts::DialOpts, NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, SwarmBuilder,
};
use tracing::{debug, info, warn};

#[derive(Parser, Debug)]
#[command(author, version, about = "P2P Test Node for NAT traversal testing")]
struct Args {
    /// Node name for logging
    #[arg(short, long, default_value = "node")]
    name: String,

    /// Port to listen on (0 for random)
    #[arg(short, long, default_value = "0")]
    port: u16,

    /// Relay server multiaddr (e.g., /ip4/10.0.0.10/tcp/4001/p2p/<peer_id>)
    #[arg(short, long)]
    relay: Option<String>,

    /// Whether to listen via the relay circuit
    #[arg(long, default_value = "false")]
    relay_listen: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Share a file and wait for fetch requests
    Share {
        /// Path to file to share
        #[arg(short, long)]
        file: PathBuf,
    },
    /// Fetch a file from a peer
    Fetch {
        /// Target peer multiaddr
        #[arg(short, long)]
        target: String,
        /// File ID (hex-encoded 32 bytes)
        #[arg(short, long)]
        file_id: String,
    },
    /// Just connect to relay and wait (for testing connectivity)
    Wait,
}

/// State for fetch retry logic
struct FetchState {
    target_addr: Multiaddr,
    target_peer: PeerId,
    file_id: [u8; 32],
    retries: u32,
}

/// Request payload: 32-byte file id.
#[derive(Clone, Debug)]
struct FileRequest(pub [u8; 32]);

/// Response payload: ok flag + name + raw bytes.
#[derive(Clone, Debug)]
struct FileResponse {
    ok: bool,
    name: String,
    data: Vec<u8>,
}

#[derive(Clone, Default)]
struct FileCodec;

#[async_trait]
impl Codec for FileCodec {
    type Protocol = String;
    type Request = FileRequest;
    type Response = FileResponse;

    async fn read_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> std::io::Result<FileRequest>
    where
        T: futures::AsyncRead + Unpin + Send,
    {
        let mut id = [0u8; 32];
        AsyncReadExt::read_exact(io, &mut id).await?;
        Ok(FileRequest(id))
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> std::io::Result<FileResponse>
    where
        T: futures::AsyncRead + Unpin + Send,
    {
        let mut header = [0u8; 11];
        AsyncReadExt::read_exact(io, &mut header).await?;
        let ok = header[0] == 1;
        let name_len = u16::from_be_bytes([header[1], header[2]]) as usize;
        let data_len = u64::from_be_bytes(header[3..11].try_into().unwrap()) as usize;
        let mut name_buf = vec![0u8; name_len];
        AsyncReadExt::read_exact(io, &mut name_buf).await?;
        let mut data = vec![0u8; data_len];
        AsyncReadExt::read_exact(io, &mut data).await?;
        let name = String::from_utf8(name_buf).unwrap_or_default();
        Ok(FileResponse { ok, name, data })
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        FileRequest(id): FileRequest,
    ) -> std::io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        AsyncWriteExt::write_all(io, &id).await
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        resp: FileResponse,
    ) -> std::io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        let mut header = [0u8; 11];
        header[0] = if resp.ok { 1 } else { 0 };
        let name_bytes = resp.name.as_bytes();
        header[1..3].copy_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        header[3..11].copy_from_slice(&(resp.data.len() as u64).to_be_bytes());
        AsyncWriteExt::write_all(io, &header).await?;
        AsyncWriteExt::write_all(io, name_bytes).await?;
        AsyncWriteExt::write_all(io, &resp.data).await
    }
}

#[derive(NetworkBehaviour)]
struct TestNodeBehaviour {
    request_response: request_response::Behaviour<FileCodec>,
    ping: ping::Behaviour,
    identify: identify::Behaviour,
    relay_client: relay::client::Behaviour,
    /// DCUtR (Direct Connection Upgrade through Relay) for hole punching.
    /// When a relayed connection is established, DCUtR will attempt to upgrade
    /// to a direct connection via hole punching. If it fails, the relay connection
    /// remains available as fallback.
    dcutr: dcutr::Behaviour,
}

fn make_noise(kp: &identity::Keypair) -> Result<noise::Config, noise::Error> {
    noise::Config::new(kp)
}

/// Information about a shared file.
#[derive(Debug, Clone)]
struct SharedFile {
    id: [u8; 32],
    name: String,
    data: Vec<u8>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,libp2p=debug".parse().unwrap()),
        )
        .init();

    let args = Args::parse();

    let keypair = Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());

    info!(name = %args.name, %peer_id, "Test node starting");

    // Build the swarm with TCP + QUIC transports for better hole punching
    let mut swarm = SwarmBuilder::with_existing_identity(keypair.clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            make_noise,
            yamux::Config::default,
        )?
        .with_quic()
        .with_relay_client(make_noise, yamux::Config::default)?
        .with_behaviour(|key, relay_client| {
            let local_peer_id = PeerId::from(key.public());
            let identify_config =
                identify::Config::new("rumble-test-node/1.0.0".to_string(), key.public());

            let rr_config = request_response::Config::default()
                .with_request_timeout(Duration::from_secs(30));

            TestNodeBehaviour {
                request_response: request_response::Behaviour::new(
                    std::iter::once(("/rumble/file/1.0.0".to_string(), ProtocolSupport::Full)),
                    rr_config,
                ),
                ping: ping::Behaviour::new(ping::Config::default()),
                identify: identify::Behaviour::new(identify_config),
                relay_client,
                dcutr: dcutr::Behaviour::new(local_peer_id),
            }
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    // Listen on TCP and QUIC (IPv4 and IPv6)
    // For hole punching to work, we need to listen on addresses that peers can try to connect to
    let listen_port = if args.port > 0 { args.port } else { 0 };

    // IPv4 TCP
    let listen_addr_tcp4: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", listen_port).parse()?;
    swarm.listen_on(listen_addr_tcp4)?;

    // IPv4 QUIC (UDP) - better for hole punching
    let listen_addr_quic4: Multiaddr = format!("/ip4/0.0.0.0/udp/{}/quic-v1", listen_port).parse()?;
    swarm.listen_on(listen_addr_quic4)?;

    // IPv6 TCP (if available)
    let listen_addr_tcp6: Multiaddr = format!("/ip6/::/tcp/{}", listen_port).parse()?;
    if let Err(e) = swarm.listen_on(listen_addr_tcp6) {
        debug!(%e, "IPv6 TCP not available");
    }

    // IPv6 QUIC (if available)
    let listen_addr_quic6: Multiaddr = format!("/ip6/::/udp/{}/quic-v1", listen_port).parse()?;
    if let Err(e) = swarm.listen_on(listen_addr_quic6) {
        debug!(%e, "IPv6 QUIC not available");
    }

    // Parse relay address if specified
    let relay_info: Option<(Multiaddr, PeerId)> = if let Some(relay_addr_str) = &args.relay {
        let relay_addr: Multiaddr = relay_addr_str.parse()?;
        // Extract relay peer ID from address
        let relay_peer_id = relay_addr
            .iter()
            .find_map(|p| {
                if let Protocol::P2p(peer) = p {
                    Some(peer)
                } else {
                    None
                }
            })
            .context("relay address must contain peer ID")?;
        info!(%relay_addr, "Dialing relay");
        swarm.dial(relay_addr.clone())?;
        Some((relay_addr, relay_peer_id))
    } else {
        None
    };

    // Track if we need to start relay listening after connection
    let mut relay_listen_pending = args.relay_listen;

    // Shared files storage
    let mut shared_files: HashMap<[u8; 32], SharedFile> = HashMap::new();

    // Fetch state for retry logic
    let mut fetch_state: Option<FetchState> = None;

    // Handle commands
    match &args.command {
        Command::Share { file } => {
            let data = std::fs::read(file).context("read file")?;
            let name = file
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("file")
                .to_string();

            let mut hasher = blake3::Hasher::new();
            hasher.update(&data);
            let id: [u8; 32] = *hasher.finalize().as_bytes();

            let shared = SharedFile {
                id,
                name: name.clone(),
                data: data.clone(),
            };
            shared_files.insert(id, shared);

            println!("\n========================================");
            println!("SHARING FILE");
            println!("========================================");
            println!("File: {}", name);
            println!("Size: {} bytes", data.len());
            println!("File ID: {}", hex::encode(id));
            println!("Peer ID: {}", peer_id);
            println!("========================================\n");

            // Output as JSON for scripting
            let info = serde_json::json!({
                "file_id": hex::encode(id),
                "file_name": name,
                "file_size": data.len(),
                "peer_id": peer_id.to_string(),
            });
            println!("JSON: {}", serde_json::to_string(&info)?);
        }
        Command::Fetch { target, file_id } => {
            let target_addr: Multiaddr = target.parse()?;
            let file_id_bytes = hex::decode(file_id)?;
            if file_id_bytes.len() != 32 {
                anyhow::bail!("file_id must be 32 bytes");
            }
            let mut id = [0u8; 32];
            id.copy_from_slice(&file_id_bytes);

            // Extract peer ID from target address - for circuit addresses, we need the LAST P2p component
            // which is the actual target, not the relay
            let target_peer = target_addr
                .iter()
                .filter_map(|p| {
                    if let Protocol::P2p(peer) = p {
                        Some(peer)
                    } else {
                        None
                    }
                })
                .last()
                .context("target must contain peer ID")?;

            info!(%target_addr, %target_peer, "Will fetch file after relay connection is established");

            // Store fetch state - actual dial and request will happen after relay connection
            // is established (in ConnectionEstablished handler)
            fetch_state = Some(FetchState {
                target_addr,
                target_peer,
                file_id: id,
                retries: 0,
            });
        }
        Command::Wait => {
            info!("Waiting for connections...");
        }
    }

    // Event loop
    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => {
                let full_addr = format!("{}/p2p/{}", address, peer_id);
                info!(%address, "Listening on");
                println!("LISTEN_ADDR: {}", full_addr);
            }
            SwarmEvent::ConnectionEstablished {
                peer_id: remote_peer,
                endpoint,
                ..
            } => {
                info!(%remote_peer, ?endpoint, "Connection established");

                // Check if this is a relayed connection
                let addr = endpoint.get_remote_address();
                if addr.iter().any(|p| matches!(p, Protocol::P2pCircuit)) {
                    info!(%remote_peer, "Connected via relay circuit");
                }

                // If we connected to the relay and need to listen, do it now
                if relay_listen_pending {
                    if let Some((ref relay_addr, relay_peer_id)) = relay_info {
                        if remote_peer == relay_peer_id {
                            let circuit_addr = relay_addr.clone().with(Protocol::P2pCircuit);
                            info!(%circuit_addr, "Starting relay circuit listener");
                            if let Err(e) = swarm.listen_on(circuit_addr) {
                                warn!(%e, "Failed to listen on relay circuit");
                            }
                            relay_listen_pending = false;
                        }
                    }
                }

                // If we have a pending fetch and just connected to the relay, start the fetch
                if let Some(ref state) = fetch_state {
                    if state.retries == 0 {
                        // Check if this is the relay connection (first peer in circuit address)
                        if let Some((_, relay_peer_id)) = relay_info {
                            if remote_peer == relay_peer_id {
                                info!("Relay connection established, now dialing target via circuit");
                                // Give the relay a moment to be ready
                                tokio::time::sleep(Duration::from_millis(500)).await;

                                // Now dial the target via circuit
                                let dial_opts = DialOpts::peer_id(state.target_peer)
                                    .addresses(vec![state.target_addr.clone()])
                                    .build();
                                if let Err(e) = swarm.dial(dial_opts) {
                                    warn!(%e, "Failed to dial target via circuit");
                                }
                            }
                        }
                    }
                }

                // If we established a circuit connection to the target, wait for DCUtR then send file request
                if let Some(ref state) = fetch_state {
                    if remote_peer == state.target_peer {
                        // Check if this is a relayed or direct connection
                        let is_relayed = addr.iter().any(|p| matches!(p, Protocol::P2pCircuit));
                        if is_relayed {
                            info!(
                                %remote_peer,
                                "Connected to target via relay circuit, waiting for DCUtR hole punch attempt..."
                            );
                            // Give DCUtR 5 seconds to attempt hole punching
                            // The file request will be sent after DCUtR succeeds or fails
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            info!("DCUtR timeout reached, sending file request over current connection");
                        } else {
                            info!(%remote_peer, "Direct connection to target (hole punch may have succeeded!)");
                        }

                        swarm.add_peer_address(state.target_peer, state.target_addr.clone());
                        let _req_id = swarm
                            .behaviour_mut()
                            .request_response
                            .send_request(&state.target_peer, FileRequest(state.file_id));
                        info!("File request sent");
                    }
                }
            }
            SwarmEvent::ConnectionClosed {
                peer_id: remote_peer,
                cause,
                ..
            } => {
                info!(%remote_peer, ?cause, "Connection closed");
            }
            SwarmEvent::Behaviour(TestNodeBehaviourEvent::RequestResponse(event)) => {
                match event {
                    request_response::Event::Message { peer, message } => match message {
                        request_response::Message::Request {
                            request, channel, ..
                        } => {
                            let FileRequest(id) = request;
                            info!(%peer, file_id = %hex::encode(id), "Received file request");

                            if let Some(file) = shared_files.get(&id) {
                                let resp = FileResponse {
                                    ok: true,
                                    name: file.name.clone(),
                                    data: file.data.clone(),
                                };
                                let _ = swarm
                                    .behaviour_mut()
                                    .request_response
                                    .send_response(channel, resp);
                                info!(%peer, "Sent file response");
                            } else {
                                let _ = swarm.behaviour_mut().request_response.send_response(
                                    channel,
                                    FileResponse {
                                        ok: false,
                                        name: String::new(),
                                        data: vec![],
                                    },
                                );
                                warn!(%peer, "File not found");
                            }
                        }
                        request_response::Message::Response {
                            request_id: _,
                            response,
                        } => {
                            if response.ok {
                                println!("\n========================================");
                                println!("FILE RECEIVED");
                                println!("========================================");
                                println!("Name: {}", response.name);
                                println!("Size: {} bytes", response.data.len());
                                println!("Content: {}", String::from_utf8_lossy(&response.data));
                                println!("========================================\n");

                                // Output as JSON
                                let info = serde_json::json!({
                                    "status": "success",
                                    "file_name": response.name,
                                    "file_size": response.data.len(),
                                    "content": String::from_utf8_lossy(&response.data),
                                });
                                println!("JSON: {}", serde_json::to_string(&info)?);

                                // Exit after successful fetch
                                return Ok(());
                            } else {
                                println!("FILE_NOT_FOUND");
                                return Ok(());
                            }
                        }
                    },
                    request_response::Event::OutboundFailure { error, .. } => {
                        warn!(%error, "Outbound request failed");

                        // Retry logic for fetch
                        if let Some(ref mut state) = fetch_state {
                            if state.retries < 5 {
                                state.retries += 1;
                                info!(retry = state.retries, "Will retry file fetch after delay");

                                // Give time for connection to settle, then retry
                                tokio::time::sleep(Duration::from_secs(2)).await;

                                // Add address and send request again (connection should already exist)
                                swarm.add_peer_address(state.target_peer, state.target_addr.clone());
                                let _req_id = swarm
                                    .behaviour_mut()
                                    .request_response
                                    .send_request(&state.target_peer, FileRequest(state.file_id));

                                info!(retry = state.retries, "File request resent");
                            } else {
                                warn!("Max retries reached, giving up");
                                std::process::exit(1);
                            }
                        }
                    }
                    request_response::Event::InboundFailure { error, .. } => {
                        warn!(%error, "Inbound request failed");
                    }
                    request_response::Event::ResponseSent { .. } => {}
                }
            }
            SwarmEvent::Behaviour(TestNodeBehaviourEvent::RelayClient(event)) => {
                match &event {
                    relay::client::Event::ReservationReqAccepted { relay_peer_id, .. } => {
                        info!(%relay_peer_id, "Relay reservation accepted");
                        println!("RELAY_RESERVATION_ACCEPTED: {}", relay_peer_id);
                    }
                    relay::client::Event::OutboundCircuitEstablished {
                        relay_peer_id,
                        limit,
                    } => {
                        info!(%relay_peer_id, ?limit, "Outbound circuit established");
                    }
                    relay::client::Event::InboundCircuitEstablished { src_peer_id, .. } => {
                        info!(%src_peer_id, "Inbound circuit established");
                    }
                }
            }
            SwarmEvent::Behaviour(TestNodeBehaviourEvent::Identify(event)) => {
                if let identify::Event::Received {
                    peer_id: remote_peer,
                    info,
                    ..
                } = event
                {
                    info!(
                        %remote_peer,
                        agent = %info.agent_version,
                        listen_addrs = ?info.listen_addrs,
                        observed_addr = %info.observed_addr,
                        "Identified peer"
                    );

                    // Add observed addresses to help DCUtR hole punching
                    for addr in &info.listen_addrs {
                        swarm.add_peer_address(remote_peer, addr.clone());
                    }

                    // Add our observed external address (crucial for hole punching)
                    // The relay tells us what our external address looks like
                    swarm.add_external_address(info.observed_addr.clone());
                    info!(observed = %info.observed_addr, "Added external address from identify");
                }
            }
            SwarmEvent::Behaviour(TestNodeBehaviourEvent::Dcutr(event)) => {
                // DCUtR event contains remote_peer_id and result (Ok(ConnectionId) or Err)
                let dcutr::Event { remote_peer_id, result } = event;
                match result {
                    Ok(connection_id) => {
                        info!(
                            %remote_peer_id,
                            ?connection_id,
                            "HOLE PUNCH SUCCEEDED - direct connection established!"
                        );
                        println!("HOLEPUNCH_SUCCESS: {}", remote_peer_id);
                    }
                    Err(error) => {
                        warn!(
                            %remote_peer_id,
                            %error,
                            "Hole punch failed - continuing with relay connection"
                        );
                        println!("HOLEPUNCH_FAILED: {} - {}", remote_peer_id, error);
                        // Note: The relay connection should remain active as fallback
                    }
                }
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                warn!(?peer_id, %error, "Outgoing connection error");
            }
            SwarmEvent::IncomingConnectionError { error, .. } => {
                warn!(%error, "Incoming connection error");
            }
            _ => {}
        }
    }
}
