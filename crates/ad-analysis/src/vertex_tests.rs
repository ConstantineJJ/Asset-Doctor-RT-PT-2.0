//! ADR-0034: G012 exact duplicate vertices (all decoded streams bitwise equal).
use super::tests::{from_primitives, primitive};
use super::*;
use ad_document::UvChannel;

/// v3 equals v1 on every stream; v4 differs from v0 only in UV; v5 differs
/// from v0 only by -0.0; v6 equals v1 too (second later twin).
fn document() -> AssetDocument {
    let mut mesh = primitive(
        vec![
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [1., 0., 0.],
            [0., 0., 0.],
            [-0., 0., 0.],
            [1., 0., 0.],
        ],
        false,
        vec![0, 1, 2, 3, 2, 4, 5, 2, 6],
    );
    mesh.normals = Some(vec![[0., 0., 1.]; 7]);
    mesh.uv_channels = vec![UvChannel {
        set: 0,
        values: vec![
            [0., 0.],
            [1., 0.],
            [0., 1.],
            [1., 0.],
            [0.5, 0.],
            [0., 0.],
            [1., 0.],
        ],
    }];
    from_primitives(vec![mesh])
}

#[test]
fn g012_counts_later_vertices_equal_on_every_stream() {
    assert_eq!(Code::ALL.len(), 12);
    assert_eq!(ALGORITHM_VERSION, 6);
    let doc = document();
    let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    let finding = report
        .findings
        .iter()
        .find(|f| f.code == Code::DuplicateVertex)
        .expect("G012 finding");
    assert_eq!(finding.count, 2);
    assert_eq!(
        finding.samples,
        Samples::Indices {
            element: Element::Vertex,
            indices: vec![3, 6]
        }
    );
    assert_eq!(finding.severity, Severity::Warning);
    let coverage = report.coverage[Code::DuplicateVertex as usize];
    assert!(coverage.is_complete());
    assert_eq!(coverage.completed_primitives, 1);
    assert!(report.is_complete());
    // The shared ordering is exact: v1 < v3 is a tie broken by ordinal.
    let mesh = &doc.primitives()[0];
    assert!(same_vertex(mesh, 1, 3) && same_vertex(mesh, 1, 6));
    assert!(!same_vertex(mesh, 0, 4) && !same_vertex(mesh, 0, 5));
    assert_eq!(vertex_order(mesh, 1, 3), std::cmp::Ordering::Equal);
}

#[test]
fn g012_scratch_refusal_is_explicit_and_keeps_other_rules() {
    // 20 vertices, one triangle: the shared admission needs 16 + 20 bytes,
    // G012 needs 20 * 4 = 80 bytes of ordinals.
    let positions = (0..20).map(|i| [i as f32, 0., 1.]).collect();
    let doc = from_primitives(vec![primitive(positions, false, vec![0, 1, 2])]);
    let limits = Limits {
        scratch_bytes: 60,
        ..Limits::default()
    };
    let report = analyze(&doc, limits, &Cancellation::default());
    let report = match report {
        Ok(r) => r,
        Err(e) => panic!("analysis refused entirely: {e}"),
    };
    let coverage = report.coverage[Code::DuplicateVertex as usize];
    assert_eq!(coverage.omitted_primitives, 1);
    assert_eq!(coverage.reason, Some(LimitReason::Scratch));
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.code == Code::DuplicateVertex)
    );
    assert!(report.coverage[Code::ZeroArea as usize].is_complete());
}
