//! ADR-0037: G004/G005 normals recomputation for one source primitive.
use super::*;
use ad_commands::{HistoryLimits, Session};
use ad_document::{Cancellation, DocumentId, ErrorCode, ImportLimits};
use std::sync::Arc;

fn session() -> Session {
    let document = ad_formats::import_glb(
        Arc::from(include_bytes!("../../../tests/fixtures/Normals_Repair.glb").as_slice()),
        "normals".into(),
        DocumentId(8),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    Session::new(document, HistoryLimits::default(), ImportLimits::default()).unwrap()
}
fn plan(s: &Session, mesh: usize, kind: RepairKind) -> AssetResult<RemovalPlan> {
    plan_normals(
        s.document(),
        s.document().tag(),
        MeshId(mesh),
        kind,
        PlanLimits::default(),
        &Cancellation::default(),
    )
}
const UP: [f32; 3] = [0., 0., 1.];
const TILT: [f32; 3] = [0., 0.6, 0.8];

#[test]
fn missing_normals_are_created_from_area_weighted_faces() {
    let mut s = session();
    let c = Cancellation::default();
    let p = plan(&s, 0, RepairKind::MissingNormals).unwrap();
    assert_eq!(p.kind(), RepairKind::MissingNormals);
    assert_eq!(p.preview().changed_normals, 4);
    let command = p.prepare(&s, VerificationLimits::default(), &c).unwrap();
    s.commit(command, &c).unwrap();
    assert_eq!(
        s.document().primitives()[0].normals.as_deref(),
        Some(&[UP; 4][..])
    );
    assert!(s.document().normals_changed(MeshId(0)));
}

#[test]
fn zero_normals_are_replaced_and_authored_ones_survive() {
    let mut s = session();
    let c = Cancellation::default();
    let p = plan(&s, 1, RepairKind::ZeroNormals).unwrap();
    assert_eq!(p.preview().changed_normals, 2);
    let command = p.prepare(&s, VerificationLimits::default(), &c).unwrap();
    s.commit(command, &c).unwrap();
    assert_eq!(
        s.document().primitives()[1].normals.as_deref(),
        Some(&[TILT, UP, UP, TILT][..])
    );
    // Primitive #0 shares POSITION but is untouched.
    assert!(s.document().primitives()[0].normals.is_none());
}

#[test]
fn unused_zero_normal_vertex_and_wrong_kind_are_refused() {
    let s = session();
    let e = plan(&s, 2, RepairKind::ZeroNormals).err().unwrap();
    assert_eq!(e.code, ErrorCode::Unsupported);
    assert!(e.message.contains("G003"), "{}", e.message);
    // G004 needs a primitive without NORMAL; G005 needs one with it.
    assert!(plan(&s, 1, RepairKind::MissingNormals).is_err());
    assert!(plan(&s, 0, RepairKind::ZeroNormals).is_err());
}

#[test]
fn normals_postcheck_refuses_a_tampered_candidate() {
    let s = session();
    let source = &s.document().primitives()[1];
    let good = vec![TILT, UP, UP, TILT];
    verify::check_normals(source, &good, true, || Ok(())).unwrap();
    for bad in [
        vec![UP, UP, UP, TILT],               // authored v0 changed
        vec![TILT, UP, [0., 0.6, 0.8], TILT], // wrong recomputed value
        vec![TILT, UP, UP],                   // length
    ] {
        assert!(verify::check_normals(source, &bad, true, || Ok(())).is_err());
    }
}
