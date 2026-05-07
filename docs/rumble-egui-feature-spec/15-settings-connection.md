# §15 — Settings — Connection

`render_settings_connection`, `app.rs:1273-1375`. See also §3, §4.

- **Server / status** read-out: address (display of `connect_address`)
  and *"Connected"* / *"Disconnected"*.
- **Username** field — pending; applied on Apply.
- **Autoconnect on launch** checkbox — pending; on Apply, flips
  `autoconnect_on_launch` and updates `auto_connect_addr`.
- **Identity sub-panel** — see §2.3.

The connect address and password are NOT exposed in the settings modal
— only via the Connect... dialog.
