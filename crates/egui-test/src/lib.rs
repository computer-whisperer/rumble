//! Rumble voice chat client library.
//!
//! This crate provides the core Rumble application logic and test harness.
//! The application can be run via:
//! - **eframe** (desktop): See `main.rs` for the native runner
//! - **test harness**: See [`TestHarness`] for automated testing with input injection

pub mod app;
pub mod harness;
pub mod key_manager;
pub mod settings;

pub use app::RumbleApp;
pub use harness::TestHarness;
pub use settings::{Args, PersistentSettings};
