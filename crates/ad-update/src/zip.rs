//! A small, strict zip reader for the release package: stored and deflate
//! entries only, no zip64, no encryption. Every name must be a plain relative
//! path; the CRC and the declared sizes are checked.
use crate::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub entries: usize,
    pub entry_bytes: u64,
    pub total_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            entries: 4096,
            entry_bytes: 512 << 20,
            total_bytes: 2 << 30,
        }
    }
}

#[derive(Debug)]
struct Entry {
    name: String,
    method: u16,
    crc: u32,
    compressed: u64,
    size: u64,
    local_offset: u64,
}

fn u16_at(b: &[u8], at: usize) -> Result<u16> {
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| "zip: truncated".to_string())
}
fn u32_at(b: &[u8], at: usize) -> Result<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| "zip: truncated".to_string())
}

/// Path parts of an entry name, refusing anything that could leave the
/// destination or that Windows would read differently.
fn parts(name: &str) -> Result<Vec<&str>> {
    let bad = |why: &str| Err(format!("zip: unsafe entry name {name:?} ({why})"));
    if name.is_empty() || name.starts_with(['/', '\\']) {
        return bad("absolute");
    }
    let parts: Vec<&str> = name
        .trim_end_matches(['/', '\\'])
        .split(['/', '\\'])
        .collect();
    for part in &parts {
        if part.is_empty() || *part == "." || *part == ".." {
            return bad("empty, . or ..");
        }
        if part.ends_with(['.', ' ']) {
            return bad("trailing dot or space");
        }
        if part
            .chars()
            .any(|c| c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        {
            return bad("reserved character");
        }
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.len() == 4
                && stem.as_bytes()[3].is_ascii_digit());
        if reserved {
            return bad("reserved device name");
        }
    }
    Ok(parts)
}

fn central_directory(bytes: &[u8], limits: Limits) -> Result<Vec<Entry>> {
    const END: u32 = 0x0605_4b50;
    const CENTRAL: u32 = 0x0201_4b50;
    if bytes.len() < 22 {
        return Err("zip: too short".into());
    }
    let lowest = bytes.len().saturating_sub(22 + 65535);
    let end = (lowest..=bytes.len() - 22)
        .rev()
        .find(|&i| u32_at(bytes, i).ok() == Some(END))
        .ok_or("zip: no end of central directory")?;
    let disk = u16_at(bytes, end + 4)?;
    let cd_disk = u16_at(bytes, end + 6)?;
    let here = u16_at(bytes, end + 8)?;
    let count = u16_at(bytes, end + 10)?;
    let cd_size = u32_at(bytes, end + 12)?;
    let cd_offset = u32_at(bytes, end + 16)?;
    if disk != 0 || cd_disk != 0 || here != count {
        return Err("zip: multi-part archives are not supported".into());
    }
    if count == u16::MAX || cd_size == u32::MAX || cd_offset == u32::MAX {
        return Err("zip: zip64 is not supported".into());
    }
    if usize::from(count) > limits.entries {
        return Err(format!("zip: more than {} entries", limits.entries));
    }
    let (cd_offset, cd_end) = (cd_offset as usize, cd_offset as usize + cd_size as usize);
    if cd_end > end {
        return Err("zip: central directory out of range".into());
    }
    let mut entries = Vec::with_capacity(count.into());
    let mut at = cd_offset;
    for _ in 0..count {
        if u32_at(bytes, at)? != CENTRAL {
            return Err("zip: bad central directory entry".into());
        }
        let flags = u16_at(bytes, at + 8)?;
        let method = u16_at(bytes, at + 10)?;
        let crc = u32_at(bytes, at + 16)?;
        let compressed = u32_at(bytes, at + 20)?;
        let size = u32_at(bytes, at + 24)?;
        let name_len = usize::from(u16_at(bytes, at + 28)?);
        let extra_len = usize::from(u16_at(bytes, at + 30)?);
        let comment_len = usize::from(u16_at(bytes, at + 32)?);
        let local_offset = u32_at(bytes, at + 42)?;
        if flags & 1 != 0 || flags & 0x40 != 0 {
            return Err("zip: encrypted entries are not supported".into());
        }
        if compressed == u32::MAX || size == u32::MAX || local_offset == u32::MAX {
            return Err("zip: zip64 is not supported".into());
        }
        let name = bytes
            .get(at + 46..at + 46 + name_len)
            .ok_or("zip: truncated name")?;
        let name = std::str::from_utf8(name)
            .map_err(|_| "zip: entry name is not UTF-8")?
            .to_owned();
        at += 46 + name_len + extra_len + comment_len;
        if at > cd_end {
            return Err("zip: central directory overrun".into());
        }
        entries.push(Entry {
            name,
            method,
            crc,
            compressed: compressed.into(),
            size: size.into(),
            local_offset: local_offset.into(),
        });
    }
    Ok(entries)
}

/// Data of one entry, checked against its size and CRC.
fn entry_data(bytes: &[u8], entry: &Entry, limits: Limits) -> Result<Vec<u8>> {
    const LOCAL: u32 = 0x0403_4b50;
    if entry.size > limits.entry_bytes {
        return Err(format!("zip: {} is too large", entry.name));
    }
    let at = entry.local_offset as usize;
    if u32_at(bytes, at)? != LOCAL {
        return Err(format!("zip: bad local header for {}", entry.name));
    }
    let start =
        at + 30 + usize::from(u16_at(bytes, at + 26)?) + usize::from(u16_at(bytes, at + 28)?);
    let data = bytes
        .get(start..start + entry.compressed as usize)
        .ok_or_else(|| format!("zip: {} is truncated", entry.name))?;
    let out = match entry.method {
        0 => data.to_vec(),
        8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, entry.size as usize)
            .map_err(|e| format!("zip: {} does not inflate: {e:?}", entry.name))?,
        other => return Err(format!("zip: {} uses method {other}", entry.name)),
    };
    if out.len() as u64 != entry.size {
        return Err(format!("zip: {} has the wrong size", entry.name));
    }
    if crc32fast::hash(&out) != entry.crc {
        return Err(format!("zip: {} fails its CRC", entry.name));
    }
    Ok(out)
}

/// Unpacks `bytes` into the empty or new folder `destination`. Returns the
/// written files relative to it.
pub fn extract(bytes: &[u8], destination: &Path, limits: Limits) -> Result<Vec<PathBuf>> {
    let entries = central_directory(bytes, limits)?;
    let total: u64 = entries.iter().map(|e| e.size).sum();
    if total > limits.total_bytes {
        return Err("zip: the package unpacks too large".into());
    }
    // Check every name before writing anything.
    let mut seen = HashSet::new();
    let mut plan = Vec::with_capacity(entries.len());
    for entry in &entries {
        let directory = entry.name.ends_with(['/', '\\']);
        let parts = parts(&entry.name)?;
        let key = parts.join("/").to_lowercase();
        if !seen.insert(key) && !directory {
            return Err(format!("zip: duplicate entry {}", entry.name));
        }
        let relative: PathBuf = parts.iter().collect();
        plan.push((entry, relative, directory));
    }
    std::fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    let mut written = Vec::new();
    for (entry, relative, directory) in plan {
        let path = destination.join(&relative);
        if directory {
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        let data = entry_data(bytes, entry, limits)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, &data))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        written.push(relative);
    }
    Ok(written)
}

/// Test helper: a zip of (name, data) pairs, deflated when `deflate`.
#[cfg(test)]
pub(crate) fn build(files: &[(&str, &[u8])], deflate: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let packed = if deflate {
            miniz_oxide::deflate::compress_to_vec(data, 6)
        } else {
            data.to_vec()
        };
        let method: u16 = if deflate { 8 } else { 0 };
        let crc = crc32fast::hash(data);
        let offset = out.len() as u32;
        let head = |sig: u32, central: bool| {
            let mut h = Vec::new();
            h.extend(sig.to_le_bytes());
            if central {
                h.extend(20u16.to_le_bytes());
            }
            h.extend(20u16.to_le_bytes());
            h.extend(0x0800u16.to_le_bytes());
            h.extend(method.to_le_bytes());
            h.extend([0u8; 4]);
            h.extend(crc.to_le_bytes());
            h.extend((packed.len() as u32).to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes());
            if central {
                // Comment length, disk, internal and external attributes.
                h.extend([0u8; 10]);
                h.extend(offset.to_le_bytes());
            }
            h.extend(name.as_bytes());
            h
        };
        out.extend(head(0x0403_4b50, false));
        out.extend(&packed);
        central.extend(head(0x0201_4b50, true));
    }
    let cd_offset = out.len() as u32;
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(cd_offset.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

#[cfg(test)]
pub(crate) fn temp_dir(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/update-tests")
        .join(format!("{tag}-{}", crate::unique_suffix()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_and_deflated_entries_unpack_into_folders() {
        for deflate in [false, true] {
            let dir = temp_dir("zip-ok");
            let big = vec![42u8; 300_000];
            let zip = build(
                &[
                    ("AssetDoctorRTPT.exe", b"program"),
                    ("assets/360.HDR/sky.hdr", &big),
                    ("assets\\notes.txt", "Привет".as_bytes()),
                ],
                deflate,
            );
            let files = extract(&zip, &dir, Limits::default()).unwrap();
            assert_eq!(files.len(), 3);
            assert_eq!(
                std::fs::read(dir.join("AssetDoctorRTPT.exe")).unwrap(),
                b"program"
            );
            assert_eq!(
                std::fs::read(dir.join("assets/360.HDR/sky.hdr")).unwrap(),
                big
            );
            assert_eq!(
                std::fs::read_to_string(dir.join("assets/notes.txt")).unwrap(),
                "Привет"
            );
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }

    #[test]
    fn a_package_made_by_powershell_unpacks() {
        // Compress-Archive (PowerShell, .NET ZipFile, deflate).
        let zip = include_bytes!("../../../tests/fixtures/update/compress-archive.zip");
        let dir = temp_dir("zip-ps");
        let mut files = extract(zip, &dir, Limits::default()).unwrap();
        files.sort();
        assert_eq!(
            files,
            [
                PathBuf::from("assets/360.HDR/CREDITS.txt"),
                PathBuf::from("readme.txt")
            ]
        );
        assert!(
            std::fs::read_to_string(dir.join("readme.txt"))
                .unwrap()
                .starts_with("Asset Doctor")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unsafe_or_broken_packages_are_refused_before_writing() {
        let refuse = |zip: Vec<u8>, why: &str| {
            let dir = temp_dir("zip-bad");
            let error = extract(&zip, &dir, Limits::default()).unwrap_err();
            assert!(error.contains(why), "{error}");
            assert_eq!(
                std::fs::read_dir(&dir).unwrap().count(),
                0,
                "{why}: wrote files"
            );
            std::fs::remove_dir_all(&dir).unwrap();
        };
        for name in [
            "../evil.exe",
            "a/../../b",
            "/root.txt",
            "\\root.txt",
            "C:/x",
            "a/./b",
            "dot./x",
            "CON.txt",
            "a/lpt1",
            "x|y",
            "a//b",
        ] {
            refuse(
                build(&[("ok.txt", b"1"), (name, b"x")], false),
                "unsafe entry name",
            );
        }
        refuse(
            build(&[("a.txt", b"1"), ("A.TXT", b"2")], false),
            "duplicate",
        );
        let mut crc = build(&[("a.txt", b"hello")], false);
        let data_at = 30 + 5;
        crc[data_at] ^= 1;
        let dir = temp_dir("zip-crc");
        assert!(
            extract(&crc, &dir, Limits::default())
                .unwrap_err()
                .contains("CRC")
        );
        std::fs::remove_dir_all(&dir).unwrap();
        refuse(
            b"PK not a zip at all, just some text".to_vec(),
            "no end of central directory",
        );
        let tight = Limits {
            entries: 1,
            ..Limits::default()
        };
        let dir = temp_dir("zip-limit");
        assert!(
            extract(&build(&[("a", b"1"), ("b", b"2")], false), &dir, tight)
                .unwrap_err()
                .contains("more than 1")
        );
        let small = Limits {
            total_bytes: 3,
            ..Limits::default()
        };
        assert!(
            extract(&build(&[("a", b"1234")], true), &dir, small)
                .unwrap_err()
                .contains("too large")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
