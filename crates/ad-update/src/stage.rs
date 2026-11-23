//! Download, verify and unpack a release into its own staging folder.
use crate::{Feed, PROGRAM_EXE, Release, Result, UPDATER_EXE, release, sha256, zip};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// Upper bound when GitHub does not state the asset size.
const ZIP_LIMIT: u64 = 1 << 30;
const SHA_LIMIT: u64 = 4096;

#[derive(Clone, Debug)]
pub struct Staged {
    pub release: Release,
    /// The staging folder (`updates\<tag>-<unique>`).
    pub folder: PathBuf,
    /// Unpacked program files; the updater runs from here.
    pub payload: PathBuf,
}

impl Staged {
    pub fn updater(&self) -> PathBuf {
        self.payload.join(UPDATER_EXE)
    }
}

/// Folder-safe form of a tag.
fn safe(tag: &str) -> String {
    tag.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Downloads the package and its checksum, checks them and unpacks the
/// package under `root`. `progress(done, total)` reports the download.
pub fn stage(
    feed: &Feed,
    release: &Release,
    root: &Path,
    progress: &dyn Fn(u64, u64),
    cancel: &AtomicBool,
) -> Result<Staged> {
    let mut sha_text = Vec::new();
    feed.download(&release.sha_url, SHA_LIMIT, &mut sha_text, &|_| (), cancel)?;
    let expected = release::parse_sha256_file(&String::from_utf8_lossy(&sha_text))?;
    let limit = if release.zip_size > 0 {
        release.zip_size
    } else {
        ZIP_LIMIT
    };
    let mut bytes = Vec::with_capacity(limit.min(ZIP_LIMIT) as usize);
    let total = release.zip_size;
    feed.download(
        &release.zip_url,
        limit,
        &mut bytes,
        &|done| progress(done, total),
        cancel,
    )?;
    if release.zip_size > 0 && bytes.len() as u64 != release.zip_size {
        return Err("the download is incomplete".into());
    }
    let actual = sha256::of_bytes(&bytes)?;
    if actual != expected {
        return Err(format!(
            "SHA-256 mismatch: expected {expected}, got {actual}"
        ));
    }
    let folder = root.join(format!("{}-{}", safe(&release.tag), crate::unique_suffix()));
    let payload = folder.join("payload");
    let unpacked = zip::extract(&bytes, &payload, zip::Limits::default());
    let staged = unpacked.and_then(|_| {
        for name in [PROGRAM_EXE, UPDATER_EXE] {
            if !payload.join(name).is_file() {
                return Err(format!("the package has no {name}"));
            }
        }
        Ok(Staged {
            release: release.clone(),
            folder: folder.clone(),
            payload: payload.clone(),
        })
    });
    if staged.is_err() {
        let _ = std::fs::remove_dir_all(&folder);
    }
    staged
}

/// Removes old staging folders and all but the newest `keep_backups`
/// backups; `updater.log` stays. Errors are ignored (files may be in use).
pub fn prune(root: &Path, keep_backups: usize) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut backups = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("backup-") {
            let modified = entry.metadata().and_then(|m| m.modified()).ok();
            backups.push((modified, path));
        } else {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
    backups.sort_by_key(|b| std::cmp::Reverse(b.0));
    for (_, path) in backups.into_iter().skip(keep_backups) {
        let _ = std::fs::remove_dir_all(&path);
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::Version;

    /// A local feed with one release `tag` holding `files`.
    pub(crate) fn local_feed(
        dir: &Path,
        tag: &str,
        files: &[(&str, &[u8])],
        sha: Option<&str>,
    ) -> Release {
        let zip = zip::build(files, true);
        std::fs::write(dir.join(release::ZIP_ASSET), &zip).unwrap();
        let hash = sha.map_or_else(|| sha256::of_bytes(&zip).unwrap(), str::to_owned);
        std::fs::write(
            dir.join(release::SHA_ASSET),
            format!("{hash}  {}\n", release::ZIP_ASSET),
        )
        .unwrap();
        let list = serde_json::json!([{
            "tag_name": tag, "draft": false, "prerelease": false, "body": "notes",
            "html_url": "", "name": tag,
            "assets": [
                {"name": release::ZIP_ASSET, "size": zip.len(), "browser_download_url": release::ZIP_ASSET},
                {"name": release::SHA_ASSET, "size": 80, "browser_download_url": release::SHA_ASSET},
            ],
        }]);
        std::fs::write(
            dir.join("releases.json"),
            serde_json::to_vec(&list).unwrap(),
        )
        .unwrap();
        let json = Feed::Local(dir.to_owned())
            .releases(&AtomicBool::new(false))
            .unwrap();
        release::newest(&json, Version::parse("0.0.1").unwrap())
            .unwrap()
            .unwrap()
    }

    #[test]
    fn a_verified_package_is_staged_and_a_tampered_one_is_not() {
        let dir = zip::temp_dir("stage");
        let feed = Feed::Local(dir.clone());
        let root = dir.join("updates");
        let cancel = AtomicBool::new(false);
        let files: [(&str, &[u8]); 3] = [
            (PROGRAM_EXE, b"new program"),
            (UPDATER_EXE, b"updater"),
            ("assets/360.HDR/a.hdr", b"sky"),
        ];
        let release = local_feed(&dir, "v9.9.9", &files, None);
        let seen = std::cell::Cell::new(0);
        let staged = stage(&feed, &release, &root, &|done, _| seen.set(done), &cancel).unwrap();
        assert_eq!(seen.get(), release.zip_size);
        assert_eq!(
            std::fs::read(staged.payload.join(PROGRAM_EXE)).unwrap(),
            b"new program"
        );
        assert!(staged.updater().is_file());
        // Wrong checksum: refused, nothing left behind.
        let release = local_feed(&dir, "v9.9.9", &files, Some(&"0".repeat(64)));
        let error = stage(&feed, &release, &root, &|_, _| (), &cancel).unwrap_err();
        assert!(error.contains("SHA-256 mismatch"), "{error}");
        // No updater in the package: refused and removed.
        let release = local_feed(&dir, "v9.9.9", &files[..1], None);
        assert!(
            stage(&feed, &release, &root, &|_, _| (), &cancel)
                .unwrap_err()
                .contains(UPDATER_EXE)
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        // Pruning keeps the newest backup only and drops staging folders.
        std::fs::create_dir_all(root.join("backup-1")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::create_dir_all(root.join("backup-2")).unwrap();
        std::fs::write(root.join("updater.log"), "log").unwrap();
        prune(&root, 1);
        let mut left: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["backup-2", "updater.log"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
