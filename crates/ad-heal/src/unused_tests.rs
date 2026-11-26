//! ADR-0033: complete G003 unused-vertex removal for one source primitive.
use super::*;
use ad_commands::{HistoryLimits, Session};
use ad_document::{Cancellation, DocumentId, ErrorCode, ImportLimits, MeshPrimitive};
use std::sync::Arc;

/// Primitive #0 uses vertices 0..=3 of five; #1 uses 0..=2 (shared accessor).
fn session() -> Session {
    let document = ad_formats::import_glb(
        Arc::from(include_bytes!("../../../tests/fixtures/Geometry_Findings.glb").as_slice()),
        "unused".into(),
        DocumentId(3),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    Session::new(document, HistoryLimits::default(), ImportLimits::default()).unwrap()
}
fn plan(s: &Session, mesh: usize) -> AssetResult<RemovalPlan> {
    plan_unused(
        s.document(),
        s.document().tag(),
        MeshId(mesh),
        PlanLimits::default(),
        &Cancellation::default(),
    )
}

#[test]
fn plan_lists_exactly_the_vertices_no_triangle_of_the_primitive_uses() {
    let s = session();
    let p0 = plan(&s, 0).unwrap();
    assert_eq!(p0.kind(), RepairKind::UnusedVertices);
    assert_eq!(p0.removed_vertices(), [4]);
    let v = p0.preview();
    assert_eq!((v.before_vertices, v.after_vertices), (5, 4));
    assert_eq!((v.before_triangles, v.after_triangles), (5, 5));
    assert_eq!(v.source_node_references, 2);
    let p1 = plan(&s, 1).unwrap();
    assert_eq!(p1.removed_vertices(), [3, 4]);
    assert_eq!(p1.preview().source_node_references, 1);
    assert!(plan(&s, 9).is_err());
}

#[test]
fn verified_compaction_commits_and_a_second_plan_is_empty() {
    let mut s = session();
    let c = Cancellation::default();
    let p = plan(&s, 1).unwrap();
    let command = p.prepare(&s, VerificationLimits::default(), &c).unwrap();
    s.commit(command, &c).unwrap();
    let mesh = &s.document().primitives()[1];
    assert_eq!(mesh.positions.len(), 3);
    assert_eq!(mesh.indices, [0, 1, 2]);
    assert_eq!(
        s.document().source_vertices(MeshId(1)),
        Some(&[0, 1, 2][..])
    );
    let again = plan(&s, 1).unwrap();
    assert!(again.removed_vertices().is_empty());
    // Stale plan refused after the commit.
    assert!(matches!(
        p.prepare(&s, VerificationLimits::default(), &c),
        Err(e) if e.code == ErrorCode::Conflict
    ));
}

#[test]
fn cancellation_returns_no_plan() {
    let s = session();
    let cancel = Cancellation::default();
    cancel.cancel();
    assert!(matches!(
        plan_unused(s.document(), s.document().tag(), MeshId(0), PlanLimits::default(), &cancel),
        Err(e) if e.code == ErrorCode::Cancelled
    ));
}

/// Hostile candidates built by hand are refused by the independent check.
#[test]
fn postcheck_refuses_wrong_order_values_indices_maps_and_incomplete_sets() {
    let s = session();
    let source = &s.document().primitives()[0];
    let candidate = |edit: &dyn Fn(&mut MeshPrimitive, &mut Vec<u32>)| {
        let mut c = MeshPrimitive {
            positions: source.positions[..4].to_vec(),
            indices: source.indices.clone(),
            normals: source.normals.clone().map(|n| n[..4].to_vec()),
            tangents: None,
            uv_channels: Vec::new(),
            uv_bindings: source.uv_bindings,
            colors: None,
            normal_texture: None,
            normal_scale: 1.,
            occlusion_texture: None,
            occlusion_strength: 1.,
            emissive_factor: [0.; 3],
            emissive_texture: None,
            metallic_roughness: source.metallic_roughness,
            base_color: source.base_color,
            color_texture: None,
            metallic_roughness_texture: None,
            alpha_cutoff: None,
            blend: false,
        };
        let mut map = vec![0, 1, 2, 3];
        edit(&mut c, &mut map);
        verify::check_unused(
            source,
            None,
            &[4],
            &c,
            Some(&map),
            VerificationLimits::default(),
            || Ok(()),
        )
    };
    candidate(&|_, _| {}).unwrap();
    assert!(candidate(&|c, _| c.positions.swap(0, 1)).is_err());
    assert!(candidate(&|c, _| c.positions[3][0] = 7.).is_err());
    assert!(candidate(&|c, _| c.indices[0] = 1).is_err());
    assert!(candidate(&|_, m| m[3] = 4).is_err());
    assert!(candidate(&|c, _| c.positions.push([9.; 3])).is_err());
    // Mesh #0 has no NORMAL stream; the candidate must not invent one.
    assert!(source.normals.is_none());
    assert!(candidate(&|c, _| c.normals = Some(vec![[0.; 3]; 4])).is_err());
    assert!(candidate(&|c, _| c.base_color = [0.5; 4]).is_err());
    // A used vertex listed as removed, or an unused one kept, is refused.
    let c = MeshPrimitive {
        positions: source.positions.clone(),
        indices: source.indices.clone(),
        normals: None,
        tangents: None,
        uv_channels: Vec::new(),
        uv_bindings: source.uv_bindings,
        colors: None,
        normal_texture: None,
        normal_scale: 1.,
        occlusion_texture: None,
        occlusion_strength: 1.,
        emissive_factor: [0.; 3],
        emissive_texture: None,
        metallic_roughness: source.metallic_roughness,
        base_color: source.base_color,
        color_texture: None,
        metallic_roughness_texture: None,
        alpha_cutoff: None,
        blend: false,
    };
    assert!(
        verify::check_unused(
            source,
            None,
            &[],
            &c,
            None,
            VerificationLimits::default(),
            || Ok(())
        )
        .is_err()
    );
    assert!(
        verify::check_unused(
            source,
            None,
            &[3],
            &c,
            None,
            VerificationLimits::default(),
            || Ok(())
        )
        .is_err()
    );
}
