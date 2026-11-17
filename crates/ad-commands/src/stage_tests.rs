//! ADR-0031 stage B2: every Session check runs before publication, and a
//! dropped stage leaves document, revision, history, cursor and redo intact.
use super::*;
use ad_document::{Cancellation, DocumentId, ErrorCode, GeometrySnapshot, ImportLimits, MeshId};
use std::sync::Arc;

fn session() -> Session {
    let document = ad_formats::import_glb(
        Arc::from(include_bytes!("../../../tests/fixtures/Geometry_Findings.glb").as_slice()),
        "stages".into(),
        DocumentId(1),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    Session::new(document, HistoryLimits::default(), ImportLimits::default()).unwrap()
}

/// Everything observable about a Session that a failed stage must preserve.
#[derive(Debug, PartialEq)]
struct Observed {
    tag: DocumentTag,
    usage: HistoryUsage,
    modified: bool,
    indices: Vec<Vec<u32>>,
}
fn observe(s: &Session) -> Observed {
    Observed {
        tag: s.document().tag(),
        usage: s.history_usage(),
        modified: s.is_modified(),
        indices: s
            .document()
            .primitives()
            .iter()
            .map(|m| m.indices.clone())
            .collect(),
    }
}
fn same_snapshot(a: &GeometrySnapshot, b: &GeometrySnapshot) {
    assert_eq!((a.document_id, a.revision), (b.document_id, b.revision));
    assert_eq!(a.positions, b.positions);
    assert_eq!(a.indices, b.indices);
    assert_eq!(a.draws.len(), b.draws.len());
    for (x, y) in a.draws.iter().zip(&b.draws) {
        assert_eq!(
            (x.index_start, x.index_count),
            (y.index_start, y.index_count)
        );
    }
}

#[test]
fn dropped_commit_stage_after_undo_keeps_redo_and_published_stage_drops_it() {
    let mut s = session();
    let c = Cancellation::default();
    let p = s
        .prepare_removal(s.document().tag(), MeshId(0), &[1], &c)
        .unwrap();
    s.commit(p, &c).unwrap();
    s.undo(s.document().tag(), &c).unwrap();
    let before = observe(&s);
    assert!(s.can_redo());

    let p = s
        .prepare_removal(s.document().tag(), MeshId(0), &[0], &c)
        .unwrap();
    let expected = p.candidate(s.document(), &c).unwrap().tag();
    let staged = s.stage_commit(p, &c).unwrap();
    assert_eq!(staged.tag_after(), expected);
    drop(staged); // e.g. GPU candidate failed after the Session checks
    assert_eq!(observe(&s), before);
    assert!(s.can_redo());

    let p = s
        .prepare_removal(s.document().tag(), MeshId(0), &[0], &c)
        .unwrap();
    let tag = s.stage_commit(p, &c).unwrap().publish();
    assert_eq!(tag, expected);
    assert_eq!(s.document().tag(), expected);
    assert!(!s.can_redo());
    assert_eq!(s.history_usage().undo, 1);
    assert!(s.is_modified());
}

#[test]
fn undo_and_redo_targets_predict_published_snapshot_and_report_exactly() {
    let mut s = session();
    let c = Cancellation::default();
    let limits = ImportLimits::default();
    assert!(matches!(s.undo_candidate(), Err(e) if e.code == ErrorCode::HistoryEmpty));
    assert!(matches!(s.redo_candidate(), Err(e) if e.code == ErrorCode::HistoryEmpty));
    let untouched = s.document().primitives()[1].indices.clone();
    let p = s
        .prepare_removal(s.document().tag(), MeshId(0), &[1], &c)
        .unwrap();
    s.commit(p, &c).unwrap();

    for step in 0..2 {
        let (snapshot, report, target) = {
            let candidate = if step == 0 {
                s.undo_candidate().unwrap()
            } else {
                s.redo_candidate().unwrap()
            };
            assert_eq!(candidate.mesh(), MeshId(0));
            let target = candidate.tag();
            let view = s.document().candidate_view(candidate, &c).unwrap();
            assert_eq!(view.indices(MeshId(1)), untouched.as_slice());
            (
                view.snapshot(limits, &c).unwrap(),
                ad_analysis::analyze_view(&view, Default::default(), &c).unwrap(),
                target,
            )
        };
        let before = observe(&s);
        let tag = s.document().tag();
        let staged = if step == 0 {
            s.stage_undo(tag, &c).unwrap()
        } else {
            s.stage_redo(tag, &c).unwrap()
        };
        assert_eq!(staged.tag_after(), target);
        drop(staged);
        assert_eq!(observe(&s), before);
        let staged = if step == 0 {
            s.stage_undo(tag, &c).unwrap()
        } else {
            s.stage_redo(tag, &c).unwrap()
        };
        assert_eq!(staged.publish(), target);
        assert_eq!(s.document().primitives()[1].indices, untouched);
        same_snapshot(&snapshot, &s.document().snapshot(limits, &c).unwrap());
        let after = ad_analysis::analyze(s.document(), Default::default(), &c).unwrap();
        assert!(report.matches(s.document()));
        assert_eq!(report.findings, after.findings);
        assert_eq!(report.coverage, after.coverage);
    }
    assert_eq!(s.document().revision(), 3);
    assert!(s.is_modified());
}

#[test]
fn stage_refusals_leave_document_history_cursor_and_redo_unchanged() {
    let mut s = session();
    let c = Cancellation::default();
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let tag = s.document().tag();
    assert!(matches!(s.stage_undo(tag, &c), Err(e) if e.code == ErrorCode::HistoryEmpty));
    assert!(matches!(s.stage_redo(tag, &c), Err(e) if e.code == ErrorCode::HistoryEmpty));
    let noop = s.prepare_removal(tag, MeshId(0), &[], &c).unwrap();
    assert!(matches!(s.stage_commit(noop, &c), Err(e) if e.code == ErrorCode::Unsupported));

    let p = s.prepare_removal(tag, MeshId(0), &[1], &c).unwrap();
    s.commit(p, &c).unwrap();
    s.undo(s.document().tag(), &c).unwrap();
    let before = observe(&s);
    let tag = s.document().tag();
    let stale = DocumentTag {
        revision: tag.revision - 1,
        ..tag
    };
    assert!(matches!(s.stage_redo(stale, &c), Err(e) if e.code == ErrorCode::Conflict));
    assert!(matches!(s.stage_redo(tag, &cancelled), Err(e) if e.code == ErrorCode::Cancelled));
    assert!(matches!(s.stage_undo(tag, &c), Err(e) if e.code == ErrorCode::HistoryEmpty));
    let p = s.prepare_removal(tag, MeshId(0), &[0], &c).unwrap();
    assert!(matches!(s.stage_commit(p, &cancelled), Err(e) if e.code == ErrorCode::Cancelled));
    s.next_state = u64::MAX;
    let p = s.prepare_removal(tag, MeshId(0), &[0], &c).unwrap();
    assert!(matches!(s.stage_commit(p, &c), Err(e) if e.code == ErrorCode::ResourceLimit));
    s.next_state = 1;
    let mut other = session();
    let foreign = other
        .prepare_removal(other.document().tag(), MeshId(0), &[0], &c)
        .unwrap();
    other.commit(foreign, &c).unwrap();
    other.undo(other.document().tag(), &c).unwrap();
    assert_eq!(other.document().tag(), tag);
    let foreign = other.prepare_removal(tag, MeshId(0), &[0], &c).unwrap();
    assert!(matches!(s.stage_commit(foreign, &c), Err(e) if e.code == ErrorCode::Conflict));
    assert_eq!(observe(&s), before);
    assert!(s.can_redo());

    let mut full = Session::new(
        session().document,
        HistoryLimits {
            entries: 1,
            ..Default::default()
        },
        ImportLimits::default(),
    )
    .unwrap();
    let p = full
        .prepare_removal(full.document().tag(), MeshId(0), &[1], &c)
        .unwrap();
    full.commit(p, &c).unwrap();
    let before = observe(&full);
    let bypass = full
        .document()
        .prepare_triangle_removal(
            full.document().tag(),
            MeshId(0),
            &[0],
            ImportLimits::default(),
            &c,
        )
        .unwrap();
    assert!(matches!(full.stage_commit(bypass, &c), Err(e) if e.code == ErrorCode::ResourceLimit));
    assert_eq!(observe(&full), before);
}
