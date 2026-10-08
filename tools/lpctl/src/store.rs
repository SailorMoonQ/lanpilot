//! lpctl's own identity and the servers it has paired with. A dev tool: the
//! identity is a plain 0600 file (the real apps use the OS keystore).

use crate::LpctlError;
use lanpilot_core::identity::{Identity, PublicKey};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct KnownServer {
    pub public_key: PublicKey,
    pub name: String,
    pub addrs: Vec<IpAddr>,
    pub port: u16,
}

#[derive(Serialize, Deserialize)]
struct Record {
    public_key: String,
    name: String,
    addrs: Vec<IpAddr>,
    port: u16,
}

pub struct ClientStore {
    dir: PathBuf,
    identity: Identity,
    servers: Vec<KnownServer>,
}

fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path)?.write_all(bytes)
}

impl ClientStore {
    pub fn open(dir: PathBuf) -> Result<Self, LpctlError> {
        std::fs::create_dir_all(&dir)?;
        let id_path = dir.join("identity.key");
        let identity = match std::fs::read(&id_path) {
            Ok(b) => {
                let secret: [u8; 32] = b
                    .as_slice()
                    .try_into()
                    .map_err(|_| LpctlError::Usage(format!("{} is corrupt", id_path.display())))?;
                Identity::from_secret_bytes(&secret)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let id = Identity::generate();
                write_private(&id_path, &id.secret_bytes())?;
                id
            }
            Err(e) => return Err(e.into()),
        };
        let servers = match std::fs::read_to_string(dir.join("servers.json")) {
            Ok(t) => serde_json::from_str::<Vec<Record>>(&t)?
                .into_iter()
                .map(|r| {
                    let bytes =
                        hex::decode(&r.public_key).map_err(|e| LpctlError::Usage(e.to_string()))?;
                    let public_key = PublicKey::from_slice(&bytes)
                        .map_err(|e| LpctlError::Usage(e.to_string()))?;
                    Ok(KnownServer {
                        public_key,
                        name: r.name,
                        addrs: r.addrs,
                        port: r.port,
                    })
                })
                .collect::<Result<Vec<_>, LpctlError>>()?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            dir,
            identity,
            servers,
        })
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    pub fn servers(&self) -> &[KnownServer] {
        &self.servers
    }

    fn save(&self) -> Result<(), LpctlError> {
        let records: Vec<Record> = self
            .servers
            .iter()
            .map(|s| Record {
                public_key: hex::encode(s.public_key.as_bytes()),
                name: s.name.clone(),
                addrs: s.addrs.clone(),
                port: s.port,
            })
            .collect();
        std::fs::write(
            self.dir.join("servers.json"),
            serde_json::to_string_pretty(&records)?,
        )?;
        Ok(())
    }

    pub fn upsert(&mut self, server: KnownServer) -> Result<(), LpctlError> {
        self.servers.retain(|s| s.public_key != server.public_key);
        self.servers.push(server);
        self.save()
    }

    pub fn remove(&mut self, public_key: &PublicKey) -> Result<(), LpctlError> {
        self.servers.retain(|s| s.public_key != *public_key);
        self.save()
    }

    pub fn find(&self, selector: &str) -> Result<&KnownServer, LpctlError> {
        let sel = selector.to_lowercase();
        let matches: Vec<&KnownServer> = self
            .servers
            .iter()
            .filter(|s| {
                s.name.to_lowercase() == sel
                    || (sel.len() >= 4 && s.public_key.short_id().starts_with(&sel))
            })
            .collect();
        match matches.as_slice() {
            [one] => Ok(one),
            [] => Err(LpctlError::Usage(format!(
                "no paired server matches '{selector}'"
            ))),
            _ => Err(LpctlError::Usage(format!(
                "'{selector}' matches several servers"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(b: u8, name: &str) -> KnownServer {
        KnownServer {
            public_key: PublicKey([b; 32]),
            name: name.into(),
            addrs: vec!["10.0.0.2".parse().unwrap()],
            port: 45810,
        }
    }

    #[test]
    fn identity_and_servers_persist() {
        let dir = tempfile::tempdir().unwrap();
        let key = {
            let mut s = ClientStore::open(dir.path().to_owned()).unwrap();
            s.upsert(server(1, "Desk")).unwrap();
            s.identity().public_key()
        };
        let s = ClientStore::open(dir.path().to_owned()).unwrap();
        assert_eq!(s.identity().public_key(), key);
        assert_eq!(s.servers(), &[server(1, "Desk")]);
    }

    #[test]
    fn find_by_prefix_or_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = ClientStore::open(dir.path().to_owned()).unwrap();
        s.upsert(server(1, "Desk")).unwrap();
        s.upsert(server(2, "Laptop")).unwrap();
        let desk_id = PublicKey([1; 32]).short_id();
        assert_eq!(s.find(&desk_id[..4]).unwrap().name, "Desk");
        assert_eq!(s.find("laptop").unwrap().name, "Laptop");
        assert!(s.find("abc").is_err(), "prefix shorter than 4 chars");
        assert!(s.find("nothing").is_err());
    }
}
