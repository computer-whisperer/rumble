//! Test node for backend P2P file transfer integration testing.
//!
//! This node uses the actual `rumble_client::BackendHandle` to connect to a real
//! Rumble server and test:
//! - P2P file transfer via BitTorrent
//! - NAT traversal via server relay
//!
//! Unlike unit tests, this runs in Docker containers with network isolation
//! to simulate real-world NAT scenarios.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use rumble_client::{BackendHandle, Command as BackendCommand, ConnectConfig, SigningCallback};
use clap::{Parser, Subcommand};
use ed25519_dalek::SigningKey;
use tracing::{debug, error, info, warn};

#[derive(Parser, Debug)]
#[command(author, version, about = "Backend P2P Integration Test Node")]
struct Args {
    /// Node name for logging
    #[arg(short, long, default_value = "node")]
    name: String,

    /// Server address (host:port)
    #[arg(short, long, default_value = "server:5000")]
    server: String,

    /// Path to server certificate (PEM format)
    #[arg(short, long)]
    cert: Option<PathBuf>,

    /// Download directory for received files
    #[arg(short, long, default_value = "/data/downloads")]
    download_dir: PathBuf,

    /// Use relay mode for file transfers (for NAT traversal)
    #[arg(long, default_value = "false")]
    prefer_relay: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Share a file and wait for it to be downloaded
    Share {
        /// Path to file to share
        #[arg(short, long)]
        file: PathBuf,

        /// Duration to wait for downloads (seconds, 0 = forever)
        #[arg(short, long, default_value = "60")]
        wait: u64,
    },

    /// Download a file using a magnet link
    Fetch {
        /// Magnet link to download
        #[arg(short, long)]
        magnet: String,

        /// Timeout in seconds
        #[arg(short, long, default_value = "60")]
        timeout: u64,
    },

    /// Connect to server and wait (for testing connectivity)
    Wait {
        /// Duration to wait (seconds, 0 = forever)
        #[arg(short, long, default_value = "0")]
        duration: u64,
    },

    /// Print connection info and exit
    Info,
}

fn random_signing_key() -> SigningKey {
    SigningKey::from_bytes(&rand::random())
}

fn create_test_credentials() -> ([u8; 32], SigningCallback) {
    let signing_key = random_signing_key();
    let public_key = signing_key.verifying_key().to_bytes();
    let key_bytes = signing_key.to_bytes();

    let signer: SigningCallback = Arc::new(move |payload: &[u8]| {
        use ed25519_dalek::Signer;
        let key = SigningKey::from_bytes(&key_bytes);
        let signature = key.sign(payload);
        Ok(signature.to_bytes())
    });

    (public_key, signer)
}

fn create_backend(cert_path: Option<&std::path::Path>, download_dir: PathBuf, prefer_relay: bool) -> (BackendHandle, Arc<AtomicBool>) {
    let repaint_called = Arc::new(AtomicBool::new(false));
    let repaint_called_clone = repaint_called.clone();

    let mut config = ConnectConfig::new()
        .with_download_dir(download_dir)
        .with_prefer_relay(prefer_relay);

    if let Some(cert) = cert_path {
        config = config.with_cert(cert);
    }

    let handle =
        BackendHandle::with_config(move || repaint_called_clone.store(true, Ordering::SeqCst), config);

    (handle, repaint_called)
}

fn wait_for<F>(handle: &BackendHandle, timeout: Duration, condition: F) -> bool
where
    F: Fn(&rumble_client::State) -> bool,
{
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if condition(&handle.state()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,backend=debug".parse().unwrap()),
        )
        .init();

    let args = Args::parse();

    info!(name = %args.name, server = %args.server, prefer_relay = %args.prefer_relay, "Test node starting");

    // Create download directory
    std::fs::create_dir_all(&args.download_dir).context("create download dir")?;

    // Create backend handle
    let (handle, _repaint) = create_backend(args.cert.as_deref(), args.download_dir.clone(), args.prefer_relay);

    // Print node info
    println!("\n========================================");
    println!("NODE INFORMATION");
    println!("========================================");
    println!("Name: {}", args.name);
    println!("Server: {}", args.server);
    println!("Download Dir: {}", args.download_dir.display());
    println!("Prefer Relay: {}", args.prefer_relay);
    println!("========================================\n");

    match args.command {
        Command::Share { file, wait } => {
            run_share(&handle, &args.server, &args.name, &file, wait)?;
        }
        Command::Fetch { magnet, timeout } => {
            run_fetch(&handle, &args.server, &args.name, &magnet, timeout)?;
        }
        Command::Wait { duration } => {
            run_wait(&handle, &args.server, &args.name, duration)?;
        }
        Command::Info => {
            // Already printed above
        }
    }

    info!("Test node shutting down");
    Ok(())
}

fn connect_to_server(handle: &BackendHandle, server: &str, name: &str) -> Result<()> {
    info!(%server, %name, "Connecting to server");

    let (public_key, signer) = create_test_credentials();
    handle.send(BackendCommand::Connect {
        addr: server.to_string(),
        name: name.to_string(),
        public_key,
        signer,
        password: None,
    });

    // Wait for connection
    let connected = wait_for(handle, Duration::from_secs(10), |s| s.connection.is_connected());

    if !connected {
        let state = handle.state();
        error!(?state.connection, "Failed to connect to server");
        anyhow::bail!("Failed to connect to server within timeout");
    }

    info!("Connected to server");
    Ok(())
}

fn run_share(handle: &BackendHandle, server: &str, name: &str, file: &PathBuf, wait_secs: u64) -> Result<()> {
    // Connect to server first
    connect_to_server(handle, server, name)?;

    // Read file to get its info
    let file_name = file
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    let file_size = std::fs::metadata(file)?.len();

    info!(?file, %file_name, %file_size, "Sharing file");

    // Share the file
    handle.send(BackendCommand::ShareFile { path: file.clone() });

    // Wait for magnet link to appear in chat messages
    let mut magnet_link = String::new();
    let found_magnet = wait_for(handle, Duration::from_secs(10), |s| {
        for msg in &s.chat_messages {
            if msg.is_local && msg.text.contains("Magnet:") {
                return true;
            }
        }
        false
    });

    if !found_magnet {
        let state = handle.state();
        warn!("Chat messages:");
        for msg in &state.chat_messages {
            warn!("- [{}] {}", msg.sender, msg.text);
        }
        anyhow::bail!("Failed to generate magnet link");
    }

    // Extract magnet link
    let state = handle.state();
    for msg in &state.chat_messages {
        if msg.is_local && msg.text.contains("Magnet:") {
            if let Some(idx) = msg.text.find("Magnet: ") {
                magnet_link = msg.text[idx + 8..].trim().to_string();
                break;
            }
        }
    }

    println!("\n========================================");
    println!("SHARING FILE");
    println!("========================================");
    println!("File: {}", file_name);
    println!("Size: {} bytes", file_size);
    println!("Magnet: {}", magnet_link);
    println!("========================================\n");

    // Output as JSON for scripting
    let info = serde_json::json!({
        "file_name": file_name,
        "file_size": file_size,
        "magnet": magnet_link,
    });
    println!("JSON: {}", serde_json::to_string(&info)?);

    // Wait for the specified duration
    if wait_secs == 0 {
        info!("Seeding forever... (Ctrl+C to exit)");
        loop {
            std::thread::sleep(Duration::from_secs(60));
            let state = handle.state();
            for transfer in &state.file_transfers {
                debug!(
                    name = %transfer.name,
                    progress = %transfer.progress,
                    state = ?transfer.state,
                    "Transfer status"
                );
            }
        }
    } else {
        info!("Seeding for {} seconds...", wait_secs);
        std::thread::sleep(Duration::from_secs(wait_secs));
    }

    Ok(())
}

fn run_fetch(
    handle: &BackendHandle,
    server: &str,
    name: &str,
    magnet: &str,
    timeout_secs: u64,
) -> Result<()> {
    // Connect to server first
    connect_to_server(handle, server, name)?;

    info!(%magnet, "Downloading file");

    // Start download
    handle.send(BackendCommand::DownloadFile {
        magnet: magnet.to_string(),
    });

    // Wait for download to complete
    let download_finished = wait_for(handle, Duration::from_secs(timeout_secs), |s| {
        for transfer in &s.file_transfers {
            if transfer.state.is_finished() && transfer.progress >= 1.0 {
                return true;
            }
        }
        false
    });

    if !download_finished {
        let state = handle.state();
        error!("Download did not complete within timeout");
        warn!("File transfers:");
        for transfer in &state.file_transfers {
            warn!(
                "- Name: {}, Progress: {:.1}%, State: {:?}",
                transfer.name,
                transfer.progress * 100.0,
                transfer.state
            );
        }
        anyhow::bail!("Download failed or timed out");
    }

    // Get transfer info
    let state = handle.state();
    let transfer = state
        .file_transfers
        .iter()
        .find(|t| t.state.is_finished())
        .context("no finished transfer")?;

    println!("\n========================================");
    println!("FILE DOWNLOADED");
    println!("========================================");
    println!("Name: {}", transfer.name);
    println!("Progress: 100%");
    println!("State: {:?}", transfer.state);
    println!("========================================\n");

    let info = serde_json::json!({
        "success": true,
        "file_name": transfer.name,
    });
    println!("JSON: {}", serde_json::to_string(&info)?);

    Ok(())
}

fn run_wait(handle: &BackendHandle, server: &str, name: &str, duration_secs: u64) -> Result<()> {
    // Connect to server
    connect_to_server(handle, server, name)?;

    if duration_secs == 0 {
        info!("Connected, waiting forever... (Ctrl+C to exit)");
        loop {
            std::thread::sleep(Duration::from_secs(60));
            let state = handle.state();
            debug!(
                connected = state.connection.is_connected(),
                users = state.users.len(),
                "Still connected"
            );
        }
    } else {
        info!("Connected, waiting {} seconds...", duration_secs);
        std::thread::sleep(Duration::from_secs(duration_secs));
    }

    Ok(())
}
