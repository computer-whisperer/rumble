//! Filesystem-based persistent storage.
//!
//! Implements [`PersistentStorage`] by storing key-value pairs as JSON in
//! a file within the platform config directory.

use std::{collections::HashMap, path::PathBuf, sync::RwLock};

use anyhow::Context;
use directories::ProjectDirs;
use rumble_client_traits::storage::PersistentStorage;

/// JSON file-based persistent storage.
///
/// Data is stored as a flat `HashMap<String, serde_json::Value>` serialized
/// to `rumble_storage.json` in the platform configuration directory
/// (e.g. `~/.config/Rumble/Rumble/` on Linux).
///
/// Uses an internal `RwLock` to satisfy the `&self` trait methods while
/// allowing mutation of the in-memory map.
pub struct FileStorage {
    path: PathBuf,
    data: RwLock<HashMap<String, serde_json::Value>>,
}

impl FileStorage {
    /// Create a new `FileStorage`, loading existing data if the file exists.
    pub fn new() -> anyhow::Result<Self> {
        let project_dirs =
            ProjectDirs::from("com", "Rumble", "Rumble").context("failed to determine platform config directory")?;

        let config_dir = project_dirs.config_dir();
        std::fs::create_dir_all(config_dir)
            .with_context(|| format!("failed to create config dir: {}", config_dir.display()))?;

        let path = config_dir.join("rumble_storage.json");

        let data = if path.exists() {
            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read storage file: {}", path.display()))?;
            serde_json::from_str(&contents)
                .with_context(|| format!("failed to parse storage file: {}", path.display()))?
        } else {
            HashMap::new()
        };

        Ok(Self {
            path,
            data: RwLock::new(data),
        })
    }

    /// Write the current data to disk. Caller must already hold the lock.
    fn flush(&self, data: &HashMap<String, serde_json::Value>) -> anyhow::Result<()> {
        let contents = serde_json::to_string_pretty(data).context("failed to serialize storage data")?;
        std::fs::write(&self.path, contents)
            .with_context(|| format!("failed to write storage file: {}", self.path.display()))?;
        Ok(())
    }
}

impl PersistentStorage for FileStorage {
    fn load(&self, key: &str) -> anyhow::Result<Option<String>> {
        let data = self.data.read().map_err(|e| anyhow::anyhow!("lock poisoned: {}", e))?;
        match data.get(key) {
            Some(value) => {
                let s = serde_json::to_string(value).context("failed to serialize value")?;
                Ok(Some(s))
            }
            None => Ok(None),
        }
    }

    fn save(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let parsed: serde_json::Value =
            serde_json::from_str(value).with_context(|| format!("invalid JSON value for key '{}'", key))?;
        let mut data = self.data.write().map_err(|e| anyhow::anyhow!("lock poisoned: {}", e))?;
        data.insert(key.to_string(), parsed);
        self.flush(&data)
    }

    fn delete(&self, key: &str) -> anyhow::Result<()> {
        let mut data = self.data.write().map_err(|e| anyhow::anyhow!("lock poisoned: {}", e))?;
        data.remove(key);
        self.flush(&data)
    }

    fn list_keys(&self, prefix: &str) -> anyhow::Result<Vec<String>> {
        let data = self.data.read().map_err(|e| anyhow::anyhow!("lock poisoned: {}", e))?;
        let keys = data.keys().filter(|k| k.starts_with(prefix)).cloned().collect();
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a FileStorage backed by a temporary directory.
    fn temp_storage(dir: &std::path::Path) -> FileStorage {
        let path = dir.join("rumble_storage.json");
        FileStorage {
            path,
            data: RwLock::new(HashMap::new()),
        }
    }

    #[test]
    fn save_and_load() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = temp_storage(tmp.path());

        storage.save("key1", r#""hello""#).unwrap();
        let val = storage.load("key1").unwrap();
        assert_eq!(val, Some(r#""hello""#.to_string()));
    }

    #[test]
    fn load_missing_key() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = temp_storage(tmp.path());

        let val = storage.load("nonexistent").unwrap();
        assert_eq!(val, None);
    }

    #[test]
    fn delete_key() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = temp_storage(tmp.path());

        storage.save("key1", r#"42"#).unwrap();
        assert!(storage.load("key1").unwrap().is_some());

        storage.delete("key1").unwrap();
        assert_eq!(storage.load("key1").unwrap(), None);
    }

    #[test]
    fn delete_nonexistent_key() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = temp_storage(tmp.path());

        // Should not error
        storage.delete("nope").unwrap();
    }

    #[test]
    fn list_keys_with_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = temp_storage(tmp.path());

        storage.save("settings.audio", r#"true"#).unwrap();
        storage.save("settings.video", r#"false"#).unwrap();
        storage.save("user.name", r#""alice""#).unwrap();

        let mut keys = storage.list_keys("settings.").unwrap();
        keys.sort();
        assert_eq!(keys, vec!["settings.audio", "settings.video"]);

        let all = storage.list_keys("").unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn persistence_across_instances() {
        let tmp = tempfile::tempdir().unwrap();

        {
            let storage = temp_storage(tmp.path());
            storage.save("persist", r#"{"a":1}"#).unwrap();
        }

        // Reload from same file
        let path = tmp.path().join("rumble_storage.json");
        let contents = std::fs::read_to_string(&path).unwrap();
        let data: HashMap<String, serde_json::Value> = serde_json::from_str(&contents).unwrap();
        let reloaded = FileStorage {
            path,
            data: RwLock::new(data),
        };

        let val = reloaded.load("persist").unwrap();
        assert_eq!(val, Some(r#"{"a":1}"#.to_string()));
    }

    #[test]
    fn save_complex_json() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = temp_storage(tmp.path());

        let complex = r#"{"users":["alice","bob"],"count":2}"#;
        storage.save("data", complex).unwrap();

        let loaded = storage.load("data").unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&loaded).unwrap();
        assert_eq!(parsed["count"], 2);
        assert_eq!(parsed["users"][0], "alice");
    }
}
