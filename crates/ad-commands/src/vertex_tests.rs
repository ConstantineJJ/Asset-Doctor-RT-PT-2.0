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

/// ADR-0041: a placement edit goes through the same history and moves the
/// snapshot; Undo restores the imported placement exactly.
#[test]
fn placement_commits_undoes_and_redoes_and_moves_the_snapshot() {
    use ad_document::Placement;
    let mut s = session(HistoryLimits::default());
    let c = Cancellation::default();
    let before = s.document().snapshot(ImportLimits::default(), &c).unwrap();
    let tag = s.document().tag();
    let same = s.prepare_placement(tag, Placement::IDENTITY, &c).unwrap();
    assert!(!same.changes(), "same placement is a no-op");
    let target = Placement::IDENTITY.then([0.; 3], 2., [1., 0., 0.]);
    let plan = s.prepare_placement(tag, target, &c).unwrap();
    let candidate = plan.candidate(s.document(), &c).unwrap();
    assert!(candidate.changes_placement() && !candidate.changes_vertices());
    let view = s.document().candidate_view(candidate, &c).unwrap();
    assert_eq!(view.placement(), target);
    let preview = view.snapshot(ImportLimits::default(), &c).unwrap();
    assert_eq!(preview.revision, 1);
    assert_eq!(
        s.document().placement(),
        Placement::IDENTITY,
        "preview only"
    );
    let outcome = s.commit(plan, &c).unwrap();
    assert!(outcome.changed && s.is_modified());
    let after = s.document().snapshot(ImportLimits::default(), &c).unwrap();
    for (a, b) in before.positions.iter().zip(&after.positions) {
        assert_eq!(*b, target.apply(*a));
    }
    assert_eq!(after.indices, before.indices);
    assert_eq!(after.bounds.min, target.apply(before.bounds.min));
    s.undo(s.document().tag(), &c).unwrap();
    assert_eq!(s.document().placement(), Placement::IDENTITY);
    assert!(!s.is_modified());
    s.redo(s.document().tag(), &c).unwrap();
    assert_eq!(s.document().placement(), target);
    let bad = Placement {
        scale: 0.,
        ..Placement::IDENTITY
    };
    let error = s
        .prepare_placement(s.document().tag(), bad, &c)
        .err()
        .unwrap();
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert!(
        s.prepare_placement(tag, target, &c).is_err(),
        "stale tag is refused"
    );
}
