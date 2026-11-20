//! ADR-0033: unused-vertex compaction of one primitive (REPAIR-02).
use crate::*;

/// Six vertices, two triangles; vertices 1 and 4 are unused. Every decoded
/// stream carries the vertex ordinal so survivors can be identified.
fn doc() -> AssetDocument {
    let mut data = crate::tests::parts();
    let mesh = &mut data.meshes[0];
    mesh.positions = (0..6).map(|i| [i as f32, (i * i) as f32, 1.]).collect();
    mesh.normals = Some((0..6).map(|i| [0., 0., i as f32 + 1.]).collect());
    mesh.tangents = Some((0..6).map(|i| [i as f32, 0., 0., 1.]).collect());
    mesh.colors = Some((0..6).map(|i| [i as f32 / 8., 0., 0., 1.]).collect());
    mesh.uv_channels = vec![
        UvChannel {
            set: 0,
            values: (0..6).map(|i| [i as f32, 0.]).collect(),
        },
        UvChannel {
            set: 3,
            values: (0..6).map(|i| [0., i as f32]).collect(),
        },
    ];
    mesh.indices = vec![0, 2, 3, 3, 2, 5];
    AssetDocument::new(data, ImportLimits::default(), &Cancellation::default()).unwrap()
}

fn ordinals(mesh: &MeshPrimitive) -> Vec<u32> {
    mesh.positions.iter().map(|p| p[0] as u32).collect()
}

fn assert_streams_follow(mesh: &MeshPrimitive, kept: &[u32]) {
    assert_eq!(ordinals(mesh), kept);
    let k = |i: usize| kept[i] as f32;
    for i in 0..kept.len() {
        assert_eq!(mesh.normals.as_ref().unwrap()[i], [0., 0., k(i) + 1.]);
        assert_eq!(mesh.tangents.as_ref().unwrap()[i], [k(i), 0., 0., 1.]);
        assert_eq!(mesh.colors.as_ref().unwrap()[i], [k(i) / 8., 0., 0., 1.]);
        assert_eq!(mesh.uv_channels[0].values[i], [k(i), 0.]);
        assert_eq!(mesh.uv_channels[1].values[i], [0., k(i)]);
    }
    assert_eq!(
        mesh.uv_channels.iter().map(|c| c.set).collect::<Vec<_>>(),
        [0, 3]
    );
}

#[test]
fn compaction_renumbers_indices_keeps_attributes_and_undo_redo_are_exact() {
    let mut d = doc();
    let (c, l) = (Cancellation::default(), ImportLimits::default());
    assert_eq!(d.source_vertices(MeshId(0)), None);
    let plan = d
        .prepare_vertex_removal(d.tag(), MeshId(0), &[1, 4], l, &c)
        .unwrap();
    let p = plan.preview();
    assert_eq!((p.before_vertices, p.after_vertices), (6, 4));
    assert_eq!((p.before_triangles, p.after_triangles), (2, 2));
    assert!(plan.changes());
    // Preparing never mutates the document.
    assert_eq!(d.revision(), 0);
    assert_eq!(d.primitives()[0].positions.len(), 6);
    let after = plan.primitive_after(&d, &c).unwrap();
    assert_streams_follow(after, &[0, 2, 3, 5]);
    assert_eq!(after.indices, [0, 1, 2, 2, 1, 3]);

    let mut change = d
        .apply_triangle_removal(plan, l, &c)
        .unwrap()
        .change
        .unwrap();
    assert_eq!(d.revision(), 1);
    let mesh = &d.primitives()[0];
    assert_streams_follow(mesh, &[0, 2, 3, 5]);
    assert_eq!(mesh.indices, [0, 1, 2, 2, 1, 3]);
    assert_eq!(d.source_vertices(MeshId(0)), Some(&[0, 2, 3, 5][..]));
    assert!(change.retained_bytes() >= 6 * (12 + 12 + 16 + 16 + 16) + 6 * 4);

    d.exchange_indices(d.tag(), &mut change, l, &c).unwrap();
    assert_eq!(d.revision(), 2);
    assert_streams_follow(&d.primitives()[0], &[0, 1, 2, 3, 4, 5]);
    assert_eq!(d.primitives()[0].indices, [0, 2, 3, 3, 2, 5]);
    assert_eq!(d.source_vertices(MeshId(0)), None);

    d.exchange_indices(d.tag(), &mut change, l, &c).unwrap();
    assert_streams_follow(&d.primitives()[0], &[0, 2, 3, 5]);
    assert_eq!(d.source_vertices(MeshId(0)), Some(&[0, 2, 3, 5][..]));
}

#[test]
fn empty_removal_is_a_no_op_and_invalid_sets_are_refused() {
    let d = doc();
    let (c, l) = (Cancellation::default(), ImportLimits::default());
    let plan = d
        .prepare_vertex_removal(d.tag(), MeshId(0), &[], l, &c)
        .unwrap();
    assert!(!plan.changes());
    assert_eq!(plan.preview().after_vertices, 6);
    for bad in [&[4, 1][..], &[1, 1], &[6], &[0], &[1, 2]] {
        let e = d
            .prepare_vertex_removal(d.tag(), MeshId(0), bad, l, &c)
            .err()
            .unwrap();
        assert_eq!(e.code, ErrorCode::InvalidGeometry, "{bad:?}");
    }
    let stale = DocumentTag {
        revision: 9,
        ..d.tag()
    };
    assert!(matches!(
        d.prepare_vertex_removal(stale, MeshId(0), &[1], l, &c),
        Err(e) if e.code == ErrorCode::Conflict
    ));
    assert!(
        d.prepare_vertex_removal(d.tag(), MeshId(5), &[1], l, &c)
            .is_err()
    );
    let cancel = Cancellation::default();
    cancel.cancel();
    assert!(matches!(
        d.prepare_vertex_removal(d.tag(), MeshId(0), &[1], l, &cancel),
        Err(e) if e.code == ErrorCode::Cancelled
    ));
}

#[test]
fn repeated_compaction_composes_the_source_map_and_index_edits_keep_it() {
    let mut d = doc();
    let (c, l) = (Cancellation::default(), ImportLimits::default());
    let plan = d
        .prepare_vertex_removal(d.tag(), MeshId(0), &[1], l, &c)
        .unwrap();
    d.apply_triangle_removal(plan, l, &c).unwrap();
    assert_eq!(d.source_vertices(MeshId(0)), Some(&[0, 2, 3, 4, 5][..]));
    // Removing the second triangle frees source vertex 5 (current ordinal 4).
    let plan = d
        .prepare_triangle_removal(d.tag(), MeshId(0), &[1], l, &c)
        .unwrap();
    d.apply_triangle_removal(plan, l, &c).unwrap();
    assert_eq!(d.source_vertices(MeshId(0)), Some(&[0, 2, 3, 4, 5][..]));
    let plan = d
        .prepare_vertex_removal(d.tag(), MeshId(0), &[3, 4], l, &c)
        .unwrap();
    d.apply_triangle_removal(plan, l, &c).unwrap();
    assert_eq!(d.source_vertices(MeshId(0)), Some(&[0, 2, 3][..]));
    assert_streams_follow(&d.primitives()[0], &[0, 2, 3]);
    assert_eq!(d.primitives()[0].indices, [0, 1, 2]);
}

#[test]
fn candidate_view_substitutes_the_whole_primitive_without_mutation() {
    let mut d = doc();
    let (c, l) = (Cancellation::default(), ImportLimits::default());
    let plan = d
        .prepare_vertex_removal(d.tag(), MeshId(0), &[1, 4], l, &c)
        .unwrap();
    let candidate = {
        let view = d
            .candidate_view(plan.candidate(&d, &c).unwrap(), &c)
            .unwrap();
        assert_eq!(view.tag().revision, 1);
        assert_eq!(view.primitive(MeshId(0)).positions.len(), 4);
        assert_eq!(view.primitives().len(), 1);
        assert_eq!(view.primitives().get(0).unwrap().positions.len(), 4);
        assert_eq!(view.primitives().iter().count(), 1);
        assert_eq!(view.indices(MeshId(0)), [0, 1, 2, 2, 1, 3]);
        assert_eq!(view.source_vertices(MeshId(0)), Some(&[0, 2, 3, 5][..]));
        view.snapshot(l, &c).unwrap()
    };
    assert_eq!(d.primitives()[0].positions.len(), 6);
    assert_eq!(d.view().source_vertices(MeshId(0)), None);
    d.apply_triangle_removal(plan, l, &c).unwrap();
    let published = d.snapshot(l, &c).unwrap();
    assert_eq!(candidate.positions, published.positions);
    assert_eq!(candidate.normals, published.normals);
    assert_eq!(candidate.uvs, published.uvs);
    assert_eq!(candidate.indices, published.indices);
    assert_eq!(candidate.revision, published.revision);
}

#[test]
fn compaction_capacity_is_reported_for_history_admission() {
    let d = doc();
    let bytes = d.primitive_capacity_bytes(MeshId(0)).unwrap();
    assert!(bytes >= 6 * (12 + 12 + 16 + 16 + 16) + 6 * 4);
    assert!(bytes >= d.index_capacity_bytes(MeshId(0)).unwrap());
    assert!(d.primitive_capacity_bytes(MeshId(3)).is_err());
}
