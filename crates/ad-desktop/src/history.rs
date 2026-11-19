//! Repair report history (HEAL-03, ADR-0039): one JSON object per line in a
//! program-owned file. History is never model state; failures are reported to
//! the caller and do not undo edits.
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

/// Rotation: past this many lines only the newest KEEP_LINES are kept.
pub const MAX_LINES: usize = 1000;
pub const KEEP_LINES: usize = 500;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// Seconds since the Unix epoch, UTC.
    pub time: u64,
    /// Identifies the program run that wrote the record.
    pub session: u64,
    pub model: String,
    pub source_bytes: u64,
    /// apply, undo, redo, queue, save, save-copy.
    pub action: String,
    /// G001… label, or "-" for actions without a rule.
    pub rule: String,
    pub mesh: Option<usize>,
    pub triangles: (usize, usize),
    pub vertices: (usize, usize),
    pub normals: usize,
    /// Findings of the repaired rule on the primitive after the step.
    pub left: Option<usize>,
    pub complete: bool,
    pub revision: u64,
}
impl Record {
    fn to_json(&self) -> Value {
        json!({
            "time": self.time, "session": self.session, "model": self.model,
            "source_bytes": self.source_bytes, "action": self.action, "rule": self.rule,
            "mesh": self.mesh, "triangles": [self.triangles.0, self.triangles.1],
            "vertices": [self.vertices.0, self.vertices.1], "normals": self.normals,
            "left": self.left, "complete": self.complete, "revision": self.revision,
        })
    }
    /// Strict: every field present with its type, otherwise None.
    fn from_json(v: &Value) -> Option<Self> {
        let u = |k: &str| v.get(k)?.as_u64();
        let s = |k: &str| Some(v.get(k)?.as_str()?.to_owned());
        let pair = |k: &str| {
            let a = v.get(k)?.as_array()?;
            (a.len() == 2).then_some(())?;
            Some((a[0].as_u64()? as usize, a[1].as_u64()? as usize))
        };
        let optional = |k: &str| match v.get(k)? {
            Value::Null => Some(None),
            other => other.as_u64().map(|n| Some(n as usize)),
        };
        Some(Self {
            time: u("time")?,
            session: u("session")?,
            model: s("model")?,
            source_bytes: u("source_bytes")?,
            action: s("action")?,
            rule: s("rule")?,
            mesh: optional("mesh")?,
            triangles: pair("triangles")?,
            vertices: pair("vertices")?,
            normals: u("normals")? as usize,
            left: optional("left")?,
            complete: v.get("complete")?.as_bool()?,
            revision: u("revision")?,
        })
    }
}

pub struct History {
    path: PathBuf,
    pub session: u64,
}
impl History {
    pub fn at(path: PathBuf, session: u64) -> Self {
        Self { path, session }
    }
    /// %LOCALAPPDATA%\AssetDoctorRTPT\repair-history.jsonl.
    pub fn default_path() -> Option<PathBuf> {
        std::env::var_os("LOCALAPPDATA").map(|base| {
            PathBuf::from(base)
                .join("AssetDoctorRTPT")
                .join("repair-history.jsonl")
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn append(&self, record: &Record) -> Result<(), String> {
        let fail = |e: std::io::Error| {
            format!("история ремонта не записана ({}): {e}", self.path.display())
        };
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(fail)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(fail)?;
        let mut line = record.to_json().to_string();
        line.push('\n');
        file.write_all(line.as_bytes()).map_err(fail)?;
        drop(file);
        self.rotate().map_err(fail)
    }
    fn rotate(&self) -> std::io::Result<()> {
        let text = fs::read_to_string(&self.path)?;
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() <= MAX_LINES {
            return Ok(());
        }
        let mut kept = lines[lines.len() - KEEP_LINES..].join("\n");
        kept.push('\n');
        let staging = self.path.with_extension("jsonl.tmp");
        fs::write(&staging, kept)?;
        fs::rename(&staging, &self.path)
    }
    /// All valid records, oldest first, and the number of corrupt lines.
    pub fn load(&self) -> Result<(Vec<Record>, usize), String> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
            Err(e) => return Err(format!("история ремонта не прочитана: {e}")),
        };
        let mut records = Vec::new();
        let mut corrupt = 0;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            match serde_json::from_str::<Value>(line)
                .ok()
                .as_ref()
                .and_then(Record::from_json)
            {
                Some(r) => records.push(r),
                None => corrupt += 1,
            }
        }
        Ok((records, corrupt))
    }
    pub fn clear(&self) -> Result<(), String> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("история ремонта не очищена: {e}")),
        }
    }
}

/// Current UTC time in seconds.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
/// "YYYY-MM-DD HH:MM UTC" (civil-from-days, no calendar dependency).
pub fn format_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
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
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rem / 3600,
        rem % 3600 / 60
    )
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod history_tests;
