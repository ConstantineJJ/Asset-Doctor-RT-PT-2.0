//! ADR-0033: vertex compaction goes through the same Session history,
//! staging and admission as triangle removal.
use super::*;
use ad_document::{Cancellation, DocumentId, ErrorCode, ImportLimits, MeshId};
use std::sync::Arc;

fn session(limits: HistoryLimits) -> Session {
    let document = ad_formats::import_glb(
        Arc::from(include_bytes!("../../../tests/fixtures/Geometry_Findings.glb").as_slice()),
        "vertices".into(),
        DocumentId(1),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    Session::new(document, limits, ImportLimits::default()).unwrap()
}

#[test]
fn compaction_commits_undoes_and_redoes_through_history() {
    let mut s = session(HistoryLimits::default());
    let c = Cancellation::default();
    let tag = s.document().tag();
    // Primitive #1 uses vertices 0..=2 of the shared five-vertex accessor.
    let p = s
        .prepare_vertex_removal(tag, MeshId(1), &[3, 4], &c)
        .unwrap();
    assert_eq!(p.preview().after_vertices, 3);
    let outcome = s.commit(p, &c).unwrap();
    assert!(outcome.changed);
    assert_eq!(outcome.tag.revision, 1);
    assert!(s.is_modified());
    assert_eq!(s.document().primitives()[1].positions.len(), 3);
    assert_eq!(s.document().primitives()[0].positions.len(), 5);
    assert_eq!(
        s.document().source_vertices(MeshId(1)),
        Some(&[0, 1, 2][..])
    );
    assert!(s.history_usage().bytes > s.history_usage().metadata_bytes);

    let undo = s.undo_candidate().unwrap();
    assert!(undo.changes_vertices());
    assert_eq!(undo.mesh(), MeshId(1));
    s.undo(s.document().tag(), &c).unwrap();
    assert!(!s.is_modified());
    assert_eq!(s.document().primitives()[1].positions.len(), 5);
    assert_eq!(s.document().source_vertices(MeshId(1)), None);
    s.redo(s.document().tag(), &c).unwrap();
    assert_eq!(s.document().primitives()[1].positions.len(), 3);
    assert_eq!(s.history_usage().undo, 1);
}

#[test]
fn empty_compaction_is_a_no_op_and_stage_refuses_it() {
    let mut s = session(HistoryLimits::default());
    let c = Cancellation::default();
    let tag = s.document().tag();
    let p = s.prepare_vertex_removal(tag, MeshId(1), &[], &c).unwrap();
    let outcome = s.commit(p, &c).unwrap();
    assert!(!outcome.changed);
    assert_eq!(outcome.tag, tag);
    let p = s.prepare_vertex_removal(tag, MeshId(1), &[], &c).unwrap();
    assert!(matches!(s.stage_commit(p, &c), Err(e) if e.code == ErrorCode::Unsupported));
    assert_eq!(s.history_usage().undo, 0);
}

#[test]
fn history_budget_refuses_a_primitive_too_large_to_keep_for_undo() {
    let roomy = session(HistoryLimits {
        bytes: 1 << 20,
        entries: 4,
    });
    let c = Cancellation::default();
    let plan = roomy
        .prepare_vertex_removal(roomy.document().tag(), MeshId(1), &[3, 4], &c)
        .unwrap();
    // History keeps the larger of the two primitives: here the original.
    let need = roomy
        .document()
        .primitive_capacity_bytes(MeshId(1))
        .unwrap();
    assert_eq!(plan.retained_bytes(), need);
    let metadata = roomy.history_usage().metadata_bytes;
    let fits = session(HistoryLimits {
        bytes: metadata + need,
        entries: 4,
    });
    assert!(
        fits.prepare_vertex_removal(fits.document().tag(), MeshId(1), &[3, 4], &c)
            .is_ok()
    );
    let tight = session(HistoryLimits {
        bytes: metadata + need - 1,
        entries: 4,
    });
    assert_eq!(tight.history_usage().metadata_bytes, metadata);
    assert!(matches!(
        tight.prepare_vertex_removal(tight.document().tag(), MeshId(1), &[3, 4], &c),
        Err(e) if e.code == ErrorCode::ResourceLimit
    ));
}
