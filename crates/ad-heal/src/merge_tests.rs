//! ADR-0034: complete G012 plan and merge postcheck for one source primitive.
use super::*;
use ad_commands::{HistoryLimits, Session};
use ad_document::{Cancellation, DocumentId, ErrorCode, ImportLimits};
use std::sync::Arc;

/// Edge_Topology primitive #4 "Coincident split indices": vertices 3..5 repeat
/// 0..2 exactly, two triangles [0,1,2] and [3,4,5].
fn session() -> Session {
    let document = ad_formats::import_glb(
        Arc::from(include_bytes!("../../../tests/fixtures/Edge_Topology.glb").as_slice()),
        "merge".into(),
        DocumentId(6),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    Session::new(document, HistoryLimits::default(), ImportLimits::default()).unwrap()
}
fn split_mesh(s: &Session) -> MeshId {
    let report =
        ad_analysis::analyze(s.document(), Default::default(), &Cancellation::default()).unwrap();
    report
        .findings
        .iter()
        .find(|f| f.code == ad_analysis::Code::DuplicateVertex)
        .unwrap()
        .mesh
}
fn plan(s: &Session, mesh: MeshId) -> AssetResult<RemovalPlan> {
    plan_vertex_merge(
        s.document(),
        s.document().tag(),
        mesh,
        PlanLimits::default(),
        &Cancellation::default(),
    )
}

#[test]
fn plan_pairs_each_duplicate_with_its_earliest_twin_and_merge_verifies() {
    let mut s = session();
    let c = Cancellation::default();
    let mesh = split_mesh(&s);
    let p = plan(&s, mesh).unwrap();
    assert_eq!(p.kind(), RepairKind::DuplicateVertices);
    assert_eq!(p.removed_vertices(), [3, 4, 5]);
    assert_eq!(p.merge_targets(), [0, 1, 2]);
    assert!(p.removed_triangles().is_empty());
    let v = p.preview();
    assert_eq!((v.before_vertices, v.after_vertices), (6, 3));
    assert_eq!((v.before_triangles, v.after_triangles), (2, 2));
    let command = p.prepare(&s, VerificationLimits::default(), &c).unwrap();
    s.commit(command, &c).unwrap();
    let m = &s.document().primitives()[mesh.0];
    assert_eq!(m.positions.len(), 3);
    assert_eq!(m.indices, [0, 1, 2, 0, 1, 2]);
    assert!(plan(&s, mesh).unwrap().removed_vertices().is_empty());
    let report =
        ad_analysis::analyze(s.document(), Default::default(), &Cancellation::default()).unwrap();
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.code == ad_analysis::Code::DuplicateVertex)
    );
    assert!(matches!(
        p.prepare(&s, VerificationLimits::default(), &c),
        Err(e) if e.code == ErrorCode::Conflict
    ));
}

#[test]
fn merge_postcheck_refuses_wrong_twins_ranks_and_values() {
    let s = session();
    let mesh = split_mesh(&s);
    let source = &s.document().primitives()[mesh.0];
    let check = |targets: &[u32], edit: &dyn Fn(&mut ad_document::MeshPrimitive)| {
        let c = Cancellation::default();
        let p = s
            .document()
            .prepare_vertex_merge(
                s.document().tag(),
                mesh,
                &[[3, 0], [4, 1], [5, 2]],
                ImportLimits::default(),
                &c,
            )
            .unwrap();
        let after = p.primitive_after(s.document(), &c).unwrap();
        let mut candidate = ad_document::MeshPrimitive {
            positions: after.positions.clone(),
            indices: after.indices.clone(),
            normals: after.normals.clone(),
            tangents: after.tangents.clone(),
            uv_channels: Vec::new(),
            uv_bindings: after.uv_bindings,
            colors: after.colors.clone(),
            normal_texture: after.normal_texture.clone(),
            normal_scale: after.normal_scale,
            occlusion_texture: after.occlusion_texture.clone(),
            occlusion_strength: after.occlusion_strength,
            emissive_factor: after.emissive_factor,
            emissive_texture: after.emissive_texture.clone(),
            metallic_roughness: after.metallic_roughness,
            base_color: after.base_color,
            color_texture: after.color_texture.clone(),
            metallic_roughness_texture: after.metallic_roughness_texture.clone(),
            alpha_cutoff: after.alpha_cutoff,
            blend: false,
        };
        assert!(source.uv_channels.is_empty());
        edit(&mut candidate);
        verify::check_merge(
            source,
            None,
            (&[3, 4, 5], targets),
            &candidate,
            p.source_vertices_after(s.document()),
            VerificationLimits::default(),
            || Ok(()),
        )
    };
    check(&[0, 1, 2], &|_| {}).unwrap();
    assert!(check(&[1, 0, 2], &|_| {}).is_err(), "unequal twins");
    assert!(check(&[0, 1], &|_| {}).is_err(), "pair count");
    assert!(check(&[0, 1, 2], &|c| c.indices[3] = 1).is_err());
    assert!(check(&[0, 1, 2], &|c| c.positions[0][0] += 1.).is_err());
}
