use super::*;
use ad_document::{Cancellation, DocumentId, ImportLimits, MeshId};
use std::sync::Arc;

fn document() -> ad_document::AssetDocument {
    ad_formats::import_glb(
        Arc::from(include_bytes!("../../../tests/fixtures/Geometry_Findings.glb").as_slice()),
        "commands".into(),
        DocumentId(1),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap()
}
fn session() -> Session {
    Session::new(
        document(),
        HistoryLimits::default(),
        ImportLimits::default(),
    )
    .unwrap()
}

#[test]
fn apply_undo_redo_changes_current_indices_and_never_revives_old_reports() {
    let mut s = session();
    let cancel = Cancellation::default();
    let original = s.document().primitives()[0].indices.clone();
    let old = ad_analysis::analyze(s.document(), Default::default(), &cancel).unwrap();
    let plan = s
        .prepare_removal(s.document().tag(), MeshId(0), &[1], &cancel)
        .unwrap();
    assert_eq!(s.document().primitives()[0].indices, original);
    let outcome = s.commit(plan, &cancel).unwrap();
    assert!(outcome.changed);
    assert_eq!(outcome.tag.revision, 1);
    assert!(s.is_modified());
    assert!(!old.matches(s.document()));
    let edited = s.document().primitives()[0].indices.clone();
    assert_eq!(edited.len(), original.len() - 3);
    let fresh = ad_analysis::analyze(s.document(), Default::default(), &cancel).unwrap();
    assert!(
        !fresh
            .findings
            .iter()
            .any(|f| f.code == ad_analysis::Code::DuplicateTriangle)
    );
    assert_eq!(
        s.document()
            .snapshot(ImportLimits::default(), &cancel)
            .unwrap()
            .indices
            .len()
            / 3,
        9
    ); // Two visible source instances.
    s.undo(s.document().tag(), &cancel).unwrap();
    assert_eq!(s.document().primitives()[0].indices, original);
    assert_eq!(s.document().revision(), 2);
    assert!(!s.is_modified());
    assert!(!old.matches(s.document()));
    s.redo(s.document().tag(), &cancel).unwrap();
    assert_eq!(s.document().primitives()[0].indices, edited);
    assert_eq!(s.document().revision(), 3);
    assert!(s.is_modified());
}

#[test]
fn no_op_and_failed_branch_keep_redo_but_successful_branch_discards_it() {
    let mut s = session();
    let c = Cancellation::default();
    let p = s
        .prepare_removal(s.document().tag(), MeshId(0), &[1], &c)
        .unwrap();
    s.commit(p, &c).unwrap();
    s.undo(s.document().tag(), &c).unwrap();
    let usage = s.history_usage();
    let tag = s.document().tag();
    assert!(s.can_redo());
    assert!(s.prepare_removal(tag, MeshId(0), &[99], &c).is_err());
    let p = s.prepare_removal(tag, MeshId(0), &[0], &c).unwrap();
    let cancel = Cancellation::default();
    cancel.cancel();
    assert!(matches!(s.commit(p,&cancel),Err(e) if e.code==ad_document::ErrorCode::Cancelled));
    let p = s.prepare_removal(tag, MeshId(0), &[], &c).unwrap();
    let r = s.commit(p, &c).unwrap();
    assert!(!r.changed);
    assert_eq!(r.tag, tag);
    assert_eq!(s.history_usage(), usage);
    assert!(s.can_redo());
    assert!(!s.is_modified());
    let p = s.prepare_removal(tag, MeshId(0), &[0], &c).unwrap();
    s.commit(p, &c).unwrap();
    assert!(!s.can_redo());
    assert!(s.is_modified());
    assert_eq!(s.history_usage().undo, 1);
    assert_eq!(s.document().revision(), 3);
    assert!(
        matches!(s.redo(s.document().tag(),&c),Err(e) if e.code==ad_document::ErrorCode::HistoryEmpty)
    );
}

#[test]
fn same_content_after_undo_does_not_authorise_a_stale_prepared_command() {
    let mut s = session();
    let c = Cancellation::default();
    let tag = s.document().tag();
    let stale = s.prepare_removal(tag, MeshId(0), &[1], &c).unwrap();
    let p = s.prepare_removal(tag, MeshId(0), &[0], &c).unwrap();
    s.commit(p, &c).unwrap();
    s.undo(s.document().tag(), &c).unwrap();
    let usage = s.history_usage();
    assert!(matches!(s.commit(stale,&c),Err(e) if e.code==ad_document::ErrorCode::Conflict));
    assert_eq!(s.document().revision(), 2);
    assert_eq!(s.history_usage(), usage);
    assert!(matches!(s.redo(tag,&c),Err(e) if e.code==ad_document::ErrorCode::Conflict));
    s.redo(s.document().tag(), &c).unwrap();
}

#[test]
fn foreign_session_plans_are_rejected_even_when_public_tags_are_identical() {
    let a = session();
    let mut b = session();
    let c = Cancellation::default();
    assert_eq!(a.document().tag(), b.document().tag());
    for list in [vec![], vec![1]] {
        let p = a
            .prepare_removal(a.document().tag(), MeshId(0), &list, &c)
            .unwrap();
        assert!(matches!(b.commit(p,&c),Err(e) if e.code==ad_document::ErrorCode::Conflict));
        assert_eq!(b.document().revision(), 0);
        assert!(!b.can_undo());
        assert!(!b.is_modified());
    }
}

#[test]
fn retained_byte_boundary_is_exact_and_undo_redo_never_increase_reservation() {
    let roomy = Session::new(
        document(),
        HistoryLimits {
            entries: 1,
            ..Default::default()
        },
        ImportLimits::default(),
    )
    .unwrap();
    let c = Cancellation::default();
    let p = roomy
        .prepare_removal(roomy.document().tag(), MeshId(0), &[1], &c)
        .unwrap();
    let exact = roomy.history_usage().metadata_bytes + p.retained_bytes();
    let mut s = Session::new(
        document(),
        HistoryLimits {
            entries: 1,
            bytes: exact,
        },
        ImportLimits::default(),
    )
    .unwrap();
    let p = s
        .prepare_removal(s.document().tag(), MeshId(0), &[1], &c)
        .unwrap();
    s.commit(p, &c).unwrap();
    assert_eq!(s.history_usage().bytes, exact);
    for _ in 0..5 {
        s.undo(s.document().tag(), &c).unwrap();
        assert_eq!(s.history_usage().bytes, exact);
        s.redo(s.document().tag(), &c).unwrap();
        assert_eq!(s.history_usage().bytes, exact);
    }
    let too_small = Session::new(
        document(),
        HistoryLimits {
            entries: 1,
            bytes: exact - 1,
        },
        ImportLimits::default(),
    )
    .unwrap();
    assert!(
        matches!(too_small.prepare_removal(too_small.document().tag(),MeshId(0),&[1],&c),Err(e) if e.code==ad_document::ErrorCode::ResourceLimit)
    );
    assert_eq!(too_small.document().revision(), 0);
}

#[test]
fn entry_limit_and_direct_plan_cannot_bypass_commit_admission() {
    let mut s = Session::new(
        document(),
        HistoryLimits {
            entries: 1,
            ..Default::default()
        },
        ImportLimits::default(),
    )
    .unwrap();
    let c = Cancellation::default();
    let p = s
        .prepare_removal(s.document().tag(), MeshId(0), &[1], &c)
        .unwrap();
    s.commit(p, &c).unwrap();
    let usage = s.history_usage();
    let indices = s.document().primitives()[0].indices.clone();
    assert!(
        matches!(s.prepare_removal(s.document().tag(),MeshId(0),&[0],&c),Err(e) if e.code==ad_document::ErrorCode::ResourceLimit)
    );
    let bypass = s
        .document()
        .prepare_triangle_removal(
            s.document().tag(),
            MeshId(0),
            &[0],
            ImportLimits::default(),
            &c,
        )
        .unwrap();
    assert!(matches!(s.commit(bypass,&c),Err(e) if e.code==ad_document::ErrorCode::ResourceLimit));
    assert_eq!(s.document().primitives()[0].indices, indices);
    assert_eq!(s.history_usage(), usage);
    s.undo(s.document().tag(), &c).unwrap();
    let p = s
        .prepare_removal(s.document().tag(), MeshId(0), &[0], &c)
        .unwrap();
    s.commit(p, &c).unwrap();
    assert_eq!(s.history_usage().undo, 1);
    assert!(!s.can_redo());
}

#[test]
fn disabled_history_still_allows_read_only_session_and_no_op() {
    let mut s = Session::new(
        document(),
        HistoryLimits {
            entries: 0,
            bytes: 0,
        },
        ImportLimits::default(),
    )
    .unwrap();
    let c = Cancellation::default();
    let tag = s.document().tag();
    let p = s.prepare_removal(tag, MeshId(0), &[], &c).unwrap();
    assert!(!s.commit(p, &c).unwrap().changed);
    assert!(
        matches!(s.prepare_removal(tag,MeshId(0),&[0],&c),Err(e) if e.code==ad_document::ErrorCode::ResourceLimit)
    );
    assert!(matches!(s.undo(tag,&c),Err(e) if e.code==ad_document::ErrorCode::HistoryEmpty));
    assert!(matches!(s.redo(tag,&c),Err(e) if e.code==ad_document::ErrorCode::HistoryEmpty));
    assert_eq!(s.history_usage().bytes, 0);
    assert_eq!(s.document().tag(), tag);
}

#[test]
fn cancelled_undo_and_redo_leave_history_cursor_and_buffers_unchanged() {
    let mut s = session();
    let c = Cancellation::default();
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let p = s
        .prepare_removal(s.document().tag(), MeshId(0), &[1], &c)
        .unwrap();
    s.commit(p, &c).unwrap();
    let usage = s.history_usage();
    let pointer = s.document().primitives()[0].indices.as_ptr();
    assert!(
        matches!(s.undo(s.document().tag(),&cancelled),Err(e) if e.code==ad_document::ErrorCode::Cancelled)
    );
    assert_eq!(s.history_usage(), usage);
    assert_eq!(s.document().primitives()[0].indices.as_ptr(), pointer);
    s.undo(s.document().tag(), &c).unwrap();
    let usage = s.history_usage();
    assert!(
        matches!(s.redo(s.document().tag(),&cancelled),Err(e) if e.code==ad_document::ErrorCode::Cancelled)
    );
    assert_eq!(s.history_usage(), usage);
    assert!(!s.is_modified());
}

#[test]
fn state_overflow_refuses_before_branch_changes_or_document_mutation() {
    let mut s = session();
    let c = Cancellation::default();
    let p = s
        .prepare_removal(s.document().tag(), MeshId(0), &[1], &c)
        .unwrap();
    s.commit(p, &c).unwrap();
    s.undo(s.document().tag(), &c).unwrap();
    let usage = s.history_usage();
    let tag = s.document().tag();
    s.next_state = u64::MAX;
    let p = s.prepare_removal(tag, MeshId(0), &[0], &c).unwrap();
    assert!(matches!(s.commit(p,&c),Err(e) if e.code==ad_document::ErrorCode::ResourceLimit));
    assert_eq!(s.history_usage(), usage);
    assert_eq!(s.document().tag(), tag);
    assert!(s.can_redo());
}

#[test]
fn textured_document_attributes_and_images_are_shared_through_history() {
    let source: Arc<[u8]> =
        Arc::from(include_bytes!("../../../tests/fixtures/Normal_Checker.glb").as_slice());
    let doc = ad_formats::import_glb(
        source.clone(),
        "normal".into(),
        DocumentId(2),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    let positions = doc.primitives()[0].positions.as_ptr();
    let normals = doc.primitives()[0].normals.as_ref().unwrap().as_ptr();
    let uvs = doc.primitives()[0].uv_channels[0].values.as_ptr();
    let texture = doc.primitives()[0].normal_texture.as_ref().unwrap().clone();
    let mut s = Session::new(doc, Default::default(), Default::default()).unwrap();
    let c = Cancellation::default();
    let p = s
        .prepare_removal(s.document().tag(), MeshId(0), &[0], &c)
        .unwrap();
    s.commit(p, &c).unwrap();
    s.undo(s.document().tag(), &c).unwrap();
    s.redo(s.document().tag(), &c).unwrap();
    let mesh = &s.document().primitives()[0];
    assert_eq!(mesh.positions.as_ptr(), positions);
    assert_eq!(mesh.normals.as_ref().unwrap().as_ptr(), normals);
    assert_eq!(mesh.uv_channels[0].values.as_ptr(), uvs);
    assert!(Arc::ptr_eq(mesh.normal_texture.as_ref().unwrap(), &texture));
    assert!(Arc::ptr_eq(s.document().source(), &source));
}

#[test]
fn interleaved_primitives_use_one_history_and_restore_in_reverse_order() {
    let c = Cancellation::default();
    let doc = ad_formats::import_glb(
        Arc::from(include_bytes!("../../../tests/fixtures/Normal_Checker.glb").as_slice()),
        "two targets".into(),
        DocumentId(3),
        ImportLimits::default(),
        &c,
    )
    .unwrap();
    let original: Vec<_> = doc.primitives().iter().map(|m| m.indices.len()).collect();
    let mut s = Session::new(doc, Default::default(), Default::default()).unwrap();
    for id in [0, 1] {
        let p = s
            .prepare_removal(s.document().tag(), MeshId(id), &[0], &c)
            .unwrap();
        s.commit(p, &c).unwrap();
    }
    assert_eq!(s.document().primitives()[0].indices.len(), original[0] - 3);
    assert_eq!(s.document().primitives()[1].indices.len(), original[1] - 3);
    s.undo(s.document().tag(), &c).unwrap();
    assert_eq!(s.document().primitives()[0].indices.len(), original[0] - 3);
    assert_eq!(s.document().primitives()[1].indices.len(), original[1]);
    s.undo(s.document().tag(), &c).unwrap();
    assert_eq!(
        s.document()
            .primitives()
            .iter()
            .map(|m| m.indices.len())
            .collect::<Vec<_>>(),
        original
    );
    assert!(!s.is_modified());
    assert_eq!(s.document().revision(), 4);
    for _ in 0..2 {
        s.redo(s.document().tag(), &c).unwrap();
    }
    assert_eq!(s.document().primitives()[0].indices.len(), original[0] - 3);
    assert_eq!(s.document().primitives()[1].indices.len(), original[1] - 3);
    assert_eq!(s.document().revision(), 6);
    assert!(s.is_modified());
}

#[test]
fn byte_limited_branch_reclaims_only_redo_and_rejection_preserves_that_redo() {
    let c = Cancellation::default();
    let fresh = || {
        ad_formats::import_glb(
            Arc::from(include_bytes!("../../../tests/fixtures/Normal_Checker.glb").as_slice()),
            "branch".into(),
            DocumentId(4),
            ImportLimits::default(),
            &c,
        )
        .unwrap()
    };
    let mut probe = Session::new(
        fresh(),
        HistoryLimits {
            entries: 2,
            ..Default::default()
        },
        Default::default(),
    )
    .unwrap();
    let metadata = probe.history_usage().metadata_bytes;
    let first = probe
        .prepare_removal(probe.document().tag(), MeshId(0), &[0], &c)
        .unwrap();
    let a = first.retained_bytes();
    probe.commit(first, &c).unwrap();
    let second = probe
        .prepare_removal(probe.document().tag(), MeshId(0), &[0], &c)
        .unwrap();
    let b = second.retained_bytes();
    assert!(a > b);
    let exact = metadata + a + b;
    let mut s = Session::new(
        fresh(),
        HistoryLimits {
            entries: 2,
            bytes: exact,
        },
        Default::default(),
    )
    .unwrap();
    for _ in 0..2 {
        let p = s
            .prepare_removal(s.document().tag(), MeshId(0), &[0], &c)
            .unwrap();
        s.commit(p, &c).unwrap();
    }
    assert_eq!(s.history_usage().bytes, exact);
    let old_branch = s.document().primitives()[0].indices.clone();
    s.undo(s.document().tag(), &c).unwrap();
    let tag = s.document().tag();
    let usage = s.history_usage();
    // Other primitive's original buffer is larger than the available suffix.
    assert!(s.document().index_capacity_bytes(MeshId(1)).unwrap() > b);
    assert!(
        matches!(s.prepare_removal(tag,MeshId(1),&[0],&c),Err(e) if e.code==ad_document::ErrorCode::ResourceLimit)
    );
    let direct = s
        .document()
        .prepare_triangle_removal(tag, MeshId(1), &[0], Default::default(), &c)
        .unwrap();
    assert!(matches!(s.commit(direct,&c),Err(e) if e.code==ad_document::ErrorCode::ResourceLimit));
    assert_eq!(s.document().tag(), tag);
    assert_eq!(s.history_usage(), usage);
    assert!(s.can_redo());
    let new_branch = s.prepare_removal(tag, MeshId(0), &[1], &c).unwrap();
    s.commit(new_branch, &c).unwrap();
    assert_eq!(s.history_usage().bytes, exact);
    assert!(!s.can_redo());
    assert_ne!(s.document().primitives()[0].indices, old_branch);
    for _ in 0..2 {
        s.undo(s.document().tag(), &c).unwrap();
    }
    assert!(!s.is_modified());
    for _ in 0..2 {
        s.redo(s.document().tag(), &c).unwrap();
    }
    assert!(s.is_modified());
    assert_eq!(s.history_usage().bytes, exact);
}

#[test]
fn impossible_history_allocation_is_a_resource_error_not_a_panic() {
    assert!(
        matches!(Session::new(document(),HistoryLimits {entries:usize::MAX,bytes:usize::MAX},ImportLimits::default()),Err(e) if e.code==ad_document::ErrorCode::ResourceLimit)
    );
}

#[test]
fn session_cannot_relabel_an_edited_document_as_an_unmodified_import() {
    let mut doc = document();
    let c = Cancellation::default();
    let p = doc
        .prepare_triangle_removal(doc.tag(), MeshId(0), &[1], ImportLimits::default(), &c)
        .unwrap();
    let mut change = doc
        .apply_triangle_removal(p, ImportLimits::default(), &c)
        .unwrap()
        .change
        .unwrap();
    // Even content restored by a standalone Undo is not a fresh imported session.
    doc.exchange_indices(doc.tag(), &mut change, ImportLimits::default(), &c)
        .unwrap();
    assert_eq!(doc.revision(), 2);
    assert!(
        matches!(Session::new(doc,HistoryLimits::default(),ImportLimits::default()),Err(e) if e.code==ad_document::ErrorCode::Conflict)
    );
}
