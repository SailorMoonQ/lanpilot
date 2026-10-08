//! Crash-safe file writes: temp file in the same directory, then rename.

use std::io::Write;
use std::path::Path;

/// Writes `bytes` to `path` via a uniquely named temp file (0600 on Unix) in
/// the same directory, then renames it over the target. The temp file removes
/// itself if any step fails.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    if path.file_name().is_none() {
        return Err(std::io::Error::other("path has no file name"));
    }
    std::fs::create_dir_all(dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    // On Windows, replacing a file that another writer is replacing at the
    // same moment (or that a reader has open) fails transiently with
    // PermissionDenied, so retry briefly. The temp file comes back in the error.
    let mut attempts = 0;
    loop {
        match tmp.persist(path) {
            Ok(_) => break,
            Err(e)
                if cfg!(windows)
                    && e.error.kind() == std::io::ErrorKind::PermissionDenied
                    && attempts < 200 =>
            {
                attempts += 1;
                tmp = e.file;
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(e) => return Err(e.error),
        }
    }
    #[cfg(unix)]
    std::fs::File::open(dir)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/file.txt");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name() != "file.txt")
            .collect();
        assert!(leftovers.is_empty(), "temp files left behind");
    }

    #[test]
    fn concurrent_writers_never_publish_torn_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared.json");
        let contents: Vec<Vec<u8>> = (0..8u8).map(|i| vec![b'a' + i; 4096]).collect();
        std::thread::scope(|s| {
            for c in &contents {
                let path = &path;
                s.spawn(move || {
                    for _ in 0..50 {
                        write_atomic(path, c).unwrap();
                    }
                });
            }
        });
        let got = std::fs::read(&path).unwrap();
        assert!(contents.contains(&got), "torn content published");
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("shared.json")]);
    }

    #[cfg(unix)]
    #[test]
    fn unix_mode_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret");
        write_atomic(&path, b"x").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
