//! Program log: every diagnostic line the program prints (the crate's
//! `eprintln!` also lands here) and every status message, kept in memory for
//! the log window and, when the owner turns it on, appended to
//! %LOCALAPPDATA%\AssetDoctorRTPT\logs\asset-doctor-<UTC date and time>.log.
use std::{
    collections::VecDeque,
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Instant,
};

/// Lines kept in memory.
pub const CAPACITY: usize = 5000;

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Seconds since the program started.
    pub at: f32,
    pub text: String,
    pub warning: bool,
}

struct Log {
    started: Instant,
    lines: VecDeque<Line>,
    /// Lines written since the start (also those dropped from memory).
    total: u64,
    warnings: u64,
    file: Option<(PathBuf, std::fs::File)>,
}
fn log() -> &'static Mutex<Log> {
    static LOG: OnceLock<Mutex<Log>> = OnceLock::new();
    LOG.get_or_init(|| {
        Mutex::new(Log {
            started: Instant::now(),
            lines: VecDeque::new(),
            total: 0,
            warnings: 0,
            file: None,
        })
    })
}

/// A refusal, failure or error by its words.
fn is_warning(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "error",
        "failed",
        "ok=false",
        "ошибк",
        "не удалось",
        "не загружен",
        "отказ",
        "не сохранен",
        "не подготовлен",
    ]
    .iter()
    .any(|w| lower.contains(w))
}

pub fn push(text: impl Into<String>) {
    let text = text.into();
    let Ok(mut log) = log().lock() else {
        return;
    };
    let at = log.started.elapsed().as_secs_f32();
    let warning = is_warning(&text);
    if let Some((_, file)) = log.file.as_mut() {
        let _ = writeln!(file, "{at:10.3} {text}");
    }
    if log.lines.len() == CAPACITY {
        log.lines.pop_front();
    }
    log.total += 1;
    log.warnings += u64::from(warning);
    log.lines.push_back(Line { at, text, warning });
}
/// Lines in memory, oldest first, with the totals since start.
pub fn snapshot() -> (Vec<Line>, u64, u64) {
    log().lock().map_or((Vec::new(), 0, 0), |log| {
        (log.lines.iter().cloned().collect(), log.total, log.warnings)
    })
}
pub fn counts() -> (u64, u64) {
    log().lock().map_or((0, 0), |log| (log.total, log.warnings))
}
pub fn clear() {
    if let Ok(mut log) = log().lock() {
        log.lines.clear();
    }
}
/// %LOCALAPPDATA%\AssetDoctorRTPT\logs.
pub fn default_folder() -> Option<PathBuf> {
    crate::history::History::default_path().and_then(|p| p.parent().map(|d| d.join("logs")))
}
/// UTC «YYYY-MM-DD_HH-MM-SS» of a Unix time.
fn stamp(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}_{:02}-{:02}-{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}
/// Starts (Some folder) or stops (None) writing to a new file in `folder`;
/// the lines already in memory are written first. Returns the file path.
pub fn write_to(folder: Option<PathBuf>) -> std::io::Result<Option<PathBuf>> {
    let Ok(mut log) = log().lock() else {
        return Ok(None);
    };
    log.file = None;
    let Some(folder) = folder else {
        return Ok(None);
    };
    std::fs::create_dir_all(&folder)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let path = folder.join(format!("asset-doctor-{}.log", stamp(now)));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(
        file,
        "Asset Doctor RT-PT {} log, UTC {}",
        env!("CARGO_PKG_VERSION"),
        stamp(now)
    )?;
    for line in &log.lines {
        writeln!(file, "{:10.3} {}", line.at, line.text)?;
    }
    log.file = Some((path.clone(), file));
    Ok(Some(path))
}
pub fn file() -> Option<PathBuf> {
    log()
        .lock()
        .ok()
        .and_then(|log| log.file.as_ref().map(|f| f.0.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lines_are_kept_marked_and_written_to_a_file() {
        assert_eq!(stamp(0), "1970-01-01_00-00-00");
        assert_eq!(stamp(1_795_219_200 + 3_725), "2026-11-21_01-02-05");
        push("view_settings_saved ok=true");
        push("edit_failed code=Unsupported");
        let (lines, total, warnings) = snapshot();
        assert!(total >= 2 && warnings >= 1);
        let last = &lines[lines.len() - 1];
        assert!(last.warning && last.text.contains("edit_failed"));
        assert!(!lines[lines.len() - 2].warning);
        let dir = std::env::temp_dir().join(format!("ad-log-{}", std::process::id()));
        let path = write_to(Some(dir.clone())).unwrap().unwrap();
        push("after the file opened");
        assert_eq!(file(), Some(path.clone()));
        write_to(None).unwrap();
        assert_eq!(file(), None);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("edit_failed") && text.contains("after the file opened"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
