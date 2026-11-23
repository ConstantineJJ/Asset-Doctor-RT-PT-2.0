//! SHA-256 through Windows CNG (BCrypt), so no hashing crate is needed.
use crate::Result;
use std::io::Read;
use std::path::Path;

/// Lower-case hex SHA-256 of a byte stream.
pub fn of_reader(mut input: impl Read) -> Result<String> {
    let mut hasher = Hasher::new()?;
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n])?;
    }
    hasher.finish()
}

pub fn of_bytes(bytes: &[u8]) -> Result<String> {
    of_reader(bytes)
}

pub fn of_file(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    of_reader(file)
}

#[cfg(windows)]
struct Hasher(windows_sys::Win32::Security::Cryptography::BCRYPT_HASH_HANDLE);

#[cfg(windows)]
impl Hasher {
    fn new() -> Result<Self> {
        use windows_sys::Win32::Security::Cryptography::{
            BCRYPT_SHA256_ALG_HANDLE, BCryptCreateHash,
        };
        let mut handle = std::ptr::null_mut();
        // SAFETY: the SHA-256 pseudo-handle needs no provider; CNG allocates
        // the hash object itself when no buffer is given.
        let status = unsafe {
            BCryptCreateHash(
                BCRYPT_SHA256_ALG_HANDLE,
                &mut handle,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
                0,
                0,
            )
        };
        if status < 0 || handle.is_null() {
            return Err(format!("SHA-256 is unavailable (status {status:#x})"));
        }
        Ok(Self(handle))
    }
    fn update(&mut self, data: &[u8]) -> Result<()> {
        use windows_sys::Win32::Security::Cryptography::BCryptHashData;
        for chunk in data.chunks(u32::MAX as usize) {
            // SAFETY: the handle is live and `chunk` is valid for its length.
            let status = unsafe { BCryptHashData(self.0, chunk.as_ptr(), chunk.len() as u32, 0) };
            if status < 0 {
                return Err(format!("SHA-256 failed (status {status:#x})"));
            }
        }
        Ok(())
    }
    fn finish(self) -> Result<String> {
        use windows_sys::Win32::Security::Cryptography::BCryptFinishHash;
        let mut digest = [0u8; 32];
        // SAFETY: the handle is live and the output buffer holds 32 bytes.
        let status = unsafe { BCryptFinishHash(self.0, digest.as_mut_ptr(), 32, 0) };
        if status < 0 {
            return Err(format!("SHA-256 failed (status {status:#x})"));
        }
        Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
    }
}

#[cfg(windows)]
impl Drop for Hasher {
    fn drop(&mut self) {
        // SAFETY: the handle came from BCryptCreateHash and is freed once.
        unsafe {
            windows_sys::Win32::Security::Cryptography::BCryptDestroyHash(self.0);
        }
    }
}

#[cfg(not(windows))]
struct Hasher;

#[cfg(not(windows))]
impl Hasher {
    fn new() -> Result<Self> {
        Err("SHA-256 needs Windows CNG".into())
    }
    fn update(&mut self, _: &[u8]) -> Result<()> {
        Ok(())
    }
    fn finish(self) -> Result<String> {
        Err("SHA-256 needs Windows CNG".into())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn known_vectors() {
        assert_eq!(
            of_bytes(b"").unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            of_bytes(b"abc").unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // Longer than the 1 MiB read buffer: one million 'a'.
        assert_eq!(
            of_bytes(&vec![b'a'; 1_000_000]).unwrap(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        let big = vec![7u8; (1 << 20) * 2 + 5];
        assert_eq!(of_bytes(&big).unwrap(), of_reader(big.as_slice()).unwrap());
    }
}
