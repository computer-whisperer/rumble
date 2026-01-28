#![cfg(feature = "p2p")]

use std::{
    net::TcpListener,
    time::{Duration, Instant},
};

use backend::p2p::P2PManager;
use futures::StreamExt;
use libp2p::{Multiaddr, PeerId, SwarmBuilder, identity::Keypair, multiaddr::Protocol};
use tempfile::tempdir;
use tokio::{sync::oneshot, time::timeout};

fn make_noise(kp: &Keypair) -> Result<libp2p::noise::Config, libp2p::noise::Error> {
    libp2p::noise::Config::new(kp)
}

fn free_tcp_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn tcp_multiaddr(port: u16) -> Multiaddr {
    Multiaddr::empty()
        .with(Protocol::Ip4([127, 0, 0, 1].into()))
        .with(Protocol::Tcp(port))
}

async fn wait_for_circuit(node: &P2PManager) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        let addrs = node.listen_addrs().await;
        if addrs.iter().any(|a| a.to_string().contains("p2p-circuit")) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    println!("relay circuit address not ready after wait");
}

async fn start_relay() -> anyhow::Result<(PeerId, Multiaddr, tokio::task::JoinHandle<()>)> {
    use libp2p::{relay, swarm::derive_prelude::NetworkBehaviour, tcp, yamux};

    #[derive(NetworkBehaviour)]
    struct RelayBehaviour {
        relay: relay::Behaviour,
    }

    let key = Keypair::generate_ed25519();
    let peer_id = PeerId::from(key.public());
    let mut swarm = SwarmBuilder::with_existing_identity(key.clone())
        .with_tokio()
        .with_tcp(tcp::Config::default().nodelay(true), make_noise, || {
            yamux::Config::default()
        })?
        .with_behaviour(|kp| {
            let local = PeerId::from(kp.public());
            RelayBehaviour {
                relay: relay::Behaviour::new(local, relay::Config::default()),
            }
        })?
        .build();
    let port = free_tcp_port();
    let addr = tcp_multiaddr(port);
    libp2p::Swarm::listen_on(&mut swarm, addr.clone()).expect("listen");

    let (ready_tx, ready_rx) = oneshot::channel();
    let handle = tokio::spawn(async move {
        let mut ready_tx = Some(ready_tx);

        loop {
            if let Some(event) = swarm.next().await {
                if let libp2p::swarm::SwarmEvent::NewListenAddr { address, .. } = event {
                    if let Some(tx) = ready_tx.take() {
                        let _ = tx.send(address.clone());
                    }
                    tracing::info!(%address, "relay listening");
                }
            }
        }
    });

    let listen_addr = ready_rx.await.unwrap_or(addr);
    Ok((peer_id, listen_addr, handle))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn file_transfer_direct() {
    let dir = tempdir().unwrap();
    let src_path = dir.path().join("hello.txt");
    tokio::fs::write(&src_path, b"hello p2p").await.unwrap();

    let port_b = free_tcp_port();
    let listen_b = tcp_multiaddr(port_b);

    let kb = Keypair::generate_ed25519();
    let ka = Keypair::generate_ed25519();

    let node_b = P2PManager::spawn(kb, vec![listen_b.clone()], None).await.unwrap();
    let node_a = P2PManager::spawn(ka, vec![], None).await.unwrap();

    // Dial B directly
    let dial_addr = listen_b.with(Protocol::P2p(node_b.peer_id().into()));
    node_a.dial(dial_addr.clone()).await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let shared = node_b.share_file(src_path.clone()).await.unwrap();

    let result = timeout(
        Duration::from_secs(5),
        node_a.fetch_file_with_addr(node_b.peer_id(), shared.id, Some(dial_addr)),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(result.0, shared.name);
    assert_eq!(result.1, b"hello p2p");

    node_a.shutdown().await;
    node_b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "relay circuit negotiation is currently flaky; track and re-enable when stable"]
async fn file_transfer_via_relay() {
    let dir = tempdir().unwrap();
    let src_path = dir.path().join("relay.txt");
    tokio::fs::write(&src_path, b"through relay").await.unwrap();

    let (relay_peer, relay_base, relay_task) = start_relay().await.expect("relay start");
    let relay_addr = relay_base.with(Protocol::P2p(relay_peer.into()));

    let kb = Keypair::generate_ed25519();
    let ka = Keypair::generate_ed25519();

    let node_b = P2PManager::spawn(
        kb,
        vec![relay_addr.clone().with(Protocol::P2pCircuit)],
        Some(relay_addr.clone()),
    )
    .await
    .unwrap();
    let node_a = P2PManager::spawn(
        ka,
        vec![relay_addr.clone().with(Protocol::P2pCircuit)],
        Some(relay_addr.clone()),
    )
    .await
    .unwrap();

    wait_for_circuit(&node_b).await;
    wait_for_circuit(&node_a).await;

    // Dial B via relay circuit
    let dial_addr = relay_addr
        .clone()
        .with(Protocol::P2pCircuit)
        .with(Protocol::P2p(node_b.peer_id().into()));
    node_a.dial(dial_addr.clone()).await;
    tokio::time::sleep(Duration::from_millis(800)).await;

    let shared = node_b.share_file(src_path.clone()).await.unwrap();

    let fetched = timeout(
        Duration::from_secs(10),
        node_a.fetch_file_with_addr(node_b.peer_id(), shared.id, Some(dial_addr.clone())),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(fetched.1, b"through relay");

    node_a.shutdown().await;
    node_b.shutdown().await;
    relay_task.abort();
}
