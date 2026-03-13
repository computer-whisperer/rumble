//! Certificate verification types — re-exports from rumble-client and rumble-native.
//!
//! The platform-agnostic types (ServerCertInfo, CapturedCert) live in rumble-client.
//! The platform-specific verifiers (InteractiveCertVerifier, FingerprintVerifier) live
//! in rumble-native. This module re-exports both for backward compatibility.

// Platform-agnostic types from rumble-client
pub use rumble_client::cert::{
    CapturedCert, ServerCertInfo, compute_sha256_fingerprint, new_captured_cert, peek_captured_cert, take_captured_cert,
};

// Platform-specific types from rumble-native
pub use rumble_native::cert_verifier::{
    AcceptAllVerifier, FingerprintVerifier, InteractiveCertVerifier, is_cert_verification_error,
};
