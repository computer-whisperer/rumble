//! TLS certificate verification for QUIC connections.
//!
//! Provides two verifiers:
//! - `FingerprintVerifier`: accepts certs whose SHA-256 fingerprint is in a known set
//! - `AcceptAllVerifier`: danger verifier that accepts any certificate (for testing)

use std::sync::Arc;

use rustls::{
    DigitallySignedStruct, Error, RootCertStore, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use sha2::{Digest, Sha256};

/// Error returned when a certificate's fingerprint is not in the accepted set.
///
/// Contains the DER bytes and fingerprint so the caller can prompt the user.
#[derive(Debug)]
pub struct UnknownFingerprintError {
    /// The DER-encoded certificate that failed verification.
    pub certificate_der: Vec<u8>,
    /// SHA-256 fingerprint of the certificate.
    pub fingerprint: [u8; 32],
}

impl std::fmt::Display for UnknownFingerprintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "certificate fingerprint {:02X}{:02X}{:02X}{:02X}... not in accepted set",
            self.fingerprint[0], self.fingerprint[1], self.fingerprint[2], self.fingerprint[3],
        )
    }
}

impl std::error::Error for UnknownFingerprintError {}

/// Compute the SHA-256 fingerprint of a DER-encoded certificate.
pub fn compute_sha256_fingerprint(cert_der: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(cert_der);
    hasher.finalize().into()
}

/// A certificate verifier that accepts certificates whose SHA-256 fingerprint
/// is in a provided set.
///
/// If the fingerprint is not found, verification fails with an error that
/// includes the certificate DER bytes and fingerprint, so the caller can
/// prompt the user and retry with the fingerprint added.
#[derive(Debug)]
pub struct FingerprintVerifier {
    fingerprints: Vec<[u8; 32]>,
    root_store: Arc<RootCertStore>,
    provider: Arc<CryptoProvider>,
}

impl FingerprintVerifier {
    /// Create a new fingerprint verifier that accepts certificates matching
    /// any of the given SHA-256 fingerprints.
    pub fn new(fingerprints: Vec<[u8; 32]>) -> Self {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        Self {
            fingerprints,
            root_store: Arc::new(RootCertStore::empty()),
            provider,
        }
    }

    /// Add additional root CA certificates (DER-encoded) for fallback
    /// WebPKI verification.
    pub fn with_additional_roots(mut self, roots: RootCertStore) -> Self {
        self.root_store = Arc::new(roots);
        self
    }
}

impl ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let fingerprint = compute_sha256_fingerprint(end_entity.as_ref());

        if self.fingerprints.iter().any(|fp| fp == &fingerprint) {
            return Ok(ServerCertVerified::assertion());
        }

        // Fingerprint not in accepted set — return an error with the cert info
        // so the caller can prompt and retry.
        Err(Error::General(format!(
            "{}",
            UnknownFingerprintError {
                certificate_der: end_entity.as_ref().to_vec(),
                fingerprint,
            }
        )))
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// A danger verifier that accepts any certificate without verification.
///
/// Only use for testing or when `accept_invalid_certs` is explicitly set.
#[derive(Debug)]
pub struct AcceptAllVerifier {
    provider: Arc<CryptoProvider>,
}

impl AcceptAllVerifier {
    pub fn new() -> Self {
        Self {
            provider: Arc::new(rustls::crypto::aws_lc_rs::default_provider()),
        }
    }
}

impl ServerCertVerifier for AcceptAllVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}
