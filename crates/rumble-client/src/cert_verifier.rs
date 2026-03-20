//! Certificate verification types — re-exports from rumble-client-traits.
//!
//! The platform-agnostic types (ServerCertInfo, CapturedCert) live in rumble-client-traits.
//! Platform-specific verifiers (InteractiveCertVerifier, etc.) live in their
//! respective platform crates (e.g. rumble-desktop).

// Platform-agnostic types from rumble-client-traits
pub use rumble_client_traits::cert::{
    CapturedCert, ServerCertInfo, compute_sha256_fingerprint, is_cert_error_message, new_captured_cert,
    peek_captured_cert, take_captured_cert,
};
