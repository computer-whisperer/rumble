//! Persistent storage abstraction for settings and state.

/// Key-value persistent storage.
///
/// Implementations may use the filesystem, `localStorage` (web), or any
/// other persistent store available on the platform.
pub trait PersistentStorage: Send + Sync + 'static {
    /// Load a value by key. Returns `None` if the key does not exist.
    fn load(&self, key: &str) -> anyhow::Result<Option<String>>;

    /// Save a value under the given key, overwriting any previous value.
    fn save(&self, key: &str, value: &str) -> anyhow::Result<()>;

    /// Delete the value for a key. No error if the key doesn't exist.
    fn delete(&self, key: &str) -> anyhow::Result<()>;

    /// List all keys that start with the given prefix.
    fn list_keys(&self, prefix: &str) -> anyhow::Result<Vec<String>>;
}
