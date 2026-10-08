//! Secrets at rest (spec 4.1): DPAPI on Windows, a 0600 file on Linux.

use crate::AgentError;
use crate::fsutil::write_atomic;
use lanpilot_core::identity::Identity;
use lanpilot_core::pairing::password::validate_password;
use std::path::Path;

pub fn read_secret(path: &Path) -> Result<Option<Vec<u8>>, AgentError> {
    match std::fs::read(path) {
        Ok(stored) => Ok(Some(platform::unprotect(&stored)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn write_secret(path: &Path, bytes: &[u8]) -> Result<(), AgentError> {
    write_atomic(path, &platform::protect(bytes)?)?;
    Ok(())
}

pub fn load_or_create_identity(path: &Path) -> Result<Identity, AgentError> {
    if let Some(bytes) = read_secret(path)? {
        let secret: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| AgentError::Secret(format!("{} is corrupt", path.display())))?;
        return Ok(Identity::from_secret_bytes(&secret));
    }
    let identity = Identity::generate();
    write_secret(path, &identity.secret_bytes())?;
    Ok(identity)
}

pub fn load_password(path: &Path) -> Result<Option<String>, AgentError> {
    read_secret(path)?
        .map(|b| {
            String::from_utf8(b).map_err(|_| AgentError::Secret("password is not UTF-8".into()))
        })
        .transpose()
}

pub fn store_password(path: &Path, password: &str) -> Result<(), AgentError> {
    validate_password(password)?;
    write_secret(path, password.as_bytes())
}

pub fn clear_password(path: &Path) -> Result<(), AgentError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(not(windows))]
mod platform {
    use crate::AgentError;

    // Protection on Linux is the 0600 mode set by `write_atomic` (spec 4.1).
    pub fn protect(bytes: &[u8]) -> Result<Vec<u8>, AgentError> {
        Ok(bytes.to_vec())
    }

    pub fn unprotect(bytes: &[u8]) -> Result<Vec<u8>, AgentError> {
        Ok(bytes.to_vec())
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod platform {
    use crate::AgentError;
    use windows_sys::Win32::Foundation::{HLOCAL, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };

    fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: bytes.len() as u32,
            pbData: bytes.as_ptr() as *mut u8,
        }
    }

    /// Copies a DPAPI output blob and frees it with LocalFree.
    fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        if out.pbData.is_null() {
            return Vec::new();
        }
        // SAFETY: `pbData` is non-null, so DPAPI returned a valid buffer of
        // `cbData` bytes allocated with LocalAlloc; we copy it before freeing
        // it exactly once.
        unsafe {
            let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData as HLOCAL);
            v
        }
    }

    pub fn protect(bytes: &[u8]) -> Result<Vec<u8>, AgentError> {
        let input = blob(bytes);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: `input` points at `bytes`, valid for the call; DPAPI only
        // reads it. `out` is a valid out-parameter. Null pointers are allowed
        // for the optional description, entropy, reserved and prompt args.
        let ok = unsafe {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return Err(AgentError::Secret(
                std::io::Error::last_os_error().to_string(),
            ));
        }
        Ok(take(out))
    }

    pub fn unprotect(bytes: &[u8]) -> Result<Vec<u8>, AgentError> {
        let input = blob(bytes);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: same contract as `protect`; the description out-pointer is null.
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return Err(AgentError::Secret(format!(
                "cannot decrypt (was it written by another Windows user?): {}",
                std::io::Error::last_os_error()
            )));
        }
        Ok(take(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_round_trips_and_is_not_plaintext_on_windows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s");
        assert_eq!(read_secret(&path).unwrap(), None);
        write_secret(&path, b"top secret bytes").unwrap();
        assert_eq!(read_secret(&path).unwrap().unwrap(), b"top secret bytes");
        let raw = std::fs::read(&path).unwrap();
        if cfg!(windows) {
            assert!(
                !raw.windows(6).any(|w| w == b"secret"),
                "DPAPI output must not contain plaintext"
            );
        }
    }

    #[test]
    fn identity_is_created_once_and_reloaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identity.key");
        let a = load_or_create_identity(&path).unwrap();
        let b = load_or_create_identity(&path).unwrap();
        assert_eq!(a.public_key(), b.public_key());
    }

    #[test]
    fn corrupt_identity_is_an_error_not_a_silent_new_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identity.key");
        write_secret(&path, b"short").unwrap();
        assert!(matches!(
            load_or_create_identity(&path),
            Err(AgentError::Secret(_))
        ));
    }

    #[test]
    fn password_store_validates_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pw");
        assert_eq!(load_password(&path).unwrap(), None);
        assert!(matches!(
            store_password(&path, "12345"),
            Err(AgentError::Password(_))
        ));
        store_password(&path, "密码六个字符").unwrap();
        assert_eq!(
            load_password(&path).unwrap().as_deref(),
            Some("密码六个字符")
        );
        clear_password(&path).unwrap();
        assert_eq!(load_password(&path).unwrap(), None);
        clear_password(&path).unwrap(); // idempotent
    }
}
