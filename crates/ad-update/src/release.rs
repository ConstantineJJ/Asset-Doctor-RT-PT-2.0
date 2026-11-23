//! Choosing the release to install from the GitHub release list.
use crate::{Result, Version};

/// The repository whose releases update this program.
pub const REPOSITORY: &str = "ConstantineJJ/Asset-Doctor-RT-PT-2.0";
/// Release assets: the program folder as a zip and its SHA-256.
pub const ZIP_ASSET: &str = "AssetDoctorRTPT-win-x64.zip";
pub const SHA_ASSET: &str = "AssetDoctorRTPT-win-x64.sha256";
/// Release notes longer than this are cut in the update window.
const NOTES_LIMIT: usize = 8000;

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub tag: String,
    pub version: Version,
    pub zip_url: String,
    pub zip_size: u64,
    pub sha_url: String,
    pub page_url: String,
    pub name: String,
    pub notes: String,
}

/// The newest stable release above `current` that carries both assets.
pub fn newest(json: &[u8], current: Version) -> Result<Option<Release>> {
    let list: serde_json::Value =
        serde_json::from_slice(json).map_err(|e| format!("release list is not JSON: {e}"))?;
    let list = list.as_array().ok_or("release list is not an array")?;
    let mut best: Option<Release> = None;
    for release in list {
        let flag = |key| release.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
        if flag("draft") || flag("prerelease") {
            continue;
        }
        let text = |key| release.get(key).and_then(|v| v.as_str()).unwrap_or("");
        let tag = text("tag_name");
        let Some(version) = Version::parse(tag) else {
            continue;
        };
        if version <= current || best.as_ref().is_some_and(|b| b.version >= version) {
            continue;
        }
        let mut zip = None;
        let mut sha = None;
        for asset in release
            .get("assets")
            .and_then(|a| a.as_array())
            .into_iter()
            .flatten()
        {
            let name = asset.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let url = asset
                .get("browser_download_url")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let size = asset.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            if url.is_empty() {
                continue;
            }
            if name.eq_ignore_ascii_case(ZIP_ASSET) {
                zip = Some((url.to_owned(), size));
            } else if name.eq_ignore_ascii_case(SHA_ASSET) {
                sha = Some(url.to_owned());
            }
        }
        let (Some((zip_url, zip_size)), Some(sha_url)) = (zip, sha) else {
            continue;
        };
        let mut notes = text("body").replace("\r\n", "\n");
        if notes.len() > NOTES_LIMIT {
            let cut = (0..=NOTES_LIMIT)
                .rev()
                .find(|&i| notes.is_char_boundary(i))
                .unwrap_or(0);
            notes.truncate(cut);
            notes.push_str("\n…");
        }
        best = Some(Release {
            tag: tag.to_owned(),
            version,
            zip_url,
            zip_size,
            sha_url,
            page_url: text("html_url").to_owned(),
            name: text("name").to_owned(),
            notes,
        });
    }
    Ok(best)
}

/// The expected SHA-256 (lower-case hex) from a `.sha256` file:
/// `<hex>` or `<hex>  <file name>`.
pub fn parse_sha256_file(text: &str) -> Result<String> {
    let hash = text
        .split_whitespace()
        .next()
        .ok_or("the checksum file is empty")?
        .to_ascii_lowercase();
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("the checksum file holds no SHA-256".into());
    }
    Ok(hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(tag: &str, draft: bool, pre: bool, assets: &[&str]) -> serde_json::Value {
        serde_json::json!({
            "tag_name": tag,
            "name": format!("AD RT-PT {tag}"),
            "draft": draft,
            "prerelease": pre,
            "html_url": format!("https://github.com/{REPOSITORY}/releases/tag/{tag}"),
            "body": "Что нового",
            "assets": assets.iter().map(|name| serde_json::json!({
                "name": name,
                "size": 1234,
                "browser_download_url": format!("https://github.com/{REPOSITORY}/releases/download/{tag}/{name}"),
            })).collect::<Vec<_>>(),
        })
    }
    fn pick(list: Vec<serde_json::Value>, current: &str) -> Option<Release> {
        let json = serde_json::to_vec(&list).unwrap();
        newest(&json, Version::parse(current).unwrap()).unwrap()
    }

    #[test]
    fn the_newest_complete_stable_release_above_the_current_one_wins() {
        let both = [ZIP_ASSET, SHA_ASSET];
        let list = vec![
            release("v2.6.0", true, false, &both),         // draft
            release("v2.5.9", false, true, &both),         // pre-release
            release("v2.5.8", false, false, &[ZIP_ASSET]), // no checksum
            release("v2.5.4", false, false, &both),
            release("v2.5.7", false, false, &both),
            release("nightly", false, false, &both),
            release("v2.5.2", false, false, &both),
        ];
        let found = pick(list.clone(), "2.5.3").unwrap();
        assert_eq!(found.tag, "v2.5.7");
        assert_eq!(found.zip_size, 1234);
        assert!(found.zip_url.ends_with(ZIP_ASSET));
        assert!(found.sha_url.ends_with(SHA_ASSET));
        assert_eq!(found.notes, "Что нового");
        assert!(pick(list.clone(), "2.5.7").is_none());
        assert!(pick(list, "9.0.0").is_none());
        assert!(pick(Vec::new(), "2.5.3").is_none());
        assert!(newest(b"{\"message\":\"rate limit\"}", Version::current()).is_err());
        assert!(newest(b"<html>", Version::current()).is_err());
    }

    #[test]
    fn checksum_files_are_read_strictly() {
        let hex = "a".repeat(64);
        assert_eq!(parse_sha256_file(&hex).unwrap(), hex);
        assert_eq!(
            parse_sha256_file(&format!("{}  {ZIP_ASSET}\r\n", "AB".repeat(32))).unwrap(),
            "ab".repeat(32)
        );
        assert!(parse_sha256_file("").is_err());
        assert!(parse_sha256_file(&"g".repeat(64)).is_err());
        assert!(parse_sha256_file(&"a".repeat(63)).is_err());
    }
}
