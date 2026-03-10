//! Transport abstraction for reliable + unreliable messaging.

use async_trait::async_trait;

/// TLS configuration for transport connections.
pub struct TlsConfig {
    pub accept_invalid_certs: bool,
    /// DER-encoded additional CA certificates.
    pub additional_ca_certs: Vec<Vec<u8>>,
    /// SHA-256 fingerprints of accepted server certificates.
    pub accepted_fingerprints: Vec<[u8; 32]>,
}

/// Reliable + unreliable transport for the Rumble protocol.
///
/// Implementations may use QUIC (native) or WebTransport (browser).
/// Reliable messages go over streams; unreliable datagrams carry voice.
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// Connect to a server at the given address.
    async fn connect(addr: &str, tls_config: TlsConfig) -> anyhow::Result<Self>
    where
        Self: Sized;

    /// Send a length-prefixed protobuf message over the reliable stream.
    async fn send(&mut self, data: &[u8]) -> anyhow::Result<()>;

    /// Receive the next reliable message, or `None` if the connection closed.
    async fn recv(&mut self) -> anyhow::Result<Option<Vec<u8>>>;

    /// Send a datagram (unreliable, unordered).
    fn send_datagram(&self, data: &[u8]) -> anyhow::Result<()>;

    /// Receive the next datagram, or `None` if the connection closed.
    async fn recv_datagram(&self) -> anyhow::Result<Option<Vec<u8>>>;

    /// Return the DER-encoded peer certificate, if available.
    fn peer_certificate_der(&self) -> Option<Vec<u8>>;

    /// Gracefully close the connection.
    async fn close(&self);
}
