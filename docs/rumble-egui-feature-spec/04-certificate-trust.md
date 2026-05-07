# §4 — Certificate Trust (TOFU)

## 4.1 Pre-load on startup

`app.rs:660-692`. On startup the app iterates
`persistent_settings.accepted_certificates` and
`shared_settings.accepted_certificates`, base64-decodes the DER bytes,
and pushes them into `ConnectConfig.accepted_certs`. Bad base64 logs a
warning but does not fail.

It also adds:

- `dev-certs/server-cert.der` if `trust_dev_cert` is true.
- The `--cert <path>` value if set.
- The `RUMBLE_SERVER_CERT_PATH` env-var path if set.

## 4.2 Untrusted-cert modal

Auto-shown when `state.connection == CertificatePending { cert_info }`.
At `app.rs:4660-4767`, fixed width 450px.

Layout:

- Heading: *"⚠ Untrusted Certificate"*.
- Body: *"The server presented a certificate that is not trusted."*
- *"Server: {server_addr}"* and *"Certificate for: {server_name}"* (bold).
- Read-only multiline `TextEdit` showing the SHA-256 fingerprint as
  colon-separated hex (`cert_info.fingerprint_hex()`), 2 rows, monospace.
- Help: *"If you expected to connect to a server with a self-signed
  certificate, verify the fingerprint matches what the server
  administrator provided."*
- Footer: weak *"⚠ Only accept if you trust this server"* on the left,
  **Accept Certificate** + **Reject** on the right.

`PendingCertificate` carries the full handshake state
(`certificate_der`, raw 32-byte fingerprint, `server_name`,
`server_addr`, `username`, `password`, `public_key`, `signer`) so accept
retries with identical credentials — the user does not re-enter
anything.

## 4.3 Accept side effects

- Pushes into `persistent_settings.accepted_certificates: Vec<
  AcceptedCertificate { server_name, fingerprint_hex,
  certificate_der_base64 }>` (deduped by fingerprint); saves
  `settings.json`.
- Pushes into `shared_settings.accepted_certificates` (deduped by
  `(server_name, fingerprint)`); saves `desktop-shell.json`.
- Sends `Command::AcceptCertificate`.
- Posts *"Accepted certificate for {server_name} (saved for future
  connections)"* to chat.

## 4.4 Reject side effects

- Sends `Command::RejectCertificate`.
- Posts *"Rejected certificate for {server_name}"* to chat.

## 4.5 Caveat: TOFU is fingerprint-based, not server-pinned

Accepted certs apply globally across server names — there is no
per-server pinning enforcement at the UI layer.
