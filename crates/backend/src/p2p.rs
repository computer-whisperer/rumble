use std::{collections::HashMap, path::PathBuf, str::FromStr, sync::Arc, time::Duration};

use anyhow::Context;
use async_trait::async_trait;
use blake3::Hasher;
use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use libp2p::{
    Multiaddr, PeerId, SwarmBuilder, dcutr, identify,
    identity::{self, Keypair},
    multiaddr::Protocol,
    noise, ping, relay,
    request_response::{self, Codec, ProtocolSupport, RequestId},
    swarm::{SwarmEvent, derive_prelude::NetworkBehaviour as DeriveNetworkBehaviour},
    tcp, yamux,
};
use tokio::{
    sync::{RwLock, mpsc, oneshot},
    task::JoinHandle,
};
use urlencoding::encode;

fn make_noise(kp: &identity::Keypair) -> Result<noise::Config, noise::Error> {
    noise::Config::new(kp)
}

/// Information about a shared file.
#[derive(Debug, Clone)]
pub struct SharedFile {
    pub id: [u8; 32],
    pub name: String,
    pub size: u64,
    pub path: PathBuf,
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

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> std::io::Result<FileRequest>
    where
        T: futures::AsyncRead + Unpin + Send,
    {
        let mut id = [0u8; 32];
        AsyncReadExt::read_exact(io, &mut id).await?;
        Ok(FileRequest(id))
    }

    async fn read_response<T>(&mut self, _: &Self::Protocol, io: &mut T) -> std::io::Result<FileResponse>
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

    async fn write_response<T>(&mut self, _: &Self::Protocol, io: &mut T, resp: FileResponse) -> std::io::Result<()>
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

#[derive(DeriveNetworkBehaviour)]
struct P2PBehaviour {
    request_response: request_response::Behaviour<FileCodec>,
    ping: ping::Behaviour,
    identify: identify::Behaviour,
    relay_client: relay::client::Behaviour,
    dcutr: dcutr::Behaviour,
}

enum Command {
    Listen(Multiaddr),
    Dial(Multiaddr),
    ShareFile(PathBuf, oneshot::Sender<anyhow::Result<SharedFile>>),
    FetchFile {
        peer: PeerId,
        file_id: [u8; 32],
        addr: Option<Multiaddr>,
        respond: oneshot::Sender<anyhow::Result<(String, Vec<u8>)>>,
    },
    Shutdown,
}

pub struct P2PManager {
    peer_id: PeerId,
    cmd_tx: mpsc::Sender<Command>,
    listen_addrs: Arc<RwLock<Vec<Multiaddr>>>,
    _task: JoinHandle<()>,
}

impl P2PManager {
    pub async fn spawn(
        keypair: Keypair,
        listen_addrs: Vec<Multiaddr>,
        relay_addr: Option<Multiaddr>,
    ) -> anyhow::Result<Self> {
        let peer_id = PeerId::from(keypair.public());
        let mut swarm = SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(tcp::Config::default().nodelay(true), make_noise, yamux::Config::default)?
            .with_relay_client(make_noise, yamux::Config::default)?
            .with_behaviour(|key, relay_client| {
                let identify =
                    identify::Behaviour::new(identify::Config::new("rumble-p2p/0.1".to_string(), key.public()));

                let mut cfg = request_response::Config::default();
                cfg.set_request_timeout(Duration::from_secs(20));
                let rr = request_response::Behaviour::new(
                    std::iter::once(("/rumble/file/1.0.0".to_string(), ProtocolSupport::Full)),
                    cfg,
                );

                P2PBehaviour {
                    request_response: rr,
                    ping: ping::Behaviour::new(ping::Config::default()),
                    identify,
                    relay_client,
                    dcutr: dcutr::Behaviour::new(peer_id),
                }
            })?
            .build();

        for addr in listen_addrs {
            let _ = libp2p::Swarm::listen_on(&mut swarm, addr);
        }

        if let Some(addr) = relay_addr.clone() {
            let circuit = addr.clone().with(Protocol::P2pCircuit);
            let _ = libp2p::Swarm::listen_on(&mut swarm, circuit);
            let _ = libp2p::Swarm::dial(&mut swarm, addr);
        }

        let (cmd_tx, mut cmd_rx) = mpsc::channel(32);
        let listen_addrs_state: Arc<RwLock<Vec<Multiaddr>>> = Arc::new(RwLock::new(Vec::new()));
        let listen_addrs_task = listen_addrs_state.clone();

        let task = tokio::spawn(async move {
            let mut shared_files: HashMap<[u8; 32], (SharedFile, Vec<u8>)> = HashMap::new();
            let mut pending: HashMap<RequestId, oneshot::Sender<anyhow::Result<(String, Vec<u8>)>>> = HashMap::new();

            loop {
                tokio::select! {
                    biased;
                    Some(cmd) = cmd_rx.recv() => {
                        match cmd {
                            Command::Listen(addr) => { let _ = libp2p::Swarm::listen_on(&mut swarm, addr); }
                            Command::Dial(addr) => { let _ = libp2p::Swarm::dial(&mut swarm, addr); }
                            Command::ShareFile(path, respond) => {
                                let res = async {
                                    let data = tokio::fs::read(&path).await?;
                                    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file").to_string();
                                    let mut hasher = Hasher::new();
                                    hasher.update(&data);
                                    let id: [u8; 32] = hasher.finalize().as_bytes().to_owned();
                                    let info = SharedFile { id, name: name.clone(), size: data.len() as u64, path: path.clone() };
                                    shared_files.insert(id, (info.clone(), data));
                                    Ok(info)
                                }.await;
                                let _ = respond.send(res);
                            }
                            Command::FetchFile { peer, file_id, addr, respond } => {
                                if let Some(addr) = addr.clone() {
                                    swarm.behaviour_mut().request_response.add_address(&peer, addr.clone());
                                    let _ = libp2p::Swarm::dial(&mut swarm, addr);
                                }
                                let req_id = swarm.behaviour_mut().request_response.send_request(&peer, FileRequest(file_id));
                                pending.insert(req_id, respond);
                            }
                            Command::Shutdown => break,
                        }
                    }
                    event = swarm.select_next_some() => {
                        match event {
                            SwarmEvent::Behaviour(P2PBehaviourEvent::RequestResponse(evt)) => {
                                match evt {
                                    request_response::Event::Message { peer: _, message } => {
                                        match message {
                                            request_response::Message::Request { request, channel, .. } => {
                                                let FileRequest(id) = request;
                                                if let Some((info, data)) = shared_files.get(&id) {
                                                    let resp = FileResponse { ok: true, name: info.name.clone(), data: data.clone() };
                                                    let _ = swarm.behaviour_mut().request_response.send_response(channel, resp);
                                                } else {
                                                    let _ = swarm.behaviour_mut().request_response.send_response(channel, FileResponse { ok: false, name: String::new(), data: vec![] });
                                                }
                                            }
                                            request_response::Message::Response { request_id, response } => {
                                                if let Some(tx) = pending.remove(&request_id) {
                                                    let _ = tx.send(if response.ok {
                                                        Ok((response.name, response.data))
                                                    } else {
                                                        Err(anyhow::anyhow!("file not found"))
                                                    });
                                                }
                                            }
                                        }
                                    }
                                    request_response::Event::OutboundFailure { request_id, error, .. } => {
                                        println!("outbound failure: {error:?}");
                                        if let Some(tx) = pending.remove(&request_id) {
                                            let _ = tx.send(Err(anyhow::anyhow!("request failed: {error}")));
                                        }
                                    }
                                    request_response::Event::ResponseSent { .. } => {}
                                    request_response::Event::InboundFailure { .. } => {}
                                }
                            }
                            SwarmEvent::NewListenAddr { address, .. } => {
                                tracing::info!(%address, "p2p listening");
                                let mut addrs = listen_addrs_task.write().await;
                                if !addrs.iter().any(|a| a == &address) {
                                    addrs.push(address);
                                }
                            }
                            SwarmEvent::OutgoingConnectionError { error, .. } => {
                                tracing::warn!(%error, "p2p dial error");
                                println!("dial error: {error}");
                            }
                            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                                tracing::debug!(%peer_id, "p2p connected");
                            }
                            SwarmEvent::Behaviour(P2PBehaviourEvent::Dcutr(evt)) => {
                                tracing::debug!(?evt, "dcutr event");
                            }
                            SwarmEvent::Behaviour(P2PBehaviourEvent::RelayClient(evt)) => {
                                tracing::debug!(?evt, "relay client");
                            }
                            _ => {}
                        }
                    }
                }
            }
        });

        Ok(Self {
            peer_id,
            cmd_tx,
            listen_addrs: listen_addrs_state,
            _task: task,
        })
    }

    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    pub async fn listen_addrs(&self) -> Vec<Multiaddr> {
        self.listen_addrs.read().await.clone()
    }

    pub async fn listen(&self, addr: Multiaddr) {
        let _ = self.cmd_tx.send(Command::Listen(addr)).await;
    }

    pub async fn dial(&self, addr: Multiaddr) {
        let _ = self.cmd_tx.send(Command::Dial(addr)).await;
    }

    pub async fn share_file(&self, path: PathBuf) -> anyhow::Result<SharedFile> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::ShareFile(path, tx))
            .await
            .context("share send")?;
        rx.await.context("share recv")?
    }

    pub async fn fetch_file(&self, peer: PeerId, file_id: [u8; 32]) -> anyhow::Result<(String, Vec<u8>)> {
        self.fetch_file_with_addr(peer, file_id, None).await
    }

    pub async fn fetch_file_with_addr(
        &self,
        peer: PeerId,
        file_id: [u8; 32],
        addr: Option<Multiaddr>,
    ) -> anyhow::Result<(String, Vec<u8>)> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::FetchFile {
                peer,
                file_id,
                addr,
                respond: tx,
            })
            .await
            .context("fetch send")?;
        rx.await.context("fetch recv")?
    }

    pub async fn shutdown(&self) {
        let _ = self.cmd_tx.send(Command::Shutdown).await;
    }
}

/// Build a Rumble-specific P2P magnet URI containing the peer and reachable multiaddrs.
pub fn build_p2p_magnet(peer: PeerId, file: &SharedFile, addrs: &[Multiaddr]) -> String {
    let mut uri = format!("rumblep2p://{}/{}", peer, hex::encode(file.id));

    if !addrs.is_empty() {
        let query = addrs
            .iter()
            .map(|addr| {
                // Ensure the peer component is present so dial attempts succeed.
                let addr_with_peer = if addr.iter().any(|p| matches!(p, Protocol::P2p(_))) {
                    addr.clone()
                } else {
                    addr.clone().with(Protocol::P2p(peer.into()))
                };

                format!("ma={}", encode(&addr_with_peer.to_string()))
            })
            .collect::<Vec<_>>()
            .join("&");
        uri.push('?');
        uri.push_str(&query);
    }

    uri
}

/// Parse a Rumble P2P magnet URI into peer id, file id, and multiaddrs.
pub fn parse_p2p_magnet(uri: &str) -> anyhow::Result<(PeerId, [u8; 32], Vec<Multiaddr>)> {
    let rest = uri
        .strip_prefix("rumblep2p://")
        .ok_or_else(|| anyhow::anyhow!("not a rumblep2p magnet"))?;

    let (head, query) = rest.split_once('?').unwrap_or((rest, ""));
    let mut head_parts = head.split('/');

    let peer_str = head_parts.next().ok_or_else(|| anyhow::anyhow!("missing peer id"))?;
    let peer = PeerId::from_str(peer_str).map_err(|e| anyhow::anyhow!("invalid peer id: {e}"))?;

    let file_hex = head_parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing file id"))?;

    let mut file_id = [0u8; 32];
    let decoded = hex::decode(file_hex)?;
    if decoded.len() != 32 {
        anyhow::bail!("file id must be 32 bytes");
    }
    file_id.copy_from_slice(&decoded);

    let mut addrs = Vec::new();
    for pair in query.split('&').filter(|s| !s.is_empty()) {
        let (key, raw_val) = pair
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid query pair"))?;
        if key != "ma" {
            continue;
        }

        let addr_str = urlencoding::decode(raw_val)
            .map_err(|e| anyhow::anyhow!("addr decode failed: {e}"))?
            .into_owned();
        let addr = Multiaddr::from_str(&addr_str).map_err(|e| anyhow::anyhow!("invalid multiaddr: {e}"))?;
        addrs.push(addr);
    }

    Ok((peer, file_id, addrs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity;

    fn make_peer() -> PeerId {
        let kp = identity::Keypair::generate_ed25519();
        PeerId::from(kp.public())
    }

    #[test]
    fn magnet_roundtrip_adds_peer_component() {
        let peer = make_peer();
        let shared = SharedFile {
            id: [1u8; 32],
            name: "foo.bin".into(),
            size: 10,
            path: PathBuf::from("/tmp/foo.bin"),
        };

        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/9000".parse().unwrap();
        let magnet = build_p2p_magnet(peer, &shared, &[addr.clone()]);

        let (parsed_peer, file_id, addrs) = parse_p2p_magnet(&magnet).unwrap();
        assert_eq!(peer, parsed_peer);
        assert_eq!(&[1u8; 32], &file_id);
        assert_eq!(1, addrs.len());

        // Ensure /p2p/<peer> was appended
        assert!(addrs[0].iter().any(|p| match p {
            Protocol::P2p(pid) => pid == peer,
            _ => false,
        }));
        assert!(addrs[0].to_string().starts_with("/ip4/127.0.0.1/tcp/9000"));
    }

    #[test]
    fn magnet_parses_multiple_addrs() {
        let peer = make_peer();
        let shared = SharedFile {
            id: [2u8; 32],
            name: "bar.bin".into(),
            size: 20,
            path: PathBuf::from("/tmp/bar.bin"),
        };

        let a1: Multiaddr = "/ip4/10.0.0.2/tcp/10000".parse().unwrap();
        let relay_peer = make_peer();
        let a2: Multiaddr = format!("/dns4/relay.example.com/tcp/443/p2p/{relay_peer}")
            .parse()
            .unwrap();

        let magnet = build_p2p_magnet(peer, &shared, &[a1.clone(), a2.clone()]);
        let (parsed_peer, file_id, addrs) = parse_p2p_magnet(&magnet).unwrap();

        assert_eq!(peer, parsed_peer);
        assert_eq!(&[2u8; 32], &file_id);
        assert_eq!(2, addrs.len());

        let addrs_set: std::collections::HashSet<String> = addrs.into_iter().map(|a| a.to_string()).collect();
        assert!(addrs_set.iter().any(|s| s.contains("10.0.0.2")));
        assert!(addrs_set.iter().any(|s| s.contains("relay.example.com")));
    }
}
