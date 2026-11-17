//! ADR-0032: complete G001 zero-area removal for one source primitive.
use super::*;
use ad_commands::{HistoryLimits, Session};
use ad_document::{
    Cancellation, DocumentId, DocumentParts, ErrorCode, ImportLimits, MeshPrimitive, Node, NodeId,
    UvBindings,
};
use std::sync::Arc;

/// Two primitives over the same positions; mesh 0 is referenced by an active
/// root and its child, mesh 1 only by an inactive (unreachable) node.
fn document(positions: Vec<[f32; 3]>, parts: Vec<Vec<[u32; 3]>>) -> AssetDocument {
    let meshes = parts
        .into_iter()
        .map(|faces| MeshPrimitive {
            positions: positions.clone(),
            indices: faces.into_iter().flatten().collect(),
            normals: None,
            tangents: None,
            uv_channels: Vec::new(),
            uv_bindings: UvBindings::default(),
            colors: None,
            normal_texture: None,
            normal_scale: 1.,
            occlusion_texture: None,
            occlusion_strength: 1.,
            emissive_factor: [0.; 3],
            emissive_texture: None,
            metallic_roughness: [0., 1.],
            base_color: [1.; 4],
            color_texture: None,
            metallic_roughness_texture: None,
            alpha_cutoff: None,
        })
        .collect::<Vec<_>>();
    let identity = [
        [1., 0., 0., 0.],
        [0., 1., 0., 0.],
        [0., 0., 1., 0.],
        [0., 0., 0., 1.],
    ];
    let node = |name: &str, children: Vec<NodeId>, mesh: usize| Node {
        name: name.into(),
        local: identity,
        children,
        meshes: Arc::from([MeshId(mesh)]),
    };
    AssetDocument::new(
        DocumentParts {
            id: DocumentId(5),
            name: "degenerate".into(),
            source: Arc::from(b"degenerate source".as_slice()),
            meshes,
            nodes: vec![
                node("Root", vec![NodeId(1)], 0),
                node("Shared", Vec::new(), 0),
                node("Inactive", Vec::new(), 1),
            ],
            roots: vec![NodeId(0)],
            notes: Vec::new(),
        },
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap()
}
fn session(document: AssetDocument) -> Session {
    Session::new(document, HistoryLimits::default(), ImportLimits::default()).unwrap()
}
fn positions() -> Vec<[f32; 3]> {
    vec![
        [0., 0., 0.],
        [1., 0., 0.],
        [2., 0., 0.],     // collinear with 0 and 1
        [0.5, 1e-30, 0.], // tiny but real height: not degenerate
        [0., 1., 0.],
    ]
}

#[test]
fn plan_removes_exactly_the_g001_set_and_keeps_thin_real_faces_and_order() {
    let faces = vec![
        [0, 1, 4], // real
        [0, 1, 2], // collinear distinct indices
        [0, 1, 3], // thin but non-zero
        [1, 1, 4], // repeated index
        [4, 1, 0], // real, reversed
    ];
    let mut s = session(document(
        positions(),
        vec![faces, vec![[0, 1, 4], [0, 2, 1]]],
    ));
    let c = Cancellation::default();
    let tag = s.document().tag();
    let plan = plan_degenerate(s.document(), tag, MeshId(0), PlanLimits::default(), &c).unwrap();
    assert_eq!(plan.kind(), RepairKind::DegenerateTriangles);
    assert_eq!(plan.removed_triangles(), [1, 3]);
    let preview = plan.preview();
    assert_eq!((preview.before_triangles, preview.after_triangles), (5, 3));
    assert_eq!(
        preview.source_node_references, 2,
        "root and child, not the inactive node"
    );
    assert_eq!(s.document().revision(), 0, "planning never mutates");
    let command = plan.prepare(&s, VerificationLimits::default(), &c).unwrap();
    s.commit(command, &c).unwrap();
    assert_eq!(
        s.document().primitives()[0].indices,
        [0, 1, 4, 0, 1, 3, 4, 1, 0]
    );
    assert_eq!(
        s.document().primitives()[1].indices,
        [0, 1, 4, 0, 2, 1],
        "other primitive untouched"
    );
    let report = ad_analysis::analyze(s.document(), Default::default(), &c).unwrap();
    assert!(report.is_complete());
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.mesh == MeshId(0) && f.code == ad_analysis::Code::ZeroArea)
    );
    // Undo restores the exact source indices.
    s.undo(s.document().tag(), &c).unwrap();
    assert_eq!(
        s.document().primitives()[0].indices,
        [0, 1, 4, 0, 1, 2, 0, 1, 3, 1, 1, 4, 4, 1, 0]
    );
}

#[test]
fn no_degenerate_faces_is_a_no_op_and_an_all_degenerate_primitive_is_refused() {
    let s = session(document(
        positions(),
        vec![vec![[0, 1, 4], [0, 1, 3]], vec![[0, 1, 2], [2, 2, 1]]],
    ));
    let c = Cancellation::default();
    let tag = s.document().tag();
    let plan = plan_degenerate(s.document(), tag, MeshId(0), PlanLimits::default(), &c).unwrap();
    assert!(plan.removed_triangles().is_empty());
    let all = plan_degenerate(s.document(), tag, MeshId(1), PlanLimits::default(), &c);
    assert!(matches!(all, Err(e) if e.code == ErrorCode::Unsupported));
    assert!(matches!(
        plan_degenerate(s.document(), tag, MeshId(7), PlanLimits::default(), &c),
        Err(e) if e.code == ErrorCode::InvalidGeometry
    ));
}

#[test]
fn degenerate_postcheck_rejects_kept_zero_area_dropped_real_and_reordered_faces() {
    let p = positions();
    let before = [0, 1, 4, 0, 1, 2, 4, 1, 0];
    for (removed, after) in [
        (vec![], before.to_vec()),                  // zero-area face kept
        (vec![1, 2], vec![0, 1, 4]),                // real face dropped
        (vec![1], vec![4, 1, 0, 0, 1, 4]),          // survivors reordered
        (vec![1], vec![0, 1, 4, 0, 4, 1]),          // survivor winding changed
        (vec![0, 1], vec![4, 1, 0]),                // real face listed as removed
        (vec![1], vec![0, 1, 4, 4, 1, 0, 0, 1, 4]), // extra face
    ] {
        assert!(
            matches!(verify::check_degenerate(&p, &before, &removed, &after, || Ok(())), Err(e) if e.code == ErrorCode::VerificationFailed),
            "{removed:?} {after:?}"
        );
    }
    verify::check_degenerate(&p, &before, &[1], &[0, 1, 4, 4, 1, 0], || Ok(())).unwrap();
}

#[test]
fn stale_foreign_cancelled_and_over_limit_plans_never_mutate() {
    let faces = vec![[0, 1, 4], [0, 1, 2]];
    let mut s = session(document(positions(), vec![faces.clone(), vec![[0, 1, 4]]]));
    let c = Cancellation::default();
    let tag = s.document().tag();
    let cancelled = Cancellation::default();
    cancelled.cancel();
    assert!(matches!(
        plan_degenerate(s.document(), tag, MeshId(0), PlanLimits::default(), &cancelled),
        Err(e) if e.code == ErrorCode::Cancelled
    ));
    let tight = PlanLimits {
        triangles: 1,
        ..Default::default()
    };
    assert!(matches!(
        plan_degenerate(s.document(), tag, MeshId(0), tight, &c),
        Err(e) if e.code == ErrorCode::ResourceLimit
    ));
    let plan = plan_degenerate(s.document(), tag, MeshId(0), PlanLimits::default(), &c).unwrap();
    let foreign = session(document(positions(), vec![faces, vec![[0, 1, 4]]]));
    assert_eq!(foreign.document().tag(), tag);
    assert!(matches!(
        plan.prepare(&foreign, VerificationLimits::default(), &c),
        Err(e) if e.code == ErrorCode::Conflict
    ));
    let other = plan_degenerate(s.document(), tag, MeshId(0), PlanLimits::default(), &c).unwrap();
    let command = other
        .prepare(&s, VerificationLimits::default(), &c)
        .unwrap();
    s.commit(command, &c).unwrap();
    assert!(matches!(
        plan.prepare(&s, VerificationLimits::default(), &c),
        Err(e) if e.code == ErrorCode::Conflict
    ));
    assert_eq!(s.document().revision(), 1);
}

#[test]
fn authored_fixture_removes_collinear_and_repeated_faces_of_a_shared_primitive() {
    let document = ad_formats::import_glb(
        Arc::from(include_bytes!("../../../tests/fixtures/Geometry_Findings.glb").as_slice()),
        "fixture".into(),
        DocumentId(1),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    let c = Cancellation::default();
    let tag = document.tag();
    let plan = plan_degenerate(&document, tag, MeshId(0), PlanLimits::default(), &c).unwrap();
    assert_eq!(plan.removed_triangles(), [3, 4]);
    assert_eq!(plan.preview().source_node_references, 2);
}
