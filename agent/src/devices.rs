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

/// Locking model: `io` serializes every file operation inside this process and
/// a sibling `<file>.lock` advisory lock serializes against other processes
/// (the CLI). Each mutation re-reads the file under both locks, applies its
/// change to that fresh copy, writes it, and only then swaps the in-memory map.
/// So the file is never overwritten with stale state, and every key that
/// disappears (by us or externally) is broadcast exactly once, after the locks
/// are released. The `devices` map is locked only briefly, never across I/O.
pub struct DeviceStore {
    path: PathBuf,
    lock_path: PathBuf,
    io: Mutex<()>,
    devices: Mutex<HashMap<PublicKey, PairedDevice>>,
    removed: broadcast::Sender<PublicKey>,
}

impl DeviceStore {
    pub fn open(path: PathBuf) -> Result<Arc<Self>, AgentError> {
        let devices = Self::read_file(&path)?;
        let (removed, _) = broadcast::channel(64);
        let mut lock_name = path.file_name().map(ToOwned::to_owned).unwrap_or_default();
        lock_name.push(".lock");
        let lock_path = path.with_file_name(lock_name);
        Ok(Arc::new(Self {
            path,
            lock_path,
            io: Mutex::new(()),
            devices: Mutex::new(devices),
            removed,
        }))
    }

    /// A missing file reads as an empty set on purpose: deleting the file
    /// revokes every paired device.
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

    /// Run one serialized read-modify-write cycle. `change` edits the freshly
    /// read map and returns extra keys to broadcast plus whether to write the
    /// file. Returns every key that was broadcast.
    fn sync_with<F>(&self, change: F) -> Result<Vec<PublicKey>, AgentError>
    where
        F: FnOnce(&mut HashMap<PublicKey, PairedDevice>) -> (Vec<PublicKey>, bool),
    {
        let announce = {
            let _io = self.io.lock().expect("device store io lock poisoned");
            if let Some(dir) = self.lock_path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let lock_file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&self.lock_path)?;
            lock_file.lock()?;

            let mut fresh = Self::read_file(&self.path)?;
            let mut announce: Vec<PublicKey> = self
                .lock()
                .keys()
                .filter(|k| !fresh.contains_key(*k))
                .copied()
                .collect();
            let (extra, write) = change(&mut fresh);
            for k in extra {
                if !announce.contains(&k) {
                    announce.push(k);
                }
            }
            if write {
                // On failure memory stays untouched so it matches the disk.
                self.write_file(&fresh)?;
            }
            *self.lock() = fresh;
            drop(lock_file);
            announce
        };
        for k in &announce {
            let _ = self.removed.send(*k);
        }
        Ok(announce)
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
        self.sync_with(|fresh| {
            let paired_at = fresh.get(&key).map_or(now, |d| d.paired_at);
            fresh.insert(
                key,
                PairedDevice {
                    public_key: key,
                    name: name.to_owned(),
                    os,
                    paired_at,
                },
            );
            (Vec::new(), true)
        })?;
        Ok(())
    }

    pub fn remove(&self, key: &PublicKey) -> Result<bool, AgentError> {
        let announced = self.sync_with(|fresh| {
            if fresh.remove(key).is_some() {
                (vec![*key], true)
            } else {
                (Vec::new(), false)
            }
        })?;
        Ok(announced.contains(key))
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
        self.sync_with(|_| (Vec::new(), false))
    }

    pub fn subscribe_removed(&self) -> broadcast::Receiver<PublicKey> {
        self.removed.subscribe()
    }

    pub fn watch(self: &Arc<Self>) -> Result<notify::RecommendedWatcher, AgentError> {
        use notify::{RecursiveMode, Watcher};
        let store = Arc::downgrade(self);
        let target = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let relevant = match res {
                Ok(event) => {
                    event.need_rescan()
                        || event.paths.iter().any(|p| {
                            let name = p.file_name().map(|n| n.to_string_lossy());
                            match (&name, &target) {
                                (Some(n), Some(t)) => file_names_match(n, t),
                                _ => false,
                            }
                        })
                }
                Err(e) => {
                    tracing::warn!("device file watcher error: {e}");
                    true
                }
            };
            if !relevant {
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
        // Catch anything that changed between `open` and the watcher starting.
        if let Err(e) = self.reload() {
            tracing::warn!("initial reload of paired devices failed: {e}");
        }
        Ok(watcher)
    }
}

fn file_names_match(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
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
    fn own_write_then_reload_broadcasts_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = DeviceStore::open(dir.path().join("devices.json")).unwrap();
        store.add(key(1), "a", Os::Ios).unwrap();
        let mut rx = store.subscribe_removed();
        assert!(store.reload().unwrap().is_empty());
        assert_eq!(
            rx.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        );
    }

    #[test]
    fn remove_broadcasts_exactly_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = DeviceStore::open(dir.path().join("devices.json")).unwrap();
        store.add(key(1), "a", Os::Ios).unwrap();
        let mut rx = store.subscribe_removed();
        assert!(store.remove(&key(1)).unwrap());
        store.reload().unwrap();
        store.reload().unwrap();
        assert_eq!(rx.try_recv().unwrap(), key(1));
        assert_eq!(
            rx.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        );
    }

    #[test]
    fn external_removal_survives_a_concurrent_add() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("devices.json");
        let running = DeviceStore::open(path.clone()).unwrap();
        running.add(key(1), "a", Os::Ios).unwrap();
        let mut rx = running.subscribe_removed();

        DeviceStore::open(path.clone())
            .unwrap()
            .remove(&key(1))
            .unwrap();
        running.add(key(2), "b", Os::Ios).unwrap();

        let reopened = DeviceStore::open(path).unwrap();
        assert!(reopened.contains(&key(2)));
        assert!(!reopened.contains(&key(1)));
        assert!(!running.contains(&key(1)));
        assert_eq!(rx.try_recv().unwrap(), key(1));
    }

    #[tokio::test]
    async fn watcher_catches_removal_made_before_watch_started() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("devices.json");
        let running = DeviceStore::open(path.clone()).unwrap();
        running.add(key(6), "a", Os::Ios).unwrap();
        let mut rx = running.subscribe_removed();

        DeviceStore::open(path).unwrap().remove(&key(6)).unwrap();
        let _watcher = running.watch().unwrap();

        let got = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("removal noticed within 5 s")
            .unwrap();
        assert_eq!(got, key(6));
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
