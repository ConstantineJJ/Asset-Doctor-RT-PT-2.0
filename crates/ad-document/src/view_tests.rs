use crate::*;

fn doc() -> AssetDocument {
    let mut data = crate::tests::parts();
    data.meshes[0].indices = vec![0, 1, 2, 1, 2, 0, 0, 2, 1, 0, 1, 3];
    AssetDocument::new(data, ImportLimits::default(), &Cancellation::default()).unwrap()
}

fn assert_same(a: &GeometrySnapshot, b: &GeometrySnapshot) {
    assert_eq!(a.document_id, b.document_id);
    assert_eq!(a.revision, b.revision);
    assert_eq!(a.positions, b.positions);
    assert_eq!(a.normals, b.normals);
    assert_eq!(a.tangents, b.tangents);
    assert_eq!(a.uvs, b.uvs);
    assert_eq!(a.metallic_roughness_uvs, b.metallic_roughness_uvs);
    assert_eq!(a.normal_uvs, b.normal_uvs);
    assert_eq!(a.occlusion_uvs, b.occlusion_uvs);
    assert_eq!(a.emissive_uvs, b.emissive_uvs);
    assert_eq!(a.colors, b.colors);
    assert_eq!(a.indices, b.indices);
    assert_eq!(a.fallback_normal_vertices, b.fallback_normal_vertices);
    assert_eq!(a.fallback_tangent_vertices, b.fallback_tangent_vertices);
    assert_eq!(a.bounds.min, b.bounds.min);
    assert_eq!(a.bounds.max, b.bounds.max);
    assert_eq!(a.draws.len(), b.draws.len());
    for (x, y) in a.draws.iter().zip(&b.draws) {
        assert_eq!(
            (x.index_start, x.index_count, x.base_color),
            (y.index_start, y.index_count, y.base_color)
        );
    }
}

#[test]
fn candidate_snapshot_equals_the_snapshot_after_a_real_commit_without_mutation() {
    let mut d = doc();
    let c = Cancellation::default();
    let limits = ImportLimits::default();
    let plan = d
        .prepare_triangle_removal(d.tag(), MeshId(0), &[1, 2], limits, &c)
        .unwrap();
    let candidate = {
        let view = d
            .candidate_view(plan.candidate(&d, &c).unwrap(), &c)
            .unwrap();
        assert!(view.is_candidate());
        assert_eq!(view.tag().revision, 1);
        assert_eq!(view.indices(MeshId(0)), [0, 1, 2, 0, 1, 3]);
        view.snapshot(limits, &c).unwrap()
    };
    assert_eq!(d.revision(), 0);
    assert_eq!(
        d.primitives()[0].indices,
        [0, 1, 2, 1, 2, 0, 0, 2, 1, 0, 1, 3]
    );
    let current = d.view();
    assert!(!current.is_candidate());
    assert_same(
        &current.snapshot(limits, &c).unwrap(),
        &d.snapshot(limits, &c).unwrap(),
    );
    d.apply_triangle_removal(plan, limits, &c).unwrap();
    assert_same(&candidate, &d.snapshot(limits, &c).unwrap());
}

#[test]
fn candidate_view_is_bound_to_origin_revision_cancellation_and_valid_indices() {
    let mut d = doc();
    let c = Cancellation::default();
    let limits = ImportLimits::default();
    let plan = d
        .prepare_triangle_removal(d.tag(), MeshId(0), &[1], limits, &c)
        .unwrap();
    let foreign = doc();
    assert_eq!(foreign.tag(), d.tag());
    let candidate = plan.candidate(&d, &c).unwrap();
    assert!(
        matches!(foreign.candidate_view(candidate, &c), Err(e) if e.code == ErrorCode::Conflict)
    );
    let cancelled = Cancellation::default();
    cancelled.cancel();
    assert!(
        matches!(d.candidate_view(candidate, &cancelled), Err(e) if e.code == ErrorCode::Cancelled)
    );
    assert!(matches!(plan.candidate(&foreign, &c), Err(e) if e.code == ErrorCode::Conflict));

    // A no-op never publishes, so it has no future revision to preview.
    let noop = d
        .prepare_triangle_removal(d.tag(), MeshId(0), &[], limits, &c)
        .unwrap();
    assert!(matches!(noop.candidate(&d, &c), Err(e) if e.code == ErrorCode::Unsupported));

    // Hostile in-crate candidates are refused before any snapshot/analysis read.
    let hostile = [0, 1, 9];
    let bad = CandidateIndices::new(&d, MeshId(0), &hostile).unwrap();
    assert!(matches!(d.candidate_view(bad, &c), Err(e) if e.code == ErrorCode::InvalidGeometry));
    let partial = [0, 1];
    let bad = CandidateIndices::new(&d, MeshId(0), &partial).unwrap();
    assert!(matches!(d.candidate_view(bad, &c), Err(e) if e.code == ErrorCode::InvalidGeometry));
    let bad = CandidateIndices::new(&d, MeshId(0), &[]).unwrap();
    assert!(matches!(d.candidate_view(bad, &c), Err(e) if e.code == ErrorCode::InvalidGeometry));
    assert!(CandidateIndices::new(&d, MeshId(5), &hostile).is_err());

    // After a real commit the old plan cannot produce a candidate any more.
    let other = d
        .prepare_triangle_removal(d.tag(), MeshId(0), &[2], limits, &c)
        .unwrap();
    d.apply_triangle_removal(other, limits, &c).unwrap();
    assert!(matches!(plan.candidate(&d, &c), Err(e) if e.code == ErrorCode::Conflict));
}
