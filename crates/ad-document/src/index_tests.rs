use crate::*;

fn doc() -> AssetDocument {
    let mut data = crate::tests::parts();
    data.meshes[0].indices = vec![0, 1, 2, 1, 2, 0, 0, 2, 1];
    AssetDocument::new(data, ImportLimits::default(), &Cancellation::default()).unwrap()
}

#[test]
fn prepared_triangle_removal_keeps_source_and_attributes_and_revisions_never_go_back() {
    let mut doc = doc();
    let source = doc.source().clone();
    let positions = doc.primitives()[0].positions.as_ptr();
    let original_indices = doc.primitives()[0].indices.as_ptr();
    let before = doc.tag();
    let plan = doc
        .prepare_triangle_removal(
            before,
            MeshId(0),
            &[1],
            ImportLimits::default(),
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(doc.revision(), 0);
    assert_eq!(doc.primitives()[0].indices.len(), 9);
    let applied = doc
        .apply_triangle_removal(plan, ImportLimits::default(), &Cancellation::default())
        .unwrap();
    assert_eq!(applied.tag.revision, 1);
    assert_eq!(doc.primitives()[0].indices, [0, 1, 2, 0, 2, 1]);
    let edited_indices = doc.primitives()[0].indices.as_ptr();
    let mut change = applied.change.unwrap();
    doc.exchange_indices(
        doc.tag(),
        &mut change,
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(doc.revision(), 2);
    assert_eq!(doc.primitives()[0].indices.as_ptr(), original_indices);
    doc.exchange_indices(
        doc.tag(),
        &mut change,
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(doc.revision(), 3);
    assert_eq!(doc.primitives()[0].indices.as_ptr(), edited_indices);
    assert_eq!(doc.primitives()[0].positions.as_ptr(), positions);
    assert!(std::sync::Arc::ptr_eq(doc.source(), &source));
    assert_eq!(
        doc.snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap()
            .revision,
        3
    );
}

#[test]
fn invalid_targets_lists_all_faces_and_empty_removal_are_atomic() {
    for (removed, code) in [
        (vec![0, 0], ErrorCode::InvalidGeometry),
        (vec![2, 1], ErrorCode::InvalidGeometry),
        (vec![3], ErrorCode::InvalidGeometry),
        (vec![0, 1, 2], ErrorCode::Unsupported),
    ] {
        let d = doc();
        assert!(
            matches!(d.prepare_triangle_removal(d.tag(),MeshId(0),&removed,ImportLimits::default(),&Cancellation::default()),Err(e) if e.code==code)
        );
        assert_eq!((d.revision(), d.primitives()[0].indices.len()), (0, 9));
    }
    let mut d = doc();
    assert!(
        matches!(d.prepare_triangle_removal(d.tag(),MeshId(99),&[],ImportLimits::default(),&Cancellation::default()),Err(e) if e.code==ErrorCode::InvalidGeometry)
    );
    let pointer = d.primitives()[0].indices.as_ptr();
    let plan = d
        .prepare_triangle_removal(
            d.tag(),
            MeshId(0),
            &[],
            ImportLimits::default(),
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(plan.retained_bytes(), 0);
    let r = d
        .apply_triangle_removal(plan, ImportLimits::default(), &Cancellation::default())
        .unwrap();
    assert!(r.change.is_none());
    assert_eq!(d.revision(), 0);
    assert_eq!(d.primitives()[0].indices.as_ptr(), pointer);
}

#[test]
fn stale_and_foreign_plans_and_history_buffers_cannot_mutate_another_document() {
    let mut d = doc();
    let mut other = doc();
    let cancel = Cancellation::default();
    let limits = ImportLimits::default();
    assert_eq!(d.tag(), other.tag()); // Public IDs alone cannot authorise a plan.
    for removed in [vec![], vec![1]] {
        let p = d
            .prepare_triangle_removal(d.tag(), MeshId(0), &removed, limits, &cancel)
            .unwrap();
        assert!(
            matches!(other.apply_triangle_removal(p,limits,&cancel),Err(e) if e.code==ErrorCode::Conflict)
        );
        assert_eq!(other.revision(), 0);
    }
    let stale = d
        .prepare_triangle_removal(d.tag(), MeshId(0), &[1], limits, &cancel)
        .unwrap();
    let p = d
        .prepare_triangle_removal(d.tag(), MeshId(0), &[0], limits, &cancel)
        .unwrap();
    let mut change = d
        .apply_triangle_removal(p, limits, &cancel)
        .unwrap()
        .change
        .unwrap();
    assert!(
        matches!(d.apply_triangle_removal(stale,limits,&cancel),Err(e) if e.code==ErrorCode::Conflict)
    );
    assert!(
        matches!(other.exchange_indices(other.tag(),&mut change,limits,&cancel),Err(e) if e.code==ErrorCode::Conflict)
    );
    assert_eq!(other.primitives()[0].indices.len(), 9);
    d.exchange_indices(d.tag(), &mut change, limits, &cancel)
        .unwrap();
    assert_eq!(d.primitives()[0].indices.len(), 9);
}

#[test]
fn decoded_limits_include_uv_and_inactive_meshes_and_undo_refusal_keeps_both_buffers() {
    let mut data = crate::tests::parts();
    data.meshes[0].indices = vec![0, 1, 2, 1, 2, 0, 0, 2, 1];
    data.meshes[0].uv_channels = vec![UvChannel {
        set: 0,
        values: vec![[0., 0.]; 4],
    }];
    data.meshes.push(crate::tests::parts().meshes.remove(0));
    let mut d =
        AssetDocument::new(data, ImportLimits::default(), &Cancellation::default()).unwrap();
    let cancel = Cancellation::default();
    let budget = |decoded_bytes| ImportLimits {
        decoded_bytes,
        ..Default::default()
    };
    // First mesh420bytes +32UV; inactive second mesh396bytes.
    assert!(
        matches!(d.prepare_triangle_removal(d.tag(),MeshId(0),&[1],budget(835),&cancel),Err(e) if e.code==ErrorCode::ResourceLimit)
    );
    let p = d
        .prepare_triangle_removal(d.tag(), MeshId(0), &[1], budget(836), &cancel)
        .unwrap();
    assert!(
        matches!(d.apply_triangle_removal(p,budget(835),&cancel),Err(e) if e.code==ErrorCode::ResourceLimit)
    );
    assert_eq!(d.revision(), 0);
    let p = d
        .prepare_triangle_removal(d.tag(), MeshId(0), &[1], budget(836), &cancel)
        .unwrap();
    let mut change = d
        .apply_triangle_removal(p, budget(836), &cancel)
        .unwrap()
        .change
        .unwrap();
    assert!(
        matches!(d.exchange_indices(d.tag(),&mut change,budget(847),&cancel),Err(e) if e.code==ErrorCode::ResourceLimit)
    );
    assert_eq!(d.primitives()[0].indices.len(), 6);
    assert_eq!(d.revision(), 1);
    d.exchange_indices(d.tag(), &mut change, budget(848), &cancel)
        .unwrap();
    assert_eq!(d.primitives()[0].indices.len(), 9);
    assert_eq!(d.revision(), 2);
}

#[test]
fn cancellation_and_revision_overflow_leave_document_and_alternate_indices_untouched() {
    let mut d = doc();
    let active = Cancellation::default();
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let limits = ImportLimits::default();
    assert!(
        matches!(d.prepare_triangle_removal(d.tag(),MeshId(0),&[1],limits,&cancelled),Err(e) if e.code==ErrorCode::Cancelled)
    );
    let p = d
        .prepare_triangle_removal(d.tag(), MeshId(0), &[1], limits, &active)
        .unwrap();
    assert!(
        matches!(d.apply_triangle_removal(p,limits,&cancelled),Err(e) if e.code==ErrorCode::Cancelled)
    );
    assert_eq!(d.revision(), 0);
    d.revision = u64::MAX;
    let p = d
        .prepare_triangle_removal(d.tag(), MeshId(0), &[1], limits, &active)
        .unwrap();
    assert!(
        matches!(d.apply_triangle_removal(p,limits,&active),Err(e) if e.code==ErrorCode::ResourceLimit)
    );
    assert_eq!(d.primitives()[0].indices.len(), 9);
    d.revision = 0;
    let p = d
        .prepare_triangle_removal(d.tag(), MeshId(0), &[1], limits, &active)
        .unwrap();
    let mut change = d
        .apply_triangle_removal(p, limits, &active)
        .unwrap()
        .change
        .unwrap();
    assert!(
        matches!(d.exchange_indices(d.tag(),&mut change,limits,&cancelled),Err(e) if e.code==ErrorCode::Cancelled)
    );
    d.revision = u64::MAX;
    assert!(
        matches!(d.exchange_indices(d.tag(),&mut change,limits,&active),Err(e) if e.code==ErrorCode::ResourceLimit)
    );
    assert_eq!(d.primitives()[0].indices.len(), 6);
    d.revision = 3;
    d.exchange_indices(d.tag(), &mut change, limits, &active)
        .unwrap();
    assert_eq!(d.primitives()[0].indices.len(), 9);
}
