//! ADR-0037: replacing a primitive's normal stream (REPAIR-03).
use crate::*;

/// Four vertices, two triangles; normals: v0 authored, v1/v2 zero, v3 authored.
fn doc(normals: Option<Vec<[f32; 3]>>) -> AssetDocument {
    let mut data = crate::tests::parts();
    let mesh = &mut data.meshes[0];
    mesh.positions = vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [1., 1., 0.]];
    mesh.indices = vec![0, 1, 2, 2, 1, 3];
    mesh.normals = normals;
    AssetDocument::new(data, ImportLimits::default(), &Cancellation::default()).unwrap()
}
const UP: [f32; 3] = [0., 0., 1.];

#[test]
fn normals_replacement_marks_the_stream_and_undo_is_exact() {
    let authored = vec![[0., 1., 0.], [0.; 3], [0.; 3], [1., 0., 0.]];
    let mut d = doc(Some(authored.clone()));
    let (c, l) = (Cancellation::default(), ImportLimits::default());
    assert!(!d.normals_changed(MeshId(0)));
    let new = vec![[0., 1., 0.], UP, UP, [1., 0., 0.]];
    let plan = d
        .prepare_normals(d.tag(), MeshId(0), new.clone(), true, l, &c)
        .unwrap();
    assert!(plan.changes());
    let p = plan.preview();
    assert_eq!((p.before_vertices, p.after_vertices), (4, 4));
    {
        let view = d
            .candidate_view(plan.candidate(&d, &c).unwrap(), &c)
            .unwrap();
        assert!(view.normals_changed(MeshId(0)));
        assert_eq!(view.primitive(MeshId(0)).normals.as_deref(), Some(&new[..]));
    }
    let mut change = d
        .apply_triangle_removal(plan, l, &c)
        .unwrap()
        .change
        .unwrap();
    assert_eq!(d.primitives()[0].normals.as_deref(), Some(&new[..]));
    assert_eq!(d.primitives()[0].indices, [0, 1, 2, 2, 1, 3]);
    assert!(d.normals_changed(MeshId(0)));
    assert_eq!(d.source_vertices(MeshId(0)), None);
    d.exchange_indices(d.tag(), &mut change, l, &c).unwrap();
    assert_eq!(d.primitives()[0].normals.as_deref(), Some(&authored[..]));
    assert!(!d.normals_changed(MeshId(0)));
}

#[test]
fn normals_replacement_creates_a_missing_stream() {
    let d = doc(None);
    let (c, l) = (Cancellation::default(), ImportLimits::default());
    let plan = d
        .prepare_normals(d.tag(), MeshId(0), vec![UP; 4], false, l, &c)
        .unwrap();
    assert_eq!(
        plan.primitive_after(&d, &c).unwrap().normals.as_deref(),
        Some(&[UP; 4][..])
    );
}

#[test]
fn normals_replacement_refuses_bad_streams_changed_authored_normals_and_tangents() {
    let (c, l) = (Cancellation::default(), ImportLimits::default());
    let d = doc(Some(vec![[0., 1., 0.], [0.; 3], [0.; 3], [1., 0., 0.]]));
    for bad in [
        vec![UP; 3],                          // wrong length
        vec![UP, UP, UP, [0., 0., 0.]],       // zero normal
        vec![UP, UP, UP, [0., 0., 2.]],       // not unit length
        vec![UP, UP, UP, [f32::NAN, 0., 1.]], // not finite
    ] {
        assert!(
            d.prepare_normals(d.tag(), MeshId(0), bad, false, l, &c)
                .is_err()
        );
    }
    // Keep-authored mode: v0 was authored and must not change.
    let e = d
        .prepare_normals(d.tag(), MeshId(0), vec![UP; 4], true, l, &c)
        .err()
        .unwrap();
    assert_eq!(e.code, ErrorCode::InvalidGeometry);
    let mut data = crate::tests::parts();
    data.meshes[0].normals = Some(vec![UP; 4]);
    data.meshes[0].tangents = Some(vec![[1., 0., 0., 1.]; 4]);
    let t = AssetDocument::new(data, l, &c).unwrap();
    let e = t
        .prepare_normals(t.tag(), MeshId(0), vec![UP; 4], false, l, &c)
        .err()
        .unwrap();
    assert_eq!(e.code, ErrorCode::Unsupported);
}
