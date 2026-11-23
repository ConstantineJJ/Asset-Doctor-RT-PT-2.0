//! Windows replacement with a verified backup of the actual displaced file.
use super::{Staging, conflict, io_error};
use ad_document::{AssetError, AssetResult, Cancellation, ErrorCode};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, PathBuf, Prefix},
};
use windows_sys::Win32::{
    Foundation::{GENERIC_READ, GENERIC_WRITE},
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, DELETE, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ,
        GetFileInformationByHandle, MOVEFILE_WRITE_THROUGH, MoveFileExW, ReplaceFileW,
    },
};

/// Freeze ordinary writes during verification. Windows needs delete sharing
/// for ReplaceFileW, so another process can still rename over this pathname.
fn guarded_read(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Replacement requires a regular, non-reparse file",
        ));
    }
    Ok(file)
}

fn holds(mut file: &File, expected: &[u8], cancel: &Cancellation) -> AssetResult<bool> {
    if file.metadata().map_err(io_error)?.len() != expected.len() as u64 {
        return Ok(false);
    }
    let mut buffer = [0; 64 << 10];
    for chunk in expected.chunks(buffer.len()) {
        cancel.check()?;
        file.read_exact(&mut buffer[..chunk.len()])
            .map_err(io_error)?;
        if &buffer[..chunk.len()] != chunk {
            return Ok(false);
        }
    }
    Ok(true)
}

fn file_id(file: &File) -> io::Result<(u32, u32, u32)> {
    // SAFETY: the handle remains open and the zeroed C struct is writable.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: the handle and struct are valid through the synchronous call.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((
        info.dwVolumeSerialNumber,
        info.nFileIndexHigh,
        info.nFileIndexLow,
    ))
}

/// Use the extended Win32 syntax without resolving the file on disk. A
/// backup path may not exist yet, and an existing path may be replaced.
fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let absolute = std::path::absolute(path)?;
    let (prefix, skip) = match absolute.components().next() {
        Some(Component::Prefix(component)) => match component.kind() {
            Prefix::Disk(_) => (r"\\?\", 0),
            Prefix::UNC(_, _) => (r"\\?\UNC\", 2),
            kind if kind.is_verbatim() => ("", 0),
            _ => return Err(io::ErrorKind::InvalidInput.into()),
        },
        _ => return Err(io::ErrorKind::InvalidInput.into()),
    };
    let mut wide: Vec<_> = prefix
        .encode_utf16()
        .chain(absolute.as_os_str().encode_wide().skip(skip))
        .collect();
    if wide.contains(&0) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    wide.push(0);
    Ok(wide)
}

fn replace_file(destination: &Path, source: &Path, backup: &Path) -> io::Result<()> {
    let (destination, source, backup) = (wide(destination)?, wide(source)?, wide(backup)?);
    // SAFETY: all paths are NUL-terminated and live through this call. The
    // reserved parameters are null. Staging was synced before publication.
    if unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            source.as_ptr(),
            backup.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn restore_missing(destination: &Path, backup: &Path) -> io::Result<()> {
    let (destination, backup) = (wide(destination)?, wide(backup)?);
    // SAFETY: both path buffers are NUL-terminated. This move has no replace flag.
    if unsafe {
        MoveFileExW(
            backup.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn backup_prefix(destination: &Path) -> std::ffi::OsString {
    let mut name = std::ffi::OsString::from(".");
    name.push(destination.file_name().expect("validated destination"));
    name.push(".asset-doctor-backup-");
    name
}

fn backup_path(destination: &Path, staging: &Path) -> PathBuf {
    let mut name = backup_prefix(destination);
    name.push(staging.file_stem().expect("owned staging"));
    name.push(".glb");
    destination.with_file_name(name)
}

/// After an interrupted Windows replace, the working name can be missing.
/// Only restore a unique backup of the exact revision the session opened.
pub(super) fn recover_missing(
    destination: &Path,
    expected: &[u8],
    cancel: &Cancellation,
) -> AssetResult<()> {
    let Some(parent) = destination
        .parent()
        .filter(|_| destination.file_name().is_some())
    else {
        return Ok(());
    };
    let prefix = backup_prefix(destination);
    let mut matching = None;
    for entry in std::fs::read_dir(parent).map_err(io_error)? {
        cancel.check()?;
        let entry = entry.map_err(io_error)?;
        if !entry
            .file_name()
            .as_encoded_bytes()
            .starts_with(prefix.as_encoded_bytes())
        {
            continue;
        }
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "glb")
            || !entry.file_type().map_err(io_error)?.is_file()
        {
            continue;
        }
        if let Ok(file) = guarded_read(&path)
            && holds(&file, expected, cancel)?
        {
            if matching.is_some() {
                return Err(AssetError::new(
                    ErrorCode::Conflict,
                    "Several backups match the missing working file; choose the right backup manually",
                ));
            }
            matching = Some((path, file));
        }
    }
    if let Some((path, _guard)) = matching {
        cancel.check()?;
        restore_missing(destination, &path)
            .map_err(|error| retain_backup(io_error(error), &path))?;
    }
    Ok(())
}

fn retain_backup(mut error: AssetError, backup: &Path) -> AssetError {
    error
        .message
        .push_str(&format!("; recovery backup: {}", backup.display()));
    error
}

pub(super) fn verify_expected(
    destination: &Path,
    expected: &[u8],
    cancel: &Cancellation,
) -> AssetResult<File> {
    let file = guarded_read(destination).map_err(io_error)?;
    if !holds(&file, expected, cancel)? {
        return Err(conflict());
    }
    Ok(file)
}

pub(super) fn publish(
    staging: &mut Staging,
    destination: &Path,
    expected: &[u8],
    staged_bytes: &[u8],
    cancel: &Cancellation,
) -> AssetResult<PathBuf> {
    let original = verify_expected(destination, expected, cancel)?;
    let backup = backup_path(destination, &staging.path);
    if backup.try_exists().map_err(io_error)? {
        return Err(io_error(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Replacement backup path is already occupied",
        )));
    }
    let owned_id =
        file_id(staging.file.as_ref().expect("owned staging handle")).map_err(io_error)?;
    cancel.check()?;

    // ReplaceFileW needs exclusive access to the staged file. If publication
    // fails, reacquire it for cleanup only when it is still the same file.
    drop(staging.file.take());
    staging.cleanup_pending = false;
    if let Err(error) = replace_file(destination, &staging.path, &backup) {
        if let Ok(file) = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ)
            .access_mode(GENERIC_READ | GENERIC_WRITE | DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&staging.path)
            && file_id(&file).is_ok_and(|id| id == owned_id)
        {
            staging.file = Some(file);
            staging.cleanup_pending = true;
        }
        drop(original);
        let error = io_error(error);
        return Err(
            if backup.is_file() && restore_missing(destination, &backup).is_err() {
                retain_backup(error, &backup)
            } else {
                error
            },
        );
    }
    drop(original);

    // A late cancellation cannot skip the audit of the file actually replaced.
    let backup = audit_backup(destination, backup, expected, staged_bytes)?;
    verify_expected(destination, staged_bytes, &Cancellation::default())
        .map_err(|error| retain_backup(error, &backup))?;
    Ok(backup)
}

fn audit_backup(
    destination: &Path,
    backup: PathBuf,
    expected: &[u8],
    staged_bytes: &[u8],
) -> AssetResult<PathBuf> {
    let after = Cancellation::default();
    let original =
        guarded_read(&backup).map_err(|error| retain_backup(io_error(error), &backup))?;
    match holds(&original, expected, &after) {
        Ok(true) => return Ok(backup),
        Ok(false) => (),
        Err(error) => return Err(retain_backup(error, &backup)),
    }
    drop(original);

    let mut error = AssetError::new(
        ErrorCode::Conflict,
        "The working file was replaced by another program during Save",
    );
    // Preserve any further replacement as well. Roll back only if the
    // working path still contains our bytes, or there is no working path.
    let mut name = backup.as_os_str().to_os_string();
    name.push(".rollback");
    let displaced = PathBuf::from(name);
    let restored = match verify_expected(destination, staged_bytes, &after) {
        Ok(published) if displaced.try_exists().is_ok_and(|exists| !exists) => {
            let result = replace_file(destination, &backup, &displaced);
            drop(published);
            if result.is_err() && displaced.is_file() && !backup.is_file() {
                error
                    .message
                    .push_str(&format!("; displaced file: {}", displaced.display()));
            }
            result.is_ok() || restore_missing(destination, &backup).is_ok()
        }
        _ => restore_missing(destination, &backup).is_ok(),
    };
    if restored {
        error
            .message
            .push_str("; the external version was restored");
        if displaced.is_file() {
            error
                .message
                .push_str(&format!("; rollback copy: {}", displaced.display()));
        }
        Err(error)
    } else {
        Err(retain_backup(error, &backup))
    }
}

#[cfg(test)]
#[path = "replace_tests.rs"]
mod tests;
