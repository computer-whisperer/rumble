//! File transfer relay plugin — store-and-serve cache model.
//!
//! Clients upload files to the server, which caches them per-room.
//! Other clients in the room can then fetch cached files by transfer ID.
//!
//! ## Upload flow
//!
//! 1. Client opens a `"file-relay"` stream to the server.
//! 2. Sends type discriminator `0x01`, then length-prefixed [`RelayUpload`],
//!    then raw file bytes.
//! 3. Server stores in room-scoped cache.
//! 4. Server responds with length-prefixed [`RelayUploadResponse`].
//!
//! ## Fetch flow
//!
//! 1. Client opens a `"file-relay"` stream to the server.
//! 2. Sends type discriminator `0x02`, then length-prefixed [`RelayFetch`].
//! 3. Server responds with length-prefixed [`RelayFetchResponse`], then raw
//!    file bytes (if found).

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::Result;
use api::proto;
use dashmap::DashMap;
use prost::Message;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::{
    plugin::{ServerCtx, ServerPlugin, StreamHeader},
    state::ClientHandle,
};

/// A cached file entry.
struct CachedFile {
    room_id: String,
    file_name: String,
    file_size: u64,
    mime: String,
    data: Vec<u8>,
    created_at: Instant,
}

/// Configuration for the relay cache.
pub struct RelayCacheConfig {
    /// Max age of a cache entry (default 30 min).
    pub ttl: Duration,
    /// Evict entries when their room empties (default true).
    pub evict_on_room_clear: bool,
    /// Max total cache size in bytes (default 500 MB).
    pub max_total_size: u64,
    /// Max single file size in bytes (default 100 MB).
    pub max_file_size: u64,
}

impl Default for RelayCacheConfig {
    fn default() -> Self {
        Self {
            ttl: Duration::from_secs(30 * 60),
            evict_on_room_clear: true,
            max_total_size: 500 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
        }
    }
}

/// Server-side file relay plugin using a store-and-serve cache model.
///
/// Uploaded files are held in memory keyed by transfer ID. Fetch requests
/// look up the cache and stream the data back to the requester.
pub struct FileTransferRelayPlugin {
    /// Room-scoped file cache: transfer_id -> CachedFile.
    cache: Arc<DashMap<String, CachedFile>>,
    /// Configuration.
    config: RelayCacheConfig,
    /// Total bytes currently cached (for quota enforcement).
    total_cached: Arc<AtomicU64>,
    /// Parent cancellation token — cancelled on stop().
    cancel: CancellationToken,
}

impl FileTransferRelayPlugin {
    /// Create a new relay plugin with default configuration.
    pub fn new() -> Self {
        Self::with_config(RelayCacheConfig::default())
    }

    /// Create a new relay plugin with the given configuration.
    pub fn with_config(config: RelayCacheConfig) -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            config,
            total_cached: Arc::new(AtomicU64::new(0)),
            cancel: CancellationToken::new(),
        }
    }

    /// Handle an upload stream.
    async fn handle_upload(
        &self,
        mut send: quinn::SendStream,
        mut recv: quinn::RecvStream,
        sender: &Arc<ClientHandle>,
        ctx: &ServerCtx,
    ) -> Result<()> {
        // Read length-prefixed RelayUpload proto.
        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let msg_len = u32::from_be_bytes(len_buf) as usize;
        if msg_len > 64 * 1024 {
            anyhow::bail!("RelayUpload message too large ({msg_len} bytes)");
        }

        let mut msg_buf = vec![0u8; msg_len];
        recv.read_exact(&mut msg_buf).await?;
        let upload = proto::RelayUpload::decode(&msg_buf[..])?;

        let user_id = sender.user_id;
        let transfer_id = upload.transfer_id.clone();

        info!(
            user_id,
            transfer_id = %transfer_id,
            file = %upload.file_name,
            size = upload.file_size,
            room = %upload.room_id,
            "file relay upload request"
        );

        // Validate file size limit.
        if upload.file_size > self.config.max_file_size {
            let resp = proto::RelayUploadResponse {
                status: proto::RelayResult::TooLarge.into(),
                error: format!(
                    "file too large: {} bytes (max {})",
                    upload.file_size, self.config.max_file_size
                ),
            };
            Self::write_response(&mut send, &resp.encode_to_vec()).await?;
            return Ok(());
        }

        // Check total cache quota (approximate — race-free enough for our purposes).
        let current_total = self.total_cached.load(Ordering::Relaxed);
        if current_total + upload.file_size > self.config.max_total_size {
            let resp = proto::RelayUploadResponse {
                status: proto::RelayResult::TooLarge.into(),
                error: "server cache full".to_owned(),
            };
            Self::write_response(&mut send, &resp.encode_to_vec()).await?;
            return Ok(());
        }

        // Validate that the user is in the claimed room.
        if let Some(actual_room) = ctx.get_user_room(user_id) {
            let actual_room_str = actual_room.to_string();
            if actual_room_str != upload.room_id {
                let resp = proto::RelayUploadResponse {
                    status: proto::RelayResult::Error.into(),
                    error: format!("room mismatch: you are in {actual_room_str}, not {}", upload.room_id),
                };
                Self::write_response(&mut send, &resp.encode_to_vec()).await?;
                return Ok(());
            }
        }

        // Read raw file data from the stream.
        let file_size = upload.file_size as usize;
        let mut data = Vec::with_capacity(file_size.min(32 * 1024 * 1024)); // pre-alloc capped at 32MB
        let mut remaining = file_size;
        let mut buf = vec![0u8; 64 * 1024];

        while remaining > 0 {
            let to_read = remaining.min(buf.len());
            match recv.read(&mut buf[..to_read]).await {
                Ok(Some(n)) if n > 0 => {
                    data.extend_from_slice(&buf[..n]);
                    remaining -= n;
                }
                Ok(Some(_)) => continue, // zero-length read, retry
                Ok(None) => {
                    // Stream closed early.
                    let resp = proto::RelayUploadResponse {
                        status: proto::RelayResult::Error.into(),
                        error: format!("stream closed after {} of {} bytes", data.len(), file_size),
                    };
                    Self::write_response(&mut send, &resp.encode_to_vec()).await?;
                    return Ok(());
                }
                Err(e) => {
                    warn!(transfer_id = %transfer_id, "upload read error: {e}");
                    return Err(e.into());
                }
            }
        }

        // Store in cache.
        let actual_size = data.len() as u64;
        self.cache.insert(
            transfer_id.clone(),
            CachedFile {
                room_id: upload.room_id.clone(),
                file_name: upload.file_name.clone(),
                file_size: actual_size,
                mime: upload.mime.clone(),
                data,
                created_at: Instant::now(),
            },
        );
        self.total_cached.fetch_add(actual_size, Ordering::Relaxed);

        info!(
            transfer_id = %transfer_id,
            bytes = actual_size,
            "file cached"
        );

        // Send success response.
        let resp = proto::RelayUploadResponse {
            status: proto::RelayResult::Ok.into(),
            error: String::new(),
        };
        Self::write_response(&mut send, &resp.encode_to_vec()).await?;

        Ok(())
    }

    /// Handle a fetch stream.
    async fn handle_fetch(&self, mut send: quinn::SendStream, mut recv: quinn::RecvStream) -> Result<()> {
        // Read length-prefixed RelayFetch proto.
        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let msg_len = u32::from_be_bytes(len_buf) as usize;
        if msg_len > 64 * 1024 {
            anyhow::bail!("RelayFetch message too large ({msg_len} bytes)");
        }

        let mut msg_buf = vec![0u8; msg_len];
        recv.read_exact(&mut msg_buf).await?;
        let fetch = proto::RelayFetch::decode(&msg_buf[..])?;

        let transfer_id = fetch.transfer_id.clone();

        debug!(transfer_id = %transfer_id, "file relay fetch request");

        // Look up in cache.
        let entry = self.cache.get(&transfer_id);
        match entry {
            Some(cached) => {
                let resp = proto::RelayFetchResponse {
                    status: proto::RelayResult::Ok.into(),
                    file_name: cached.file_name.clone(),
                    file_size: cached.file_size,
                    mime: cached.mime.clone(),
                    error: String::new(),
                };
                let resp_bytes = resp.encode_to_vec();
                Self::write_response(&mut send, &resp_bytes).await?;

                // Write raw file bytes.
                send.write_all(&cached.data).await?;
                send.finish()?;

                info!(
                    transfer_id = %transfer_id,
                    bytes = cached.file_size,
                    "file served from cache"
                );
            }
            None => {
                let resp = proto::RelayFetchResponse {
                    status: proto::RelayResult::NotFound.into(),
                    file_name: String::new(),
                    file_size: 0,
                    mime: String::new(),
                    error: "transfer not found or expired".to_owned(),
                };
                Self::write_response(&mut send, &resp.encode_to_vec()).await?;
                send.finish()?;

                debug!(transfer_id = %transfer_id, "fetch: not found");
            }
        }

        Ok(())
    }

    /// Write a length-prefixed protobuf response on a send stream.
    async fn write_response(send: &mut quinn::SendStream, data: &[u8]) -> Result<()> {
        let len_bytes = (data.len() as u32).to_be_bytes();
        send.write_all(&len_bytes).await?;
        send.write_all(data).await?;
        Ok(())
    }

    /// Remove all cache entries for a given room.
    fn evict_room(&self, room_id: &str) {
        let to_remove: Vec<String> = self
            .cache
            .iter()
            .filter(|entry| entry.value().room_id == room_id)
            .map(|entry| entry.key().clone())
            .collect();

        for tid in &to_remove {
            if let Some((_, cached)) = self.cache.remove(tid) {
                self.total_cached.fetch_sub(cached.file_size, Ordering::Relaxed);
            }
        }

        if !to_remove.is_empty() {
            info!(room_id, count = to_remove.len(), "evicted room cache entries");
        }
    }
}

impl Default for FileTransferRelayPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ServerPlugin for FileTransferRelayPlugin {
    fn name(&self) -> &str {
        "file-relay"
    }

    async fn on_message(
        &self,
        _envelope: &proto::Envelope,
        _sender: &Arc<ClientHandle>,
        _ctx: &ServerCtx,
    ) -> Result<bool> {
        // The cache model has no control-stream messages.
        Ok(false)
    }

    async fn on_stream(
        &self,
        _header: StreamHeader,
        send: quinn::SendStream,
        mut recv: quinn::RecvStream,
        sender: &Arc<ClientHandle>,
        ctx: &ServerCtx,
    ) -> Result<()> {
        // Read the type discriminator byte.
        let mut type_buf = [0u8; 1];
        recv.read_exact(&mut type_buf).await?;

        let child_cancel = self.cancel.child_token();

        match type_buf[0] {
            0x01 => {
                // Upload
                tokio::select! {
                    _ = child_cancel.cancelled() => {
                        debug!("upload stream cancelled by shutdown");
                    }
                    result = self.handle_upload(send, recv, sender, ctx) => {
                        if let Err(e) = result {
                            warn!(user_id = sender.user_id, "upload error: {e}");
                        }
                    }
                }
            }
            0x02 => {
                // Fetch
                tokio::select! {
                    _ = child_cancel.cancelled() => {
                        debug!("fetch stream cancelled by shutdown");
                    }
                    result = self.handle_fetch(send, recv) => {
                        if let Err(e) = result {
                            warn!(user_id = sender.user_id, "fetch error: {e}");
                        }
                    }
                }
            }
            other => {
                warn!(user_id = sender.user_id, type_byte = other, "unknown relay stream type");
            }
        }

        Ok(())
    }

    async fn on_disconnect(&self, client: &Arc<ClientHandle>, ctx: &ServerCtx) {
        if !self.config.evict_on_room_clear {
            return;
        }

        let user_id = client.user_id;

        // Find the room the user was in.
        let room_id = match ctx.get_user_room(user_id) {
            Some(r) => r,
            None => return,
        };

        // Check if the room is now empty (this user is the last one leaving).
        // get_room_members returns a snapshot; the user may still be in it.
        let members = ctx.get_room_members(room_id);
        let remaining = members.iter().filter(|uid| **uid != user_id).count();

        if remaining == 0 {
            let room_str = room_id.to_string();
            self.evict_room(&room_str);
        }
    }

    async fn start(&self, _ctx: &ServerCtx) -> Result<()> {
        // Spawn the TTL sweep task as a child of our cancellation token.
        let cache = self.cache.clone();
        let total_cached = self.total_cached.clone();
        let ttl = self.config.ttl;
        let child = self.cancel.child_token();

        tokio::spawn(async move {
            let interval = Duration::from_secs(60);
            loop {
                tokio::select! {
                    _ = child.cancelled() => {
                        debug!("TTL sweep task cancelled");
                        break;
                    }
                    _ = tokio::time::sleep(interval) => {
                        let now = Instant::now();
                        let expired: Vec<String> = cache
                            .iter()
                            .filter(|entry| now.duration_since(entry.value().created_at) > ttl)
                            .map(|entry| entry.key().clone())
                            .collect();

                        for tid in &expired {
                            if let Some((_, cached)) = cache.remove(tid) {
                                total_cached.fetch_sub(cached.file_size, Ordering::Relaxed);
                            }
                        }

                        if !expired.is_empty() {
                            info!(count = expired.len(), "TTL sweep evicted entries");
                        }
                    }
                }
            }
        });

        info!(
            ttl_secs = self.config.ttl.as_secs(),
            max_file = self.config.max_file_size,
            max_total = self.config.max_total_size,
            "file relay plugin started"
        );

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        self.cancel.cancel();
        info!("file relay plugin stopped");
        Ok(())
    }
}
