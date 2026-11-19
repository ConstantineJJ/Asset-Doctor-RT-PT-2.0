//! ADR-0039: repair report history storage.
use super::*;
use std::{fs, path::PathBuf};

fn dir() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/history-tests");
    fs::create_dir_all(&root).unwrap();
    for n in 0..64 {
        let path = root.join(format!(
            "h-{}-{}-{n}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        if fs::create_dir(&path).is_ok() {
            return path;
        }
    }
    panic!("no test directory")
}
fn record(n: u64) -> Record {
    Record {
        time: 1_763_530_000 + n,
        session: 7,
        model: "Vertex_Weld.glb".into(),
        source_bytes: 1176,
        action: "apply".into(),
        rule: "G012".into(),
        mesh: Some(0),
        triangles: (3, 3),
        vertices: (9, 7),
        normals: 0,
        left: Some(0),
        complete: true,
        revision: n,
    }
}

#[test]
fn records_round_trip_and_corrupt_lines_are_skipped_and_counted() {
    let d = dir();
    let history = History::at(d.join("h.jsonl"), 7);
    history.append(&record(1)).unwrap();
    history.append(&record(2)).unwrap();
    let mut text = fs::read_to_string(d.join("h.jsonl")).unwrap();
    text.push_str("{not json}\n{\"time\":\"x\"}\n");
    fs::write(d.join("h.jsonl"), text).unwrap();
    history.append(&record(3)).unwrap();
    let (records, corrupt) = history.load().unwrap();
    assert_eq!(corrupt, 2);
    assert_eq!(
        records.iter().map(|r| r.revision).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(records[0], record(1));
}

#[test]
fn rotation_keeps_the_newest_records() {
    let d = dir();
    let history = History::at(d.join("h.jsonl"), 7);
    for n in 0..(MAX_LINES as u64 + 5) {
        history.append(&record(n)).unwrap();
    }
    let (records, _) = history.load().unwrap();
    assert!(records.len() <= MAX_LINES);
    assert_eq!(records.last().unwrap().revision, MAX_LINES as u64 + 4);
    assert!(records.len() >= KEEP_LINES);
}

#[test]
fn unwritable_storage_is_an_error_and_clear_empties_history() {
    let d = dir();
    fs::write(d.join("file"), b"x").unwrap();
    let blocked = History::at(d.join("file").join("h.jsonl"), 7);
    assert!(blocked.append(&record(1)).is_err());
    let history = History::at(d.join("h.jsonl"), 7);
    history.append(&record(1)).unwrap();
    history.clear().unwrap();
    assert!(history.load().unwrap().0.is_empty());
    // A missing file is simply an empty history.
    assert!(
        History::at(d.join("none.jsonl"), 7)
            .load()
            .unwrap()
            .0
            .is_empty()
    );
}

#[test]
fn utc_time_is_formatted_without_a_calendar_dependency() {
    assert_eq!(format_utc(0), "1970-01-01 00:00 UTC");
    assert_eq!(format_utc(1_763_530_620), "2025-11-19 05:37 UTC");
    assert_eq!(format_utc(951_782_400), "2000-02-29 00:00 UTC");
}
