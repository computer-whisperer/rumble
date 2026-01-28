//! CLI for the Rumble GUI test harness daemon.
//!
//! This tool provides a command-line interface for automated GUI testing.
//! It runs as a daemon that manages GUI client instances and provides
//! screenshot and interaction capabilities.
//!
//! # Usage
//!
//! ```bash
//! # Start the daemon (forks to background)
//! rumble-harness daemon start
//!
//! # Start the server
//! rumble-harness server start
//!
//! # Create a client
//! rumble-harness client new --name bot1 --server 127.0.0.1:5000
//!
//! # Take a screenshot
//! rumble-harness client screenshot 1 --output screenshot.png
//!
//! # Run interaction
//! rumble-harness client click 1 100 200
//! rumble-harness client type 1 "Hello world"
//!
//! # Stop everything
//! rumble-harness daemon stop
//! ```

mod daemon;
mod protocol;
mod renderer;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

use crate::protocol::{Command, Response, ResponseData};

#[derive(Parser)]
#[command(name = "rumble-harness")]
#[command(about = "CLI for automated GUI testing of Rumble")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Socket path (default: $XDG_RUNTIME_DIR/rumble-harness.sock)
    #[arg(long, global = true)]
    socket: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Daemon management
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },

    /// Server management
    Server {
        #[command(subcommand)]
        action: ServerAction,
    },

    /// Client management and interaction
    Client {
        #[command(subcommand)]
        action: ClientAction,
    },

    /// Get daemon status
    Status,
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon (runs in foreground by default)
    Start {
        /// Fork to background
        #[arg(short, long)]
        background: bool,
    },

    /// Stop the daemon
    Stop,

    /// Check if daemon is running
    Status,
}

#[derive(Subcommand)]
enum ServerAction {
    /// Start the Rumble server
    Start {
        /// Port to listen on
        #[arg(short, long, default_value = "5000")]
        port: u16,
    },

    /// Stop the Rumble server
    Stop,
}

#[derive(Subcommand)]
enum ClientAction {
    /// Create a new GUI client instance
    New {
        /// Client display name
        #[arg(short, long)]
        name: Option<String>,

        /// Server address to connect to
        #[arg(short, long)]
        server: Option<String>,
    },

    /// List all active clients
    List,

    /// Close a client instance
    Close {
        /// Client ID
        id: u32,
    },

    /// Take a screenshot
    Screenshot {
        /// Client ID
        id: u32,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Click at a position
    Click {
        /// Client ID
        id: u32,
        /// X coordinate
        x: f32,
        /// Y coordinate
        y: f32,
    },

    /// Move mouse to position
    MouseMove {
        /// Client ID
        id: u32,
        /// X coordinate
        x: f32,
        /// Y coordinate
        y: f32,
    },

    /// Press a key
    KeyPress {
        /// Client ID
        id: u32,
        /// Key name (e.g., "space", "enter", "a")
        key: String,
    },

    /// Release a key
    KeyRelease {
        /// Client ID
        id: u32,
        /// Key name
        key: String,
    },

    /// Tap a key (press and release)
    KeyTap {
        /// Client ID
        id: u32,
        /// Key name
        key: String,
    },

    /// Type text
    Type {
        /// Client ID
        id: u32,
        /// Text to type
        text: String,
    },

    /// Run frames to advance the UI
    Frames {
        /// Client ID
        id: u32,
        /// Number of frames to run
        #[arg(default_value = "1")]
        count: u32,
    },

    /// Check if connected to server
    Connected {
        /// Client ID
        id: u32,
    },

    /// Get backend state as JSON
    State {
        /// Client ID
        id: u32,
    },

    /// Click a widget by its accessible label
    ClickWidget {
        /// Client ID
        id: u32,
        /// Widget label text
        label: String,
    },

    /// Check if a widget with the given label exists
    HasWidget {
        /// Client ID
        id: u32,
        /// Widget label text
        label: String,
    },

    /// Get the bounding rectangle of a widget by label
    WidgetRect {
        /// Client ID
        id: u32,
        /// Widget label text
        label: String,
    },

    /// Run frames until UI settles (animations complete)
    Run {
        /// Client ID
        id: u32,
    },

    /// Enable or disable auto-download
    SetAutoDownload {
        /// Client ID
        id: u32,
        /// Enable auto-download (true/false)
        #[arg(action = clap::ArgAction::Set, value_parser = clap::builder::BoolishValueParser::new())]
        enabled: bool,
    },

    /// Set auto-download rules (JSON array)
    SetAutoDownloadRules {
        /// Client ID
        id: u32,
        /// Rules as JSON: [{"mime_pattern": "image/*", "max_size_bytes": 10485760}]
        rules_json: String,
    },

    /// Get file transfer settings
    GetFileTransferSettings {
        /// Client ID
        id: u32,
    },

    /// Share a file
    ShareFile {
        /// Client ID
        id: u32,
        /// Path to the file to share
        path: String,
    },

    /// Get list of file transfers
    GetFileTransfers {
        /// Client ID
        id: u32,
    },

    /// Show or hide the file transfers window
    ShowTransfers {
        /// Client ID
        id: u32,
        /// Show the window (true) or hide it (false)
        #[arg(action = clap::ArgAction::Set, value_parser = clap::builder::BoolishValueParser::new())]
        show: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let socket_path = cli.socket.unwrap_or_else(daemon::socket_path);

    match cli.command {
        Commands::Daemon { action } => handle_daemon(action, &socket_path).await,
        Commands::Server { action } => handle_server(action, &socket_path).await,
        Commands::Client { action } => handle_client(action, &socket_path).await,
        Commands::Status => {
            let response = send_command(&socket_path, Command::Status).await?;
            print_response(&response);
            Ok(())
        }
    }
}

async fn handle_daemon(action: DaemonAction, socket_path: &PathBuf) -> Result<()> {
    match action {
        DaemonAction::Start { background } => {
            if background {
                // Fork to background using double-fork
                println!("Starting daemon in background...");

                let exe = std::env::current_exe()?;

                std::process::Command::new(&exe)
                    .args(["--socket", &socket_path.to_string_lossy(), "daemon", "start"])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()?;

                // Wait a moment for daemon to start
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                // Check if it's running
                if socket_path.exists() {
                    println!("Daemon started. Socket: {}", socket_path.display());
                } else {
                    eprintln!("Warning: Daemon may not have started correctly");
                }
            } else {
                // Run in foreground
                tracing_subscriber::fmt()
                    .with_env_filter(
                        tracing_subscriber::EnvFilter::from_default_env()
                            .add_directive("harness_cli=debug".parse().unwrap())
                            .add_directive("info".parse().unwrap()),
                    )
                    .init();

                println!("Starting daemon (foreground)...");
                println!("Socket: {}", socket_path.display());
                println!("Press Ctrl+C to stop");

                let daemon = daemon::Daemon::new(socket_path.clone());
                daemon.run().await?;
            }
            Ok(())
        }

        DaemonAction::Stop => {
            let response = send_command(socket_path, Command::Shutdown).await?;
            print_response(&response);
            Ok(())
        }

        DaemonAction::Status => {
            match send_command(socket_path, Command::Ping).await {
                Ok(_) => println!("Daemon is running"),
                Err(_) => println!("Daemon is not running"),
            }
            Ok(())
        }
    }
}

async fn handle_server(action: ServerAction, socket_path: &PathBuf) -> Result<()> {
    let cmd = match action {
        ServerAction::Start { port } => Command::ServerStart { port },
        ServerAction::Stop => Command::ServerStop,
    };

    let response = send_command(socket_path, cmd).await?;
    print_response(&response);
    Ok(())
}

async fn handle_client(action: ClientAction, socket_path: &PathBuf) -> Result<()> {
    let cmd = match action {
        ClientAction::New { name, server } => Command::ClientNew { name, server },
        ClientAction::List => Command::ClientList,
        ClientAction::Close { id } => Command::ClientClose { id },
        ClientAction::Screenshot { id, output } => Command::Screenshot { id, output },
        ClientAction::Click { id, x, y } => Command::Click { id, x, y },
        ClientAction::MouseMove { id, x, y } => Command::MouseMove { id, x, y },
        ClientAction::KeyPress { id, key } => Command::KeyPress { id, key },
        ClientAction::KeyRelease { id, key } => Command::KeyRelease { id, key },
        ClientAction::KeyTap { id, key } => Command::KeyTap { id, key },
        ClientAction::Type { id, text } => Command::TypeText { id, text },
        ClientAction::Frames { id, count } => Command::RunFrames { id, count },
        ClientAction::Connected { id } => Command::IsConnected { id },
        ClientAction::State { id } => Command::GetState { id },
        ClientAction::ClickWidget { id, label } => Command::ClickWidget { id, label },
        ClientAction::HasWidget { id, label } => Command::HasWidget { id, label },
        ClientAction::WidgetRect { id, label } => Command::WidgetRect { id, label },
        ClientAction::Run { id } => Command::Run { id },
        ClientAction::SetAutoDownload { id, enabled } => Command::SetAutoDownload { id, enabled },
        ClientAction::SetAutoDownloadRules { id, rules_json } => {
            let rules: Vec<protocol::AutoDownloadRuleConfig> = serde_json::from_str(&rules_json)
                .context("Invalid JSON for rules. Expected: [{\"mime_pattern\": \"image/*\", \"max_size_bytes\": 10485760}]")?;
            Command::SetAutoDownloadRules { id, rules }
        }
        ClientAction::GetFileTransferSettings { id } => Command::GetFileTransferSettings { id },
        ClientAction::ShareFile { id, path } => Command::ShareFile { id, path },
        ClientAction::GetFileTransfers { id } => Command::GetFileTransfers { id },
        ClientAction::ShowTransfers { id, show } => Command::ShowTransfers { id, show },
    };

    let response = send_command(socket_path, cmd).await?;
    print_response(&response);
    Ok(())
}

/// Send a command to the daemon and get the response.
async fn send_command(socket_path: &PathBuf, cmd: Command) -> Result<Response> {
    let stream = UnixStream::connect(socket_path)
        .await
        .context("Failed to connect to daemon. Is it running? (rumble-harness daemon start)")?;

    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Send command
    let cmd_json = serde_json::to_string(&cmd)?;
    writer.write_all(cmd_json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    // Read response
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let response: Response = serde_json::from_str(&line)?;
    Ok(response)
}

/// Print a response to stdout.
fn print_response(response: &Response) {
    match response {
        Response::Ok { data } => {
            match data {
                ResponseData::Ack => {
                    println!("OK");
                }
                ResponseData::Pong => {
                    println!("pong");
                }
                ResponseData::Status {
                    server_running,
                    client_count,
                } => {
                    println!("Server running: {}", server_running);
                    println!("Active clients: {}", client_count);
                }
                ResponseData::ClientCreated { id } => {
                    println!("Client created: {}", id);
                }
                ResponseData::ClientList { clients } => {
                    if clients.is_empty() {
                        println!("No active clients");
                    } else {
                        println!("Active clients:");
                        for client in clients {
                            println!(
                                "  [{}] {} (connected: {})",
                                client.id, client.name, client.connected
                            );
                        }
                    }
                }
                ResponseData::Screenshot { path, width, height } => {
                    println!("Screenshot saved: {} ({}x{})", path, width, height);
                }
                ResponseData::Connected { connected } => {
                    println!("{}", connected);
                }
                ResponseData::State { state } => {
                    println!("{}", serde_json::to_string_pretty(state).unwrap());
                }
                ResponseData::FramesRun { count } => {
                    println!("Ran {} frames", count);
                }
                ResponseData::WidgetExists { exists } => {
                    println!("{}", exists);
                }
                ResponseData::WidgetRect {
                    found,
                    x,
                    y,
                    width,
                    height,
                } => {
                    if *found {
                        println!(
                            "x: {}, y: {}, width: {}, height: {}",
                            x.unwrap_or(0.0),
                            y.unwrap_or(0.0),
                            width.unwrap_or(0.0),
                            height.unwrap_or(0.0)
                        );
                    } else {
                        println!("Widget not found");
                    }
                }
                ResponseData::FileTransferSettings {
                    auto_download_enabled,
                    auto_download_rules,
                } => {
                    println!("Auto-download enabled: {}", auto_download_enabled);
                    println!("Rules:");
                    for rule in auto_download_rules {
                        println!(
                            "  {} (max {} bytes)",
                            rule.mime_pattern, rule.max_size_bytes
                        );
                    }
                }
                ResponseData::FileShared { infohash, magnet_link } => {
                    println!("File shared!");
                    println!("Infohash: {}", infohash);
                    println!("Magnet: {}", magnet_link);
                }
                ResponseData::FileTransfers { transfers } => {
                    if transfers.is_empty() {
                        println!("No file transfers");
                    } else {
                        println!("File transfers:");
                        for t in transfers {
                            println!(
                                "  [{}] {} ({:.1}% - {})",
                                &t.infohash[..8],
                                t.name,
                                t.progress * 100.0,
                                t.state
                            );
                        }
                    }
                }
            }
        }
        Response::Error { message } => {
            eprintln!("Error: {}", message);
            std::process::exit(1);
        }
    }
}
