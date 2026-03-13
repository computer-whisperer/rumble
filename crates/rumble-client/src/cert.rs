//! Certificate verification types for interactive self-signed cert acceptance.
//!
//! These types are platform-agnostic and used by both the transport layer
//! and the UI layer for the cert acceptance flow:
//!
//! 1. Transport::connect() fails on unknown cert → stores info in CapturedCert
//! 2. BackendHandle reads CapturedCert, prompts user
//! 3. User accepts → retry with fingerprint in TlsConfig.accepted_fingerprints

use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

/// Shared storage for a captured certificate during verification.
///
/// When a transport implementation encounters an unknown certificate during
/// TLS handshake, it stores the certificate info here. The caller checks
/// this after a connection error to determine if user confirmation is needed.
pub type CapturedCert = Arc<Mutex<Option<ServerCertInfo>>>;

/// Create a new empty captured certificate storage.
pub fn new_captured_cert() -> CapturedCert {
    Arc::new(Mutex::new(None))
}

/// Take the captured certificate, removing it from storage.
pub fn take_captured_cert(captured: &CapturedCert) -> Option<ServerCertInfo> {
    captured.lock().ok()?.take()
}

/// Peek at the captured certificate without removing it.
pub fn peek_captured_cert(captured: &CapturedCert) -> Option<ServerCertInfo> {
    captured.lock().ok()?.clone()
}

/// Information about a server certificate that failed verification.
#[derive(Debug, Clone)]
pub struct ServerCertInfo {
    /// The DER-encoded certificate that failed verification.
    pub certificate_der: Vec<u8>,
    /// SHA-256 fingerprint of the certificate.
    pub fingerprint: [u8; 32],
    /// The server name that was being verified.
    pub server_name: String,
}

impl ServerCertInfo {
    /// Create a new ServerCertInfo from a certificate and server name.
    pub fn new(cert_der: &[u8], server_name: &str) -> Self {
        Self {
            certificate_der: cert_der.to_vec(),
            fingerprint: compute_sha256_fingerprint(cert_der),
            server_name: server_name.to_string(),
        }
    }

    /// Get a hex-encoded fingerprint string for display.
    pub fn fingerprint_hex(&self) -> String {
        self.fingerprint
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|c| c.join(""))
            .collect::<Vec<_>>()
            .join(":")
    }

    /// Get a shortened fingerprint for compact display.
    pub fn fingerprint_short(&self) -> String {
        self.fingerprint
            .iter()
            .take(8)
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(":")
    }
}

impl std::fmt::Display for ServerCertInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Certificate for '{}' (fingerprint: {}...)",
            self.server_name,
            self.fingerprint_short()
        )
    }
}

impl std::error::Error for ServerCertInfo {}

/// Compute the SHA-256 fingerprint of a DER-encoded certificate.
pub fn compute_sha256_fingerprint(cert_der: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(cert_der);
    hasher.finalize().into()
}

/// Check if an error message indicates a certificate verification failure.
///
/// This is a string-based check for error chains. Transport implementations
/// may provide more specific error checking.
pub fn is_cert_error_message(error: &anyhow::Error) -> bool {
    for cause in error.chain() {
        let msg = cause.to_string();
        if msg.contains("UnknownIssuer") || msg.contains("BadSignature") || msg.contains("invalid peer certificate") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fingerprint_formatting() {
        let cert_der = [0u8; 100];
        let info = ServerCertInfo::new(&cert_der, "example.com");
        assert_eq!(info.fingerprint.len(), 32);
        let hex = info.fingerprint_hex();
        assert!(hex.contains(':'));
        let short = info.fingerprint_short();
        assert!(short.len() < hex.len());
    }

    #[test]
    fn test_display() {
        let cert_der = [0u8; 100];
        let info = ServerCertInfo::new(&cert_der, "test.server.com");
        let display = format!("{}", info);
        assert!(display.contains("test.server.com"));
        assert!(display.contains("fingerprint"));
    }

    #[test]
    fn test_captured_cert() {
        let captured = new_captured_cert();
        assert!(take_captured_cert(&captured).is_none());

        let cert_info = ServerCertInfo::new(&[1, 2, 3, 4, 5], "test.server.com");
        {
            let mut lock = captured.lock().unwrap();
            *lock = Some(cert_info);
        }

        let peeked = peek_captured_cert(&captured);
        assert!(peeked.is_some());
        assert_eq!(peeked.unwrap().server_name, "test.server.com");

        let taken = take_captured_cert(&captured);
        assert!(taken.is_some());
        assert!(take_captured_cert(&captured).is_none());
    }
}
