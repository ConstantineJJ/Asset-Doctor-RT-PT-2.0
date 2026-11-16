//! Verified new-path GLB storage; no dialogs, renderer or history ownership.
use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentStamp, DocumentTag, ErrorCode,
};
use ad_formats::VerifiedGlb;
#[cfg(windows)]
use std::{
    fs::OpenOptions,
    time::{SystemTime, UNIX_EPOCH},
};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

/// Receipt for a published file. It does not change a document's saved state.
pub struct SavedCopy {
    stamp: DocumentStamp,
    path: PathBuf,
    cleanup_warning: bool,
}
impl SavedCopy {
    pub fn tag(&self) -> DocumentTag {
        self.stamp.tag()
    }
    /// Checks document identity/version, not subsequent changes to the saved file.
    pub fn is_current(&self, document: &AssetDocument) -> bool {
        self.stamp.is_current(document)
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn cleanup_warning(&self) -> bool {
        self.cleanup_warning
    }
}
fn io_error(e: std::io::Error) -> AssetError {
    AssetError::new(ErrorCode::Io, format!("Save Copy IO: {e}"))
}
struct Staging {
    file: Option<File>,
    path: PathBuf,
    cleanup_pending: bool,
}
impl Staging {
    #[cfg(not(windows))]
    fn create(_: &Path, _: &Cancellation) -> AssetResult<Self> {
        Err(AssetError::new(
            ErrorCode::Unsupported,
            "Save Copy storage currently requires Windows handle-bound cleanup",
        ))
    }
    #[cfg(windows)]
    fn create(parent: &Path, cancel: &Cancellation) -> AssetResult<Self> {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| AssetError::new(ErrorCode::Io, e.to_string()))?
            .as_nanos();
        for attempt in 0..64 {
            cancel.check()?;
            let path = parent.join(format!(
                ".asset-doctor-copy-{}-{time}-{attempt}.tmp",
                std::process::id()
            ));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                use windows_sys::Win32::{
                    Foundation::{GENERIC_READ, GENERIC_WRITE},
                    Storage::FileSystem::{DELETE, FILE_SHARE_READ},
                };
                // FILE_SHARE_READ: other handles cannot write or delete staging.
                // DELETE access permits cleanup by this handle without releasing
                // its protection and resolving the staging pathname again.
                options
                    .share_mode(FILE_SHARE_READ)
                    .access_mode(GENERIC_READ | GENERIC_WRITE | DELETE);
            }
            match options.open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        file: Some(file),
                        path,
                        cleanup_pending: true,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(io_error(e)),
            }
        }
        Err(AssetError::new(
            ErrorCode::Io,
            "Cannot allocate exclusive Save Copy staging file",
        ))
    }
    fn cleanup(&mut self) -> std::io::Result<()> {
        if !self.cleanup_pending {
            return Ok(());
        }
        self.cleanup_pending = false;
        #[cfg(windows)]
        let result = mark_staging_for_deletion(self.file.as_ref().expect("owned staging handle"));
        #[cfg(not(windows))]
        let result = Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Windows cleanup required",
        ));
        self.file.take();
        result
    }
}
#[cfg(windows)]
fn mark_staging_for_deletion(file: &File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
    };
    let info = FILE_DISPOSITION_INFO { DeleteFile: 1 };
    // SAFETY: file owns a valid, open Windows handle with DELETE access. info is
    // the documented disposition structure, alive for the synchronous call;
    // its exact size is supplied. The handle remains owned by File afterward.
    let success = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle() as _,
            FileDispositionInfo,
            (&info as *const FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    };
    if success == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Created,
    Written,
    Ready,
}

/// Save to a new absolute .glb path. Hard-link publication never replaces a file.
/// Failure before publication cleans staging; after publication return a receipt,
/// even if cancellation arrives or staging cleanup fails.
pub fn save_copy(
    verified: &VerifiedGlb,
    document: &AssetDocument,
    destination: &Path,
    cancel: &Cancellation,
) -> AssetResult<SavedCopy> {
    save_copy_with(verified, document, destination, cancel, |_, _| Ok(()))
}
fn save_copy_with(
    verified: &VerifiedGlb,
    document: &AssetDocument,
    destination: &Path,
    cancel: &Cancellation,
    mut hook: impl FnMut(Stage, &mut File) -> AssetResult<()>,
) -> AssetResult<SavedCopy> {
    verified.verify_current(document, cancel)?;
    if !destination.is_absolute()
        || !destination
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("glb"))
    {
        return Err(AssetError::new(
            ErrorCode::Io,
            "Save Copy requires a new absolute .glb path",
        ));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| AssetError::new(ErrorCode::Io, "Missing destination directory"))?
        .canonicalize()
        .map_err(io_error)?;
    let filename = destination
        .file_name()
        .ok_or_else(|| AssetError::new(ErrorCode::Io, "Missing destination filename"))?;
    let destination = parent.join(filename);
    let mut staging = Staging::create(&parent, cancel)?;
    let result = (|| {
        let file = staging.file.as_mut().expect("owned staging handle");
        hook(Stage::Created, file)?;
        for chunk in verified.bytes().chunks(64 << 10) {
            cancel.check()?;
            file.write_all(chunk).map_err(io_error)?;
        }
        file.sync_all().map_err(io_error)?;
        hook(Stage::Written, file)?;
        if file.metadata().map_err(io_error)?.len() != verified.bytes().len() as u64 {
            return Err(AssetError::new(
                ErrorCode::VerificationFailed,
                "Saved staging length differs from verified GLB",
            ));
        }
        file.seek(SeekFrom::Start(0)).map_err(io_error)?;
        let mut buffer = [0u8; 64 << 10];
        for chunk in verified.bytes().chunks(buffer.len()) {
            cancel.check()?;
            file.read_exact(&mut buffer[..chunk.len()])
                .map_err(io_error)?;
            if &buffer[..chunk.len()] != chunk {
                return Err(AssetError::new(
                    ErrorCode::VerificationFailed,
                    "Saved staging bytes differ from verified GLB",
                ));
            }
        }
        hook(Stage::Ready, file)?;
        verified.verify_current(document, cancel)?;
        fs::hard_link(&staging.path, &destination).map_err(io_error)?;
        // Publication commit point. There are no fallible/cancellation checks below.
        Ok(())
    })();
    let cleanup = staging.cleanup();
    match result {
        Ok(()) => Ok(SavedCopy {
            stamp: document.stamp(),
            path: destination,
            cleanup_warning: cleanup.is_err(),
        }),
        Err(mut error) => {
            if let Err(e) = cleanup {
                error.message.push_str(&format!(
                    "; staging cleanup failed at {}: {e}",
                    staging.path.display()
                ));
            }
            Err(error)
        }
    }
}
#[cfg(all(test, windows))]
mod tests;
