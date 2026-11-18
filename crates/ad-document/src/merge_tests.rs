//! ADR-0034: merging exact duplicate vertices of one primitive (REPAIR-04).
use crate::*;

/// Vertices: 0 a, 1 b, 2 c, 3 = b, 4 = a, 5 = a except UV, 6 = a except -0.0.
fn doc() -> AssetDocument {
    let mut data = crate::tests::parts();
    let mesh = &mut data.meshes[0];
    let a = [0., 0., 0.];
    let b = [1., 0., 0.];
    let c = [0., 1., 0.];
    mesh.positions = vec![a, b, c, b, a, a, [-0., 0., 0.]];
    mesh.normals = Some(vec![[0., 0., 1.]; 7]);
    mesh.uv_channels = vec![UvChannel {
        set: 0,
        values: vec![
            [0., 0.],
            [1., 0.],
            [0., 1.],
            [1., 0.],
            [0., 0.],
            [0.5, 0.],
            [0., 0.],
        ],
    }];
    mesh.indices = vec![0, 1, 2, 4, 3, 2, 5, 3, 6];
    AssetDocument::new(data, ImportLimits::default(), &Cancellation::default()).unwrap()
}

#[test]
fn merge_points_duplicates_at_earlier_twins_and_undo_is_exact() {
    let mut d = doc();
    let (c, l) = (Cancellation::default(), ImportLimits::default());
    let plan = d
        .prepare_vertex_merge(d.tag(), MeshId(0), &[[3, 1], [4, 0]], l, &c)
        .unwrap();
    let p = plan.preview();
    assert_eq!((p.before_vertices, p.after_vertices), (7, 5));
    assert_eq!((p.before_triangles, p.after_triangles), (3, 3));
    let after = plan.primitive_after(&d, &c).unwrap();
    // Survivors 0,1,2,5,6 keep order; 3 -> 1 and 4 -> 0.
    assert_eq!(after.indices, [0, 1, 2, 0, 1, 2, 3, 1, 4]);
    assert_eq!(after.positions.len(), 5);
    assert_eq!(after.uv_channels[0].values[3], [0.5, 0.]);
    assert_eq!(plan.source_vertices_after(&d), Some(&[0, 1, 2, 5, 6][..]));
    assert_eq!(d.revision(), 0);
    let mut change = d
        .apply_triangle_removal(plan, l, &c)
        .unwrap()
        .change
        .unwrap();
    assert_eq!(d.primitives()[0].indices, [0, 1, 2, 0, 1, 2, 3, 1, 4]);
    d.exchange_indices(d.tag(), &mut change, l, &c).unwrap();
    assert_eq!(d.primitives()[0].indices, [0, 1, 2, 4, 3, 2, 5, 3, 6]);
    assert_eq!(d.primitives()[0].positions.len(), 7);
    assert_eq!(d.source_vertices(MeshId(0)), None);
}

#[test]
fn merge_refuses_unequal_later_removed_or_unsorted_pairs() {
    let d = doc();
    let (c, l) = (Cancellation::default(), ImportLimits::default());
    for bad in [
        &[[5, 0]][..],     // differs in UV
        &[[6, 0]],         // differs by -0.0
        &[[1, 3]],         // target later than the removed vertex
        &[[3, 3]],         // self
        &[[4, 0], [3, 1]], // unsorted
        &[[3, 1], [4, 3]], // target is itself removed
        &[[9, 0]],         // out of range
    ] {
        let e = d
            .prepare_vertex_merge(d.tag(), MeshId(0), bad, l, &c)
            .err()
            .unwrap();
        assert_eq!(e.code, ErrorCode::InvalidGeometry, "{bad:?}");
    }
    let noop = d
        .prepare_vertex_merge(d.tag(), MeshId(0), &[], l, &c)
        .unwrap();
    assert!(!noop.changes());
}
