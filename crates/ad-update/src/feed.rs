//! Where releases come from: GitHub, or a local folder for tests and smokes
//! (`releases.json` whose asset URLs are file names in that folder).
use crate::{Result, release};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// The release list is small; a larger answer is not GitHub's.
const LIST_LIMIT: u64 = 8 << 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Feed {
    GitHub,
    Local(PathBuf),
}

impl Feed {
    /// The release list JSON.
    pub fn releases(&self, cancel: &AtomicBool) -> Result<Vec<u8>> {
        match self {
            Feed::GitHub => {
                let mut out = Vec::new();
                fetch(
                    &format!(
                        "https://api.github.com/repos/{}/releases?per_page=20",
                        release::REPOSITORY
                    ),
                    "application/vnd.github+json",
                    LIST_LIMIT,
                    &mut out,
                    &|_| (),
                    cancel,
                )?;
                Ok(out)
            }
            Feed::Local(folder) => {
                let path = folder.join("releases.json");
                std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
            }
        }
    }
    /// Copies one asset into `out`, at most `limit` bytes.
    pub fn download(
        &self,
        url: &str,
        limit: u64,
        out: &mut dyn Write,
        progress: &dyn Fn(u64),
        cancel: &AtomicBool,
    ) -> Result<u64> {
        match self {
            Feed::GitHub => fetch(
                url,
                "application/octet-stream",
                limit,
                out,
                progress,
                cancel,
            ),
            Feed::Local(folder) => {
                let name = Path::new(url)
                    .file_name()
                    .ok_or_else(|| format!("bad local asset {url}"))?;
                let path = folder.join(name);
                let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                if bytes.len() as u64 > limit {
                    return Err("download larger than expected".into());
                }
                out.write_all(&bytes).map_err(|e| e.to_string())?;
                progress(bytes.len() as u64);
                Ok(bytes.len() as u64)
            }
        }
    }
}

#[cfg(windows)]
fn fetch(
    url: &str,
    accept: &str,
    limit: u64,
    out: &mut dyn Write,
    progress: &dyn Fn(u64),
    cancel: &AtomicBool,
) -> Result<u64> {
    crate::http::get(url, accept, limit, out, progress, cancel)
}

#[cfg(not(windows))]
fn fetch(
    _: &str,
    _: &str,
    _: u64,
    _: &mut dyn Write,
    _: &dyn Fn(u64),
    _: &AtomicBool,
) -> Result<u64> {
    Err("updates need Windows".into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    /// Real request to the public repository (run with --ignored; network).
    #[test]
    #[ignore]
    fn github_release_list_answers() {
        let json = Feed::GitHub.releases(&AtomicBool::new(false)).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert!(value.is_array(), "{}", String::from_utf8_lossy(&json));
        let newest = release::newest(&json, crate::Version::parse("0.0.1").unwrap()).unwrap();
        eprintln!(
            "releases={} newest={:?}",
            value.as_array().unwrap().len(),
            newest.map(|r| r.tag)
        );
    }
    #[test]
    fn only_github_https_urls_are_requested() {
        let cancel = AtomicBool::new(false);
        for url in [
            "http://github.com/x",
            "https://example.com/x",
            "file:///C:/x",
            "https://github.com.evil.test/x",
        ] {
            let error = Feed::GitHub
                .download(url, 10, &mut Vec::new(), &|_| (), &cancel)
                .unwrap_err();
            assert!(
                error.contains("outside GitHub") || error.contains("bad update URL"),
                "{url}: {error}"
            );
        }
    }
}
