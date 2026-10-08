//! Paired devices (spec 4.4). Stored apart from the config file because it is
//! state, not something users edit by hand.

use crate::AgentError;
use crate::fsutil::write_atomic;
use lanpilot_core::identity::PublicKey;
use lanpilot_core::proto::v1::Os;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq)]
pub struct PairedDevice {
    pub public_key: PublicKey,
    pub name: String,
    pub os: Os,
    pub paired_at: u64,
}

#[derive(Serialize, Deserialize)]
struct FileFormat {
    version: u32,
    devices: Vec<DeviceRecord>,
}

#[derive(Serialize, Deserialize)]
struct DeviceRecord {
    public_key: String,
    name: String,
    os: String,
    paired_at: u64,
}

fn os_to_str(os: Os) -> &'static str {
    match os {
        Os::Windows => "windows",
        Os::Linux => "linux",
        Os::Macos => "macos",
        Os::Ios => "ios",
        Os::Android => "android",
        Os::Unspecified => "unknown",
    }
}

fn os_from_str(s: &str) -> Os {
    match s {
        "windows" => Os::Windows,
        "linux" => Os::Linux,
        "macos" => Os::Macos,
        "ios" => Os::Ios,
        "android" => Os::Android,
        _ => Os::Unspecified,
    }
}

pub struct DeviceStore {
    path: PathBuf,
    devices: Mutex<HashMap<PublicKey, PairedDevice>>,
    removed: broadcast::Sender<PublicKey>,
}

impl DeviceStore {
    pub fn open(path: PathBuf) -> Result<Arc<Self>, AgentError> {
        let devices = Self::read_file(&path)?;
        let (removed, _) = broadcast::channel(64);
        Ok(Arc::new(Self {
            path,
            devices: Mutex::new(devices),
            removed,
        }))
    }

    fn read_file(path: &Path) -> Result<HashMap<PublicKey, PairedDevice>, AgentError> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
            Err(e) => return Err(e.into()),
        };
        let bad = |what: String| AgentError::Config(format!("{}: {what}", path.display()));
        let file: FileFormat = serde_json::from_str(&text).map_err(|e| bad(e.to_string()))?;
        let mut map = HashMap::new();
        for r in file.devices {
            let bytes = hex::decode(&r.public_key).map_err(|e| bad(e.to_string()))?;
            let key = PublicKey::from_slice(&bytes).map_err(|e| bad(e.to_string()))?;
            map.insert(
                key,
                PairedDevice {
                    public_key: key,
                    name: r.name,
                    os: os_from_str(&r.os),
                    paired_at: r.paired_at,
                },
            );
        }
        Ok(map)
    }

    fn write_file(&self, devices: &HashMap<PublicKey, PairedDevice>) -> Result<(), AgentError> {
        let mut records: Vec<DeviceRecord> = devices
            .values()
            .map(|d| DeviceRecord {
                public_key: hex::encode(d.public_key.as_bytes()),
                name: d.name.clone(),
                os: os_to_str(d.os).to_owned(),
                paired_at: d.paired_at,
            })
            .collect();
        records.sort_by(|a, b| {
            a.paired_at
                .cmp(&b.paired_at)
                .then(a.public_key.cmp(&b.public_key))
        });
        let json = serde_json::to_string_pretty(&FileFormat {
            version: 1,
            devices: records,
        })
        .map_err(|e| AgentError::Config(e.to_string()))?;
        write_atomic(&self.path, json.as_bytes())?;
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PublicKey, PairedDevice>> {
        self.devices.lock().expect("device store lock poisoned")
    }

    pub fn contains(&self, key: &PublicKey) -> bool {
        self.lock().contains_key(key)
    }

    pub fn list(&self) -> Vec<PairedDevice> {
        let mut v: Vec<PairedDevice> = self.lock().values().cloned().collect();
        v.sort_by_key(|d| d.paired_at);
        v
    }

    pub fn add(&self, key: PublicKey, name: &str, os: Os) -> Result<(), AgentError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let mut devices = self.lock();
        let paired_at = devices.get(&key).map_or(now, |d| d.paired_at);
        devices.insert(
            key,
            PairedDevice {
                public_key: key,
                name: name.to_owned(),
                os,
                paired_at,
            },
        );
        self.write_file(&devices)
    }

    pub fn remove(&self, key: &PublicKey) -> Result<bool, AgentError> {
        let mut devices = self.lock();
        if devices.remove(key).is_none() {
            return Ok(false);
        }
        self.write_file(&devices)?;
        drop(devices);
        let _ = self.removed.send(*key);
        Ok(true)
    }

    pub fn remove_by_short_id(&self, short_id: &str) -> Result<Option<PairedDevice>, AgentError> {
        let found = self
            .lock()
            .values()
            .find(|d| d.public_key.short_id() == short_id)
            .cloned();
        match found {
            Some(d) => {
                self.remove(&d.public_key)?;
                Ok(Some(d))
            }
            None => Ok(None),
        }
    }

    pub fn reload(&self) -> Result<Vec<PublicKey>, AgentError> {
        let fresh = Self::read_file(&self.path)?;
        let mut devices = self.lock();
        let gone: Vec<PublicKey> = devices
            .keys()
            .filter(|k| !fresh.contains_key(*k))
            .copied()
            .collect();
        *devices = fresh;
        drop(devices);
        for k in &gone {
            let _ = self.removed.send(*k);
        }
        Ok(gone)
    }

    pub fn subscribe_removed(&self) -> broadcast::Receiver<PublicKey> {
        self.removed.subscribe()
    }

    pub fn watch(self: &Arc<Self>) -> Result<notify::RecommendedWatcher, AgentError> {
        use notify::{RecursiveMode, Watcher};
        let store = Arc::downgrade(self);
        let target = self.path.file_name().map(ToOwned::to_owned);
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(event) = res else { return };
            let touches_file = event
                .paths
                .iter()
                .any(|p| p.file_name().map(ToOwned::to_owned) == target);
            if !touches_file {
                return;
            }
            if let Some(store) = store.upgrade()
                && let Err(e) = store.reload()
            {
                tracing::warn!("reloading paired devices failed: {e}");
            }
        })
        .map_err(|e| AgentError::Config(e.to_string()))?;
        let dir = self
            .path
            .parent()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&dir)?;
        watcher
            .watch(&dir, RecursiveMode::NonRecursive)
            .map_err(|e| AgentError::Config(e.to_string()))?;
        Ok(watcher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn key(b: u8) -> PublicKey {
        PublicKey([b; 32])
    }

    #[test]
    fn add_persists_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("devices.json");
        let store = DeviceStore::open(path.clone()).unwrap();
        assert!(!store.contains(&key(1)));
        store.add(key(1), "iPhone", Os::Ios).unwrap();
        assert!(store.contains(&key(1)));
        let reopened = DeviceStore::open(path).unwrap();
        let list = reopened.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "iPhone");
        assert_eq!(list[0].os, Os::Ios);
    }

    #[test]
    fn re_adding_updates_name_without_duplicating() {
        let dir = tempfile::tempdir().unwrap();
        let store = DeviceStore::open(dir.path().join("devices.json")).unwrap();
        store.add(key(1), "old", Os::Ios).unwrap();
        store.add(key(1), "new", Os::Ios).unwrap();
        let list = store.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "new");
    }

    #[tokio::test]
    async fn remove_by_short_id_broadcasts() {
        let dir = tempfile::tempdir().unwrap();
        let store = DeviceStore::open(dir.path().join("devices.json")).unwrap();
        store.add(key(2), "Pixel", Os::Android).unwrap();
        let mut rx = store.subscribe_removed();
        let removed = store
            .remove_by_short_id(&key(2).short_id())
            .unwrap()
            .unwrap();
        assert_eq!(removed.name, "Pixel");
        assert_eq!(rx.recv().await.unwrap(), key(2));
        assert!(
            store
                .remove_by_short_id("0000000000000000")
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn reload_detects_external_removal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("devices.json");
        let running = DeviceStore::open(path.clone()).unwrap();
        running.add(key(3), "a", Os::Ios).unwrap();
        running.add(key(4), "b", Os::Ios).unwrap();
        let mut rx = running.subscribe_removed();

        // Another process (the CLI) removes one device.
        let cli = DeviceStore::open(path).unwrap();
        cli.remove(&key(3)).unwrap();

        assert_eq!(running.reload().unwrap(), vec![key(3)]);
        assert_eq!(rx.recv().await.unwrap(), key(3));
        assert!(!running.contains(&key(3)));
        assert!(running.contains(&key(4)));
    }

    #[tokio::test]
    async fn watcher_reacts_to_file_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("devices.json");
        let running = DeviceStore::open(path.clone()).unwrap();
        running.add(key(5), "a", Os::Ios).unwrap();
        let mut rx = running.subscribe_removed();
        let _watcher = running.watch().unwrap();

        DeviceStore::open(path).unwrap().remove(&key(5)).unwrap();

        let got = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("watcher noticed within 5 s")
            .unwrap();
        assert_eq!(got, key(5));
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("devices.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(matches!(
            DeviceStore::open(path),
            Err(AgentError::Config(_))
        ));
    }
}
