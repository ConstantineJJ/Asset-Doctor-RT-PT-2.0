//! ADR-0049 auto-update: find a newer GitHub release, download and verify
//! its package, unpack it, and (in the updater binary) replace the program
//! files with a backup and rollback.
//!
//! This is the program's only network access. It is limited to the release
//! list of the program's own repository and that list's asset downloads.

pub mod apply;
pub mod feed;
#[cfg(windows)]
mod http;
pub mod release;
pub mod sha256;
pub mod stage;
pub mod version;
pub mod zip;

pub use feed::Feed;
pub use release::Release;
pub use stage::Staged;
pub use version::Version;

/// Errors are short human-readable texts for the status line and the log.
pub type Result<T> = std::result::Result<T, String>;

/// Executable names in the program folder and in the release package.
pub const PROGRAM_EXE: &str = "AssetDoctorRTPT.exe";
pub const UPDATER_EXE: &str = "AssetDoctorRTPT-Updater.exe";

/// `%LOCALAPPDATA%\AssetDoctorRTPT\updates`, or `chosen` (smokes use an
/// owned folder); created on demand.
pub fn updates_root(chosen: Option<&std::path::Path>) -> Result<std::path::PathBuf> {
    let root = match chosen {
        Some(path) => path.to_owned(),
        None => {
            let base = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
            std::path::PathBuf::from(base)
                .join("AssetDoctorRTPT")
                .join("updates")
        }
    };
    std::fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    Ok(root)
}

/// A name unlikely to repeat: process id, the clock and a per-process
/// counter (two threads can read the same clock value).
pub(crate) fn unique_suffix() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{}-{nanos:x}-{n}", std::process::id())
}
