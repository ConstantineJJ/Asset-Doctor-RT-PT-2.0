//! Replacing program files with a backup, and putting them back.
use crate::Result;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// What an apply changed, relative to the program folder.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Applied {
    pub replaced: Vec<PathBuf>,
    pub created: Vec<PathBuf>,
}

fn files(root: &Path, at: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(at)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            files(root, &path, out)?;
        } else if let Ok(relative) = path.strip_prefix(root) {
            out.push(relative.to_owned());
        }
    }
    Ok(())
}

/// `fs::copy`, retried while the target is still locked (the old program
/// closing, an antivirus scan) until `wait` runs out.
fn copy_retrying(from: &Path, to: &Path, wait: Duration) -> std::io::Result<()> {
    let started = Instant::now();
    loop {
        match std::fs::copy(from, to) {
            Ok(_) => return Ok(()),
            Err(error)
                if started.elapsed() < wait
                    && (error.kind() == std::io::ErrorKind::PermissionDenied
                        || error.raw_os_error() == Some(32)) =>
            {
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(error) => return Err(error),
        }
    }
}

/// Copies every file of `source` into `target`; a file it replaces is first
/// copied to `backup`. On error returns what was already changed, so the
/// caller can roll back.
pub fn apply(
    source: &Path,
    target: &Path,
    backup: &Path,
    wait: Duration,
) -> std::result::Result<Applied, (String, Applied)> {
    let mut applied = Applied::default();
    let mut list = Vec::new();
    if let Err(error) = files(source, source, &mut list) {
        return Err((format!("cannot read the update: {error}"), applied));
    }
    list.sort();
    for relative in list {
        let to = target.join(&relative);
        let step = (|| -> std::io::Result<bool> {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let existed = to.is_file();
            if existed {
                let saved = backup.join(&relative);
                if let Some(parent) = saved.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&to, &saved)?;
            }
            copy_retrying(&source.join(&relative), &to, wait)?;
            Ok(existed)
        })();
        match step {
            Ok(true) => applied.replaced.push(relative),
            Ok(false) => applied.created.push(relative),
            Err(error) => {
                // A failed copy may have left a partly written file.
                if to.is_file() && backup.join(&relative).is_file() {
                    applied.replaced.push(relative.clone());
                }
                return Err((format!("{}: {error}", to.display()), applied));
            }
        }
    }
    Ok(applied)
}

/// Puts replaced files back from `backup` and removes created ones.
pub fn rollback(target: &Path, backup: &Path, applied: &Applied, wait: Duration) -> Result<()> {
    let mut failed = Vec::new();
    for relative in &applied.replaced {
        if let Err(error) = copy_retrying(&backup.join(relative), &target.join(relative), wait) {
            failed.push(format!("{}: {error}", relative.display()));
        }
    }
    for relative in &applied.created {
        let path = target.join(relative);
        if path.is_file()
            && let Err(error) = std::fs::remove_file(&path)
        {
            failed.push(format!("{}: {error}", relative.display()));
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("rollback incomplete: {}", failed.join("; ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zip::temp_dir;

    fn write(root: &Path, relative: &str, text: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    fn read(root: &Path, relative: &str) -> Option<String> {
        std::fs::read_to_string(root.join(relative)).ok()
    }

    #[test]
    fn apply_replaces_and_adds_files_and_rollback_restores_them() {
        let dir = temp_dir("apply");
        let (source, target, backup) =
            (dir.join("payload"), dir.join("program"), dir.join("backup"));
        write(&target, "AssetDoctorRTPT.exe", "old");
        write(&target, "assets/360.HDR/mine.hdr", "owner's own panorama");
        write(&source, "AssetDoctorRTPT.exe", "new");
        write(&source, "AssetDoctorRTPT-Updater.exe", "updater");
        write(&source, "assets/360.HDR/bundled.hdr", "sky");
        let applied = apply(&source, &target, &backup, Duration::ZERO).unwrap();
        assert_eq!(applied.replaced, [PathBuf::from("AssetDoctorRTPT.exe")]);
        assert_eq!(applied.created.len(), 2);
        assert_eq!(read(&target, "AssetDoctorRTPT.exe").as_deref(), Some("new"));
        assert_eq!(
            read(&target, "assets/360.HDR/mine.hdr").as_deref(),
            Some("owner's own panorama")
        );
        assert_eq!(read(&backup, "AssetDoctorRTPT.exe").as_deref(), Some("old"));
        rollback(&target, &backup, &applied, Duration::ZERO).unwrap();
        assert_eq!(read(&target, "AssetDoctorRTPT.exe").as_deref(), Some("old"));
        assert!(read(&target, "AssetDoctorRTPT-Updater.exe").is_none());
        assert!(read(&target, "assets/360.HDR/bundled.hdr").is_none());
        assert_eq!(
            read(&target, "assets/360.HDR/mine.hdr").as_deref(),
            Some("owner's own panorama")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn a_locked_file_stops_the_apply_and_the_rest_rolls_back() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = temp_dir("apply-lock");
        let (source, target, backup) =
            (dir.join("payload"), dir.join("program"), dir.join("backup"));
        write(&target, "a.exe", "old a");
        write(&target, "b.exe", "old b");
        write(&source, "a.exe", "new a");
        write(&source, "b.exe", "new b");
        // b.exe held without sharing, as a running program would hold it.
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(target.join("b.exe"))
            .unwrap();
        let (error, applied) =
            apply(&source, &target, &backup, Duration::from_millis(300)).unwrap_err();
        assert!(error.contains("b.exe"), "{error}");
        assert_eq!(read(&target, "a.exe").as_deref(), Some("new a"));
        drop(lock);
        rollback(&target, &backup, &applied, Duration::ZERO).unwrap();
        assert_eq!(read(&target, "a.exe").as_deref(), Some("old a"));
        assert_eq!(read(&target, "b.exe").as_deref(), Some("old b"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
