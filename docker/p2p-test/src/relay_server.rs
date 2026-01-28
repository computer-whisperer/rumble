//! Relay server for P2P NAT traversal testing.
//!
//! This server acts as a libp2p relay that helps peers behind NAT to discover
//! each other and establish direct connections via hole punching (DCUtR).
//!
//! Supports:
//! - IPv4 and IPv6 dual-stack
//! - TCP and QUIC transports

use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use futures::StreamExt;
use libp2p::{
    identify, noise, ping, relay,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, SwarmBuilder,
};
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(author, version, about = "P2P Relay Server for NAT traversal testing")]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "4001")]
    port: u16,

    /// Bind address
    #[arg(short, long, default_value = "0.0.0.0")]
    bind: String,
}

#[derive(NetworkBehaviour)]
struct RelayServerBehaviour {
    relay: relay::Behaviour,
    ping: ping::Behaviour,
    identify: identify::Behaviour,
}

fn make_noise(
    kp: &libp2p::identity::Keypair,
) -> Result<noise::Config, noise::Error> {
    noise::Config::new(kp)
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

    // Generate a new keypair for this relay instance
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());

    info!(%peer_id, "Relay server starting");

    // Build the swarm with TCP + QUIC transports
    let mut swarm = SwarmBuilder::with_existing_identity(keypair.clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            make_noise,
            || yamux::Config::default(),
        )?
        .with_quic()
        .with_behaviour(|key| {
            let local_peer_id = PeerId::from(key.public());

            // Use default relay config
            let relay_config = relay::Config::default();

            let identify_config = identify::Config::new(
                "rumble-relay/1.0.0".to_string(),
                key.public(),
            );

            RelayServerBehaviour {
                relay: relay::Behaviour::new(local_peer_id, relay_config),
                ping: ping::Behaviour::new(ping::Config::default()),
                identify: identify::Behaviour::new(identify_config),
            }
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(120)))
        .build();

    // Listen on IPv4 TCP
    let listen_addr_tcp4: Multiaddr = format!("/ip4/{}/tcp/{}", args.bind, args.port).parse()?;
    swarm.listen_on(listen_addr_tcp4.clone())?;

    // Listen on IPv4 QUIC (UDP)
    let listen_addr_quic4: Multiaddr = format!("/ip4/{}/udp/{}/quic-v1", args.bind, args.port).parse()?;
    swarm.listen_on(listen_addr_quic4.clone())?;

    // Listen on IPv6 if available (dual-stack)
    let bind_v6 = if args.bind == "0.0.0.0" { "::" } else { &args.bind };
    let listen_addr_tcp6: Multiaddr = format!("/ip6/{}/tcp/{}", bind_v6, args.port).parse()?;
    if let Err(e) = swarm.listen_on(listen_addr_tcp6.clone()) {
        info!(%e, "IPv6 TCP not available, skipping");
    }

    let listen_addr_quic6: Multiaddr = format!("/ip6/{}/udp/{}/quic-v1", bind_v6, args.port).parse()?;
    if let Err(e) = swarm.listen_on(listen_addr_quic6.clone()) {
        info!(%e, "IPv6 QUIC not available, skipping");
    }

    // Add external addresses so relay can tell clients their relayed addresses
    // This is crucial for DCUtR to work - peers need to know where the relay is
    swarm.add_external_address(listen_addr_tcp4.clone());
    swarm.add_external_address(listen_addr_quic4.clone());

    info!(%listen_addr_tcp4, %listen_addr_quic4, "Relay listening");

    // Print connection info for other nodes
    println!("\n========================================");
    println!("RELAY SERVER INFORMATION");
    println!("========================================");
    println!("Peer ID: {}", peer_id);
    println!("TCP (IPv4): {}/p2p/{}", listen_addr_tcp4, peer_id);
    println!("QUIC (IPv4): {}/p2p/{}", listen_addr_quic4, peer_id);
    println!("TCP (IPv6): {}/p2p/{}", listen_addr_tcp6, peer_id);
    println!("QUIC (IPv6): {}/p2p/{}", listen_addr_quic6, peer_id);
    println!("========================================\n");

    // Output as JSON for scripting (use TCP IPv4 as primary for backwards compat)
    let info = serde_json::json!({
        "peer_id": peer_id.to_string(),
        "listen_addr": listen_addr_tcp4.to_string(),
        "full_addr": format!("{}/p2p/{}", listen_addr_tcp4, peer_id),
        "quic_addr": format!("{}/p2p/{}", listen_addr_quic4, peer_id),
    });
    println!("JSON: {}", serde_json::to_string(&info)?);

    // Event loop
    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => {
                let full_addr = format!("{}/p2p/{}", address, peer_id);
                info!(%address, %full_addr, "Listening on new address");
            }
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                info!(%peer_id, ?endpoint, "Connection established");
            }
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                info!(%peer_id, ?cause, "Connection closed");
            }
            SwarmEvent::Behaviour(RelayServerBehaviourEvent::Relay(event)) => {
                match &event {
                    relay::Event::ReservationReqAccepted { src_peer_id, .. } => {
                        info!(%src_peer_id, "Reservation accepted");
                    }
                    relay::Event::CircuitClosed { src_peer_id, dst_peer_id, .. } => {
                        info!(%src_peer_id, %dst_peer_id, "Circuit closed");
                    }
                    _ => {
                        tracing::debug!(?event, "Relay event");
                    }
                }
            }
            SwarmEvent::Behaviour(RelayServerBehaviourEvent::Identify(event)) => {
                if let identify::Event::Received { peer_id, info, .. } = event {
                    info!(%peer_id, agent = %info.agent_version, "Identified peer");
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
