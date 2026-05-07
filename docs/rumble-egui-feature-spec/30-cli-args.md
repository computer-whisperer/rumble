# §30 — CLI Arguments

`crates/rumble-egui/src/settings.rs:21-57` (`Args`,
`#[derive(Parser, Debug, Clone, Default)]`).

| Flag | Type | Effect |
|---|---|---|
| `-s, --server <addr>` | `Option<String>` | Override persisted server address; auto-connects if first-run is complete. |
| `-n, --name <name>` | `Option<String>` | Override stored display name. |
| `-p, --password <pw>` | `Option<String>` | Override stored server password. |
| `--trust-dev-cert` | `bool`, default `true` | Trust `dev-certs/server-cert.der`. |
| `--cert <path>` | `Option<String>` | Trust an additional CA/server cert (DER or PEM). |
| `--rpc <command>` (Unix) | `Option<String>` | Send a single command to a running instance over Unix socket and exit. See §32. |
| `--rpc-socket <path>` | `Option<String>` | Override the default RPC socket path (`rumble_client::rpc::default_socket_path()`). |
| `--rpc-server` | `bool` | Start the RPC server inside the running GUI. |
| `--config-dir <path>` | `Option<String>` | Override platform-default config dir. |

Plus the env var `RUMBLE_SERVER_CERT_PATH` — additional cert path beyond
CLI/persisted settings.

CLI overrides do **not** mutate the persisted values.
