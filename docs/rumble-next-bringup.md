# rumble-next bringup

Feature-diff between `rumble-egui` (existing client, ~5400 LOC in `app.rs`) and
`rumble-next` (new from-scratch client built on `rumble-widgets` themes /
paradigms). Tracks what's wired, what's missing, and what we still need to
build to reach parity. Update this doc as items land.

## Where we are

**Bringup status (2026-04-25):** Steps 0–6 (partial) + 9 (partial) + 10
have landed. Steps 7 (admin/ACL) and 8 (file transfer) are deferred —
both need design work that the current state doesn't justify rushing.
See "Delivered vs deferred" at the bottom for the per-step breakdown.

`rumble-next` is in early bringup. The architecture is settled:

- **Paradigm system** — `Paradigm::{Modern, MumbleClassic, Luna}` × `dark`
  toggle = 6 themes, installed via `rumble_widgets::install_theme` whenever
  the selection changes. Each paradigm owns its chrome (menubar/toolbar/
  statusbar) and dispatches to a shared `Shell` for tree, chat, composer,
  modals, context menu. Source: `crates/rumble-next/src/paradigm/{modern,
  mumble, luna}.rs`, `src/shell.rs`.
- **Adapters** — `src/adapters.rs` is a pure read-only converter from
  `rumble_protocol::State` to display models (tree nodes, chat entries,
  breadcrumbs, connection summary). Same `BackendHandle<NativePlatform>` as
  rumble-egui.
- **Identity** — `src/identity.rs` shares `<config_dir>/identity.json` with
  rumble-egui (same `ProjectDirs::from("com", "rumble", "Rumble")`), but only
  understands the `LocalPlaintext` variant; `LocalEncrypted` and `SshAgent`
  records are rejected and a fresh key is regenerated.
- **Connection** — `src/connect_view.rs` shows a form (server / username /
  password) and the certificate-approval block when
  `ConnectionState::CertificatePending`. Hidden once connected; paradigm
  takes over.
- **Toasts** — `src/toasts.rs` fully wired; `app.rs` drains
  `state.permission_denied`, `state.kicked`, and connection transitions.
- **Headless test** — `examples/screenshot.rs` renders a paradigm to PNG
  via `egui_kittest` (wgpu). Supports `--paradigm`, `--dark`, `--out`,
  `--size`, plus `RUMBLE_NEXT_AUTOCONNECT=1` for end-to-end smoke runs.

## Shared-code strategy: `rumble-desktop-shell`

Most of the gaps below are not "build new feature" but "lift existing
rumble-egui code into a place rumble-next can call it too." We're
introducing a new crate, **`rumble-desktop-shell`**, that holds the
non-rendering desktop client plumbing both clients need:

- Settings (schema + JSON load/save).
- Identity / key manager (plaintext, encrypted, SSH agent, first-run
  wizard state machine).
- Hotkeys (global hotkey service + Wayland portal backend).
- Toast manager (currently duplicated in `rumble-egui/toasts.rs` and
  `rumble-next/src/toasts.rs`).

Why one crate rather than three small ones:
- All three are desktop-only and always consumed together by a GUI
  client. Splitting yields three tiny always-coupled crates.
- Keeps `rumble-desktop` focused on its sole job (the `Platform` trait
  impl that the engine consumes) — engine code should not pull in
  hotkey or settings deps.
- WASM is deferred indefinitely (`docs/v2-progress.md`); the *schemas*
  (Settings struct, identity types) lift easily later if needed, but the
  *backends* would have to be rewritten for the browser regardless.

Platform variance handled by feature flags inside the crate:

| Feature           | Pulls in                                  | Default |
| ----------------- | ----------------------------------------- | ------- |
| `ssh-agent`       | `ssh-agent-lib`, `ssh-key`, `service-binding` | on (Unix) |
| `encrypted-keys`  | `argon2`, `chacha20poly1305`              | on      |
| `wayland-portal`  | `ashpd`, `futures-util`                   | on (Linux) |
| `global-hotkeys`  | `global-hotkey`                           | on      |

**Splitting trigger:** if a non-GUI consumer appears (e.g.
`mumble-bridge` or `server` wants to share identity-format parsing),
split that piece into its own crate then. Until that pressure exists,
one crate is simpler.

References below to "extract into shared crate" mean
`rumble-desktop-shell` unless noted otherwise.

## Parity matrix

Legend: ✅ done · 🟡 partial · ❌ missing.

| Area                              | rumble-egui | rumble-next |
| --------------------------------- | ----------- | ----------- |
| Connect form + cert approval      | ✅          | ✅          |
| Saved/recent servers              | ✅          | ✅          |
| Auto-connect on launch            | settings    | settings    |
| Identity: plaintext key           | ✅          | ✅          |
| Identity: encrypted (Argon2)      | ✅          | ✅ (load only) |
| Identity: SSH agent               | ✅          | ✅ (load only) |
| First-run identity wizard         | ✅          | ❌          |
| Chat send / receive               | ✅          | ✅          |
| Chat: room / DM / tree            | ✅          | ✅ (slash commands) |
| Chat: timestamps + 5 formats      | ✅          | ✅          |
| Chat: history sync request        | ✅          | ✅          |
| Chat: image paste / file share    | ✅          | ❌ (placeholder only) |
| Voice: mute / deafen toggles      | ✅          | ✅          |
| Voice: PTT (latched button)       | ✅          | ✅          |
| Voice: PTT (hold-to-talk hotkey)  | ✅          | ✅          |
| Audio device picker               | ✅          | ✅          |
| Audio level meters                | ✅          | 🟡 dB readout, no bar |
| TX pipeline editor (denoise/VAD)  | ✅          | ❌          |
| Encoder settings (bitrate, FEC…)  | ✅          | ❌          |
| Sound effects (settings + playback)| ✅         | 🟡 connect/disconnect/mute |
| Per-user volume slider            | ✅          | ✅          |
| Rooms: tree, join, create, rename, delete | ✅  | ✅          |
| Rooms: edit description           | ✅          | ❌          |
| Rooms: drag-drop reparent         | ✅          | ❌          |
| User: kick (modal w/ reason)      | ✅          | 🟡 empty reason |
| User: ban (modal w/ duration)     | ✅          | ✅          |
| User: server-mute / deafen        | ✅          | 🟡 local mute only |
| Sudo / elevate to superuser       | ✅          | ❌          |
| Group management UI               | ✅          | ❌          |
| Per-room ACL editor               | ✅          | ❌          |
| Global hotkeys (Win/Mac/X11)      | ✅          | ✅          |
| Global hotkeys (Wayland portal)   | ✅          | ✅          |
| Settings persistence (JSON)       | ✅          | ✅          |
| Settings: 11 categories           | ✅          | 6 (Connection, Voice, Devices, Chat, Statistics, About) |
| Statistics panel (RTT/jitter/loss)| ✅          | ✅          |
| RPC server (Unix socket)          | ✅          | ❌          |
| Test harness library export       | ✅          | ✅ (basic)  |
| Toast notifications               | ✅          | ✅          |
| Paradigm/theme switcher           | n/a         | ✅          |
| Light/dark toggle                 | n/a         | ✅          |

## Detailed gap inventory

### 1. Identity & key management

**Missing:**
- Encrypted-at-rest keys (Argon2 + ChaCha20 from `rumble-egui/key_manager.rs`).
  Today, an encrypted `identity.json` triggers a silent regeneration — that
  would orphan the user's server registration.
- SSH agent backed signing (production users in egui rely on this).
- First-run wizard: user picks plaintext / encrypted / agent and (if agent)
  selects which key by SHA256 fingerprint + comment.
- Public-key copy-to-clipboard in About panel.

**To do:** lift `rumble-egui/src/key_manager.rs` into
`rumble-desktop-shell::identity`, gated by the `encrypted-keys` and
`ssh-agent` features. The signer indirection in `Identity::signer()`
already returns a `SigningCallback`, so the only client-visible change is
"how do we obtain that callback".

### 2. Connection

**Missing:**
- Saved-servers list / recent-servers menu. The form is ephemeral; the only
  way to persist a server today is to set `RUMBLE_NEXT_AUTOCONNECT=1`.
- Settings-driven auto-connect (egui has `autoconnect` flag).
- Per-server saved password.
- `accepted_certificates` storage so the cert-approval modal doesn't fire
  every connect. Today each launch re-prompts.
- Custom certificate path UI (egui exposes `custom_cert_path`); rumble-next
  reads `RUMBLE_SERVER_CERT_PATH` env or dev cert paths.

**To do:** introduce `SettingsStore` (see §7), add `recent_servers:
Vec<ServerProfile>` with `{addr, name, password?, accepted_cert_fp?}`,
and replace the connect form with a list + "new server" entry.

### 3. Voice & audio

**Wired:** mute/deafen self, PTT latch button, voice mode radio
(PTT/Continuous), input/output device pickers, refresh devices.

**Missing:**
- **Hold-to-talk PTT**, the most-used voice feature. The shell.rs comment at
  line 473 acknowledges this belongs to "the hotkey layer, not mouse click."
  Cannot ship without this.
- Input level meter on the device picker.
- Per-user volume slider in the tree (egui: -40dB..+40dB).
- TX pipeline editor (denoise / VAD / gain modulation), with the dynamic
  processor schema from `rumble-audio`.
- Encoder settings (bitrate, complexity, jitter buffer depth, FEC, packet
  loss percent).
- Sound effects (connect/disconnect/join/leave/message/mute/unmute) — both
  the settings and the actual playback.

### 4. Chat

**Wired:** send/receive room messages, DM via context menu modal, system
messages with tone (Join/Disc/Info), 50-message ephemeral buffer.

**Missing:**
- `/msg <user> <text>` and `/tree <text>` slash commands in the composer.
- Configurable timestamp formats (egui has 5: 24h, 12h, date+24h, date+12h,
  relative). Today rumble-next hard-codes `HH:MM`.
- "Sync history" button + `auto_sync_history` setting.
- Clipboard-image paste → file-share pipeline (📋 button in egui).
- Manual file share → file picker → upload.
- Click-to-enlarge image viewer with zoom/pan.
- Permission gating (disable composer when no `TEXT_MESSAGE`).

### 5. Rooms / users / ACL / admin

**Wired:** tree render with talking + muted + deafened badges; join, create,
rename, delete rooms; mute (local) / kick / ban / DM users via right-click.

**Missing:**
- Edit room description modal.
- Drag-and-drop reparenting (with confirmation).
- Kick reason field (today sends empty reason).
- Server-mute / server-deafen others (egui MUTE_DEAFEN permission).
- Sudo / elevate prompt + 🔑 toolbar button.
- Group management panel (create, edit perms grid, add/remove members,
  delete) — gated by `MANAGE_ACL`.
- Per-room ACL editor modal (group rows × permission columns, grant/deny,
  apply-here / apply-subs, inherit toggle).

The protocol and server already support all of this (see
`memory/MEMORY.md` "ACL System" section); this is purely UI work.

### 6. Hotkeys

**Missing entirely.** Rumble-egui has ~1200 LOC across `hotkeys.rs` and
`portal_hotkeys.rs`:
- PTT, toggle mute, toggle deafen — bindable per-action.
- Conflict detection in the binding UI.
- Windows / macOS / Linux X11 via `global-hotkey` crate.
- Linux Wayland via XDG `GlobalShortcuts` portal (KDE/GNOME/Hyprland) with
  graceful fallback.
- Settings: `global_hotkeys_enabled` master toggle.

This is the highest-impact gap, blocked by no other work. Lift
`hotkeys.rs` + `portal_hotkeys.rs` into `rumble-desktop-shell::hotkeys`
behind the `global-hotkeys` and `wayland-portal` features. The API
surface is a small `HotkeyService` that both clients consume.

### 7. Settings persistence

**Missing entirely.** Rumble-next reads live state from `state.audio` and
emits `Command::*` on change — nothing survives a restart. Egui persists to
`<config_dir>/Rumble/settings.json` via `serde_json` and saves on every
mutation (`app.rs:1130-1145`).

Categories egui has, rumble-next doesn't: Sounds, Processing, Encoder, Chat,
File Transfer, Keyboard, Statistics, Admin (8 of 11). Connection, Voice,
Devices, About are present in some form.

**To do:** define `Settings` struct in `rumble-desktop-shell::settings`
mirroring egui's; load on `App::new`; save in a debounced background
task. Both clients consume the same store so settings stay portable as
users migrate. Most fields can land as the matching features land — but
`recent_servers`, `accepted_certificates`, and `auto_connect` should
land first since they unblock a real user workflow.

### 8. File transfer

**Missing entirely** (placeholder `Media::File` / `Media::Image` enum
variants in `data.rs` are decoration only). Egui has:
- Share-file picker (top menu + chat 📋 button).
- Auto-download rules (MIME pattern + size limit).
- Speed limits (down/up).
- Seed-after-download toggle.
- Cleanup-on-exit toggle.
- Transfers panel (list, progress, pause/resume).
- Magnet-link URL handling.

Backend support is in `rumble-client` via the `FileTransferPlugin` injected
through the Platform trait factory. The same plugin instance is what egui
uses; rumble-next wiring is just UI + plugging the plugin into `BackendHandle`
construction.

### 9. RPC server / test harness

- Egui ships `rpc_client.rs` plus the `--rpc-server` daemon flag in `main.rs`
  for Unix-socket remote control (status, mute, join-room, send-chat,
  share-file, …). Used by `harness-cli`. **Missing in rumble-next.**
- Egui exports `harness::TestHarness` from `lib.rs` for in-process tests
  (with the `test-harness` feature). **Rumble-next has only the standalone
  `examples/screenshot.rs`** — fine for CI screenshots, not enough for
  programmatic interaction tests. Need an equivalent harness export so
  `harness-cli` can target rumble-next.

### 10. Diagnostics

- Statistics settings tab (RTT, jitter, packet loss, input level,
  processing time). Missing in rumble-next.

## Suggested bringup order

Each step is small enough to land standalone and unblocks downstream work:

0. **Stand up `rumble-desktop-shell`.** Create the crate with empty
   `settings`, `identity`, `hotkeys`, `toasts` modules and the feature
   flags listed above. Move `rumble-next/src/toasts.rs` into it as the
   first occupant (small, already duplicated, proves the wiring). Both
   clients depend on it from this point on.
1. **Settings persistence skeleton.** `Settings` struct + load/save +
   `SettingsStore` in `rumble-desktop-shell::settings`. Even if it only
   persists `paradigm + dark + recent_servers + accepted_certificates`,
   that immediately fixes the cert-reprompt and saves the user's chosen
   UI.
2. **Recent servers / cert pinning.** Replace the connect form with a
   list + "new server". Auto-connect setting replaces the env var.
3. **Hotkey service.** Lift `rumble-egui/hotkeys.rs` +
   `portal_hotkeys.rs` into `rumble-desktop-shell::hotkeys`. Wire PTT
   hold-to-talk, then toggle-mute / toggle-deafen. Migrate rumble-egui
   to consume the shared service in the same PR so we don't run two
   copies.
4. **Identity unification.** Lift `key_manager.rs` into
   `rumble-desktop-shell::identity`. Plaintext + encrypted + SSH agent.
   First-run wizard. Migrate rumble-egui in the same PR.
5. **Chat polish.** Slash commands, timestamp formats, history sync,
   permission gating.
6. **Audio depth.** Level meter, per-user volume, TX pipeline editor,
   encoder settings, sound effects.
7. **Admin/ACL.** Sudo elevate, group management, per-room ACL editor,
   server-mute, room description, drag-reparent.
8. **File transfer UI.** Share picker, transfers panel, auto-download
   rules.
9. **RPC + harness.** Unix-socket RPC server, library `TestHarness`
   export, `harness-cli` adoption.
10. **Diagnostics.** Statistics tab.

Steps 1–3 are the critical path for daily-driver usability; the rest can
land in any order.

## Delivered vs deferred (2026-04-25)

What's actually shipped against each step:

### Step 0 — `rumble-desktop-shell` skeleton ✅

New crate with `settings`, `identity`, `hotkeys`, `toasts` modules and
the four feature flags from the table above. Both clients depend on it.
Toasts migrated end-to-end as the first occupant.

### Step 1 — Settings persistence skeleton ✅

`Settings` / `SettingsStore` with synchronous save-on-mutation, JSON at
`<config>/desktop-shell.json`, `#[serde(flatten)]` `_extra` map for
forward compatibility. Now persists `paradigm`, `dark`, `recent_servers`,
`accepted_certificates`, `auto_connect_addr`, `keyboard`, `chat`, `sfx`.
3 unit tests cover load/save and unknown-field round-tripping.

### Step 2 — Recent servers + cert pinning ✅

Connect view rewritten as list-or-form. Saved servers sorted by
`last_used_unix`, "+ New server" entry, address locked when editing a
saved server. Certificate acceptance pins by `(server_name, fingerprint)`
into `accepted_certificates`; `App::new` re-trusts every saved cert
into `ConnectConfig::accepted_certs`, so reconnects skip the prompt.
`auto_connect_addr` setting drives launch-time auto-connect (env var
remains for headless smoke).

### Step 3 — Global hotkey service ✅

`rumble-desktop-shell::hotkeys` houses `HotkeyManager` + the Wayland
portal backend, both behind `cfg`-free APIs. rumble-egui re-exports;
its old `hotkeys.rs` and `portal_hotkeys.rs` are deleted. rumble-next
constructs the manager in `App::new` (with a single-worker tokio
runtime for portal IO), polls events each frame, and dispatches to
`Command::{StartTransmit, StopTransmit, SetMuted, SetDeafened}`. Window-
focused fallback covers portal-less Wayland sessions.

### Step 4 — Identity unification ✅ (wizard deferred)

`key_manager.rs` lifted in full to `rumble-desktop-shell::identity`,
including SSH-agent + encrypted-key support gated by features. rumble-
egui re-exports for backwards compat; the old file is deleted. rumble-
next's `Identity` now wraps `KeyManager`, so encrypted and SSH-agent
identities written by rumble-egui load straight in.

**Deferred:** The first-run wizard UI lives in `rumble-egui/first_run.rs`
and is tied to that paradigm's render code. rumble-next still falls back
to "generate plaintext key on first launch." A paradigm-aware wizard
for rumble-next is a follow-up — the hard part (key types + storage) is
done.

### Step 5 — Chat polish ✅

- `/msg <user> <text>` and `/tree <text>` slash commands in the
  composer. Bad usernames surface as a local system message (cheaper
  than a toast, easy to ignore).
- `TimestampFormat` enum in `Settings` with 5 variants (24h / 12h /
  date+24h / date+12h / relative). New Chat tab in the settings panel
  toggles visibility and picks the format.
- "⟳ sync" button next to the composer dispatches
  `Command::RequestChatHistory`. Auto-sync-on-join setting persists.

**Not done:** `TEXT_MESSAGE` permission gating on the composer — the
ACL state shape needs a careful look first.

### Step 6 — Audio depth (partial) 🟡

Landed:
- Per-user volume slider (-40 dB..+40 dB, 1 dB step) in the user
  context menu, reading from `state.audio.per_user_rx[id].volume_db`.
- Sound effects: `SfxSettings { enabled, volume }` in shared settings;
  Connect / Disconnect / Mute / Unmute play through `Command::PlaySfx`
  with the persisted volume; UI toggle + slider in the Voice page.

Deferred:
- TX pipeline editor (denoise / VAD / gain). Needs the dynamic
  processor schema from `rumble-audio` plumbed into the UI; large
  enough to be its own step.
- Encoder settings (bitrate, complexity, FEC, jitter buffer). Small
  but no UI design yet.
- Input-level meter on the device picker. The data is on
  `state.audio.input_level_db` (visible on the Statistics tab); a
  proper meter widget can come with the device-picker rework.

### Step 7 — Admin / ACL ❌ deferred

Sudo elevate prompt, group management, per-room ACL editor, server-mute
others, room description editor, drag-reparent. The protocol surface is
all in place (see `memory/MEMORY.md` "ACL System"), but the UI is large
enough — particularly the ACL editor's group × permission grid — that
landing it without thoughtful design work would be a net negative. Pick
this up when you have a clear UX for the editor.

### Step 8 — File transfer UI ❌ deferred

Same story: the backend (`FileTransferPlugin`) is wired through the
Platform trait and rumble-egui consumes it, but the UI work (share
picker, transfers panel with progress / pause / resume, auto-download
rules editor) is substantial and benefits from end-to-end design rather
than incremental landings.

### Step 9 — TestHarness export + RPC server (partial) 🟡

Landed: `rumble-next::TestHarness`, gated behind a `test-harness`
feature, wraps `egui_kittest::Harness` around `App` with a thin API
(`step()`, `run_frames(n)`, `render()`, `app()` / `app_mut()`,
`kittest_mut()` escape hatch). Gives `harness-cli` a foothold to start
adopting rumble-next.

Deferred: Unix-socket RPC server. The protocol is small and obvious
(lift `rpc_client.rs` from rumble-egui), but it's not on the critical
path for daily-driver use; it lights up batch automation. Worth
revisiting when `harness-cli` needs to drive rumble-next from outside
the test process.

### Step 10 — Statistics tab ✅

New "Statistics" page in the settings panel surfaces input level (dB),
transmit / mute / deafen state, connection summary, and live peer count.
Uses a small `stat_row` helper (mono value, muted label).

## Critical-path follow-ups

In rough priority for the next push:

1. **rumble-egui adopts the shared `SettingsStore`.** Today the two
   clients write to different files (`settings.json` vs
   `desktop-shell.json`). When egui migrates, identity + recent servers
   + cert trust become genuinely portable.
2. **First-run wizard for rumble-next.** Mirror egui's flow but render
   it through the active paradigm. Prerequisite for shipping rumble-
   next as the default client.
3. **TX pipeline editor** (step 6 leftover) — the denoise/VAD UI is
   what users miss most after switching from Mumble.
4. **ACL editor** (step 7) — needs design before code.
5. **File transfer UI** (step 8) — needs design before code.
