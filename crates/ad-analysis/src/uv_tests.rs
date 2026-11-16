use super::*;
use crate::tests::{from_primitives, primitive};
use ad_document::{MeshPrimitive, UvChannel};

fn mesh(channels: &[(u32, Vec<[f32; 2]>)]) -> MeshPrimitive {
    let mut mesh = primitive(
        vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        false,
        vec![0, 1, 2],
    );
    mesh.normals = Some(vec![[0., 0., 1.]; 3]);
    mesh.uv_channels = channels
        .iter()
        .map(|(set, values)| UvChannel {
            set: *set,
            values: values.clone(),
        })
        .collect();
    mesh
}

#[test]
fn missing_uv0_is_not_inferred_from_a_material_selected_uv2() {
    let mut mesh = mesh(&[(2, vec![[0., 0.], [1., 0.], [0., 1.]])]);
    mesh.uv_bindings.color = Some(2);
    let doc = from_primitives(vec![mesh]);
    let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    let missing = report.findings.iter().find(|f| f.code == Code::MissingUv0);
    assert!(
        missing.is_some(),
        "source UV0 absence must be explicit even when UV2 is selected"
    );
    assert_eq!(missing.unwrap().count, 1);
    assert_eq!(missing.unwrap().samples, Samples::MissingUv { set: 0 });
    assert_eq!(&**doc.source(), &[1, 2, 3]);
}

#[test]
fn no_channels_are_not_applicable_and_do_not_invent_zero_finding_scans() {
    let doc = from_primitives(vec![mesh(&[])]);
    let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    assert!(report.is_complete());
    assert_eq!(report.coverage.len(), 11);
    for code in [Code::ZeroUvArea, Code::OutsideUnitUv] {
        let c = report.coverage[code as usize];
        assert_eq!(
            (
                c.completed_primitives,
                c.not_applicable_primitives,
                c.omitted_primitives
            ),
            (0, 1, 0)
        );
        assert!(!report.findings.iter().any(|f| f.code == code));
    }
    let limited = analyze(
        &doc,
        Limits {
            findings: 0,
            ..Default::default()
        },
        &Cancellation::default(),
    )
    .unwrap();
    assert!(!limited.is_complete());
    assert_eq!(
        limited.coverage[Code::MissingUv0 as usize].reason,
        Some(LimitReason::Findings)
    );
    assert_eq!(
        limited.coverage[Code::ZeroUvArea as usize].not_applicable_primitives,
        1
    );
}

#[test]
fn zero_uv_area_is_exact_at_tiny_and_large_scales_and_independent_of_winding() {
    for scale in [f32::from_bits(1), 1e-30, 1., f32::MAX / 4.] {
        for values in [
            vec![[0., 0.], [scale, 0.], [0., scale]],
            vec![[0., scale], [scale, 0.], [0., 0.]],
        ] {
            let doc = from_primitives(vec![mesh(&[(0, values)])]);
            let r = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
            assert!(!r.findings.iter().any(|f| f.code == Code::ZeroUvArea));
        }
        let doc = from_primitives(vec![mesh(&[(
            0,
            vec![[0., 0.], [scale, 0.], [2. * scale, 0.]],
        )])]);
        let r = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
        assert_eq!(
            r.findings
                .iter()
                .find(|f| f.code == Code::ZeroUvArea)
                .unwrap()
                .count,
            1
        );
    }
    let doc = from_primitives(vec![mesh(&[(
        0,
        vec![[1e12, 1e12], [1e-12, 0.], [0., 1e-12]],
    )])]);
    assert!(
        !analyze(&doc, Limits::default(), &Cancellation::default())
            .unwrap()
            .findings
            .iter()
            .any(|f| f.code == Code::ZeroUvArea)
    );
}

#[test]
fn all_source_channels_and_unused_uv_vertices_are_examined_once_with_typed_samples() {
    let mut m = mesh(&[
        (0, vec![[0., 0.]; 3]),
        (2, vec![[0., 0.], [1., 0.], [0., 1.]]),
    ]);
    m.uv_bindings.color = Some(2);
    m.positions.push([8., 8., 8.]);
    m.normals.as_mut().unwrap().push([0., 0., 1.]);
    m.uv_channels[0].values.push([1., 0.]);
    m.uv_channels[1].values.push([-0.01, 1.01]);
    m.indices.extend([0, 0, 1]);
    let doc = from_primitives(vec![m]);
    let r = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    assert!(r.is_complete());
    assert_eq!(r.coverage[Code::ZeroUvArea as usize].excluded_triangles, 1);
    let zero = r
        .findings
        .iter()
        .find(|f| f.code == Code::ZeroUvArea)
        .unwrap();
    assert_eq!((zero.count, zero.severity), (1, Severity::Warning));
    assert_eq!(
        zero.samples,
        Samples::Uv {
            set: 0,
            element: Element::Triangle,
            indices: vec![0]
        }
    );
    let outside = r
        .findings
        .iter()
        .find(|f| f.code == Code::OutsideUnitUv)
        .unwrap();
    assert_eq!(
        (outside.count, outside.severity),
        (1, Severity::Information)
    );
    assert_eq!(
        outside.samples,
        Samples::Uv {
            set: 2,
            element: Element::Vertex,
            indices: vec![3]
        }
    );
    assert!(!r.findings.iter().any(|f| f.code == Code::MissingUv0));
    assert_eq!(r.primitives, 1); // Two visible instances never multiply source counts.
    assert_eq!(doc.primitives()[0].source_uv(2).unwrap()[3], [-0.01, 1.01]);
}

#[test]
fn collapsed_uvs_are_checked_even_when_geometry_is_collinear_but_not_when_indices_repeat() {
    let mut m = mesh(&[(0, vec![[0.5, 0.5]; 3])]);
    m.positions[2] = [2., 0., 0.];
    m.indices.extend([0, 0, 1]);
    let r = analyze(
        &from_primitives(vec![m]),
        Limits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    let f = r
        .findings
        .iter()
        .find(|f| f.code == Code::ZeroUvArea)
        .unwrap();
    assert_eq!(f.count, 1);
    assert_eq!(r.coverage[Code::ZeroUvArea as usize].excluded_triangles, 1);
}

#[test]
fn uv_samples_are_lowest_sixteen_and_boundary_values_are_allowed() {
    let mut m = mesh(&[(0, vec![[-1., 2.]; 3])]);
    for i in 3..40 {
        m.positions.push([i as f32, 0., 0.]);
        m.normals.as_mut().unwrap().push([0., 0., 1.]);
        m.uv_channels[0].values.push([-1., 2.]);
    }
    m.indices = std::iter::repeat_n([0, 1, 2], 40).flatten().collect();
    let r = analyze(
        &from_primitives(vec![m]),
        Limits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    for (code, element) in [
        (Code::ZeroUvArea, Element::Triangle),
        (Code::OutsideUnitUv, Element::Vertex),
    ] {
        let f = r.findings.iter().find(|f| f.code == code).unwrap();
        assert_eq!(f.count, 40);
        assert_eq!(
            f.samples,
            Samples::Uv {
                set: 0,
                element,
                indices: (0..16).collect()
            }
        );
    }
    let r = analyze(
        &from_primitives(vec![mesh(&[(0, vec![[-0., 1.], [1., 0.], [0., 0.]])])]),
        Limits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    assert!(!r.findings.iter().any(|f| f.code == Code::OutsideUnitUv));
}

#[test]
fn eight_channel_output_is_atomic_per_primitive_rule_and_earlier_results_survive() {
    let channels = (0..8)
        .map(|set| (set, vec![[-1., 2.]; 3]))
        .collect::<Vec<_>>();
    let doc = from_primitives(vec![mesh(&channels)]);
    let full = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    let before = full
        .findings
        .iter()
        .filter(|f| (f.code as usize) < 9)
        .count();
    assert_eq!(
        full.findings
            .iter()
            .filter(|f| f.code == Code::ZeroUvArea)
            .count(),
        8
    );
    assert_eq!(
        full.findings
            .iter()
            .filter(|f| f.code == Code::OutsideUnitUv)
            .count(),
        8
    );
    let partial = analyze(
        &doc,
        Limits {
            findings: before + 7,
            ..Default::default()
        },
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(partial.findings.len(), before);
    assert!(!partial.is_complete());
    for code in [Code::ZeroUvArea, Code::OutsideUnitUv] {
        let c = partial.coverage[code as usize];
        assert_eq!(
            (c.completed_primitives, c.omitted_primitives, c.reason),
            (0, 1, Some(LimitReason::Findings))
        );
    }
    let exact = analyze(
        &doc,
        Limits {
            findings: before + 8,
            ..Default::default()
        },
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(
        exact.coverage[Code::ZeroUvArea as usize].completed_primitives,
        1
    );
    assert_eq!(
        exact
            .findings
            .iter()
            .filter(|f| f.code == Code::ZeroUvArea)
            .count(),
        8
    );
    assert_eq!(
        exact.coverage[Code::OutsideUnitUv as usize].omitted_primitives,
        1
    );
}

#[test]
fn inactive_primitives_keep_per_rule_checked_and_not_applicable_coverage() {
    let doc = from_primitives(vec![
        mesh(&[(0, vec![[0., 0.], [1., 0.], [0., 1.]])]),
        mesh(&[]),
    ]);
    let r = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    assert_eq!(r.primitives, 2);
    assert!(r.is_complete());
    assert_eq!(
        r.findings
            .iter()
            .filter(|f| f.code == Code::MissingUv0)
            .map(|f| f.mesh)
            .collect::<Vec<_>>(),
        [MeshId(1)]
    );
    let c = r.coverage[Code::ZeroUvArea as usize];
    assert_eq!(
        (c.completed_primitives, c.not_applicable_primitives),
        (1, 1)
    );
}

#[test]
fn failed_channel_group_allocation_retains_no_fragment_and_cancel_returns_no_result() {
    let doc = from_primitives(vec![mesh(&[])]);
    let mut report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    let old = report.findings.len();
    report.retain_many(Code::ZeroUvArea, MeshId(0), 2, Limits::default(), || {
        Err(budget())
    });
    assert_eq!(report.findings.len(), old);
    assert_eq!(
        report.coverage[Code::ZeroUvArea as usize].reason,
        Some(LimitReason::Allocation)
    );
    let cancel = Cancellation::default();
    cancel.cancel();
    assert!(
        matches!(uv::analyze(&doc,&mut report,Limits::default(),&cancel),Err(e) if e.code==ErrorCode::Cancelled)
    );
    assert!(
        matches!(analyze(&doc,Limits::default(),&cancel),Err(e) if e.code==ErrorCode::Cancelled)
    );
}

#[test]
fn a_refused_channel_group_does_not_skip_later_clean_or_not_applicable_primitives() {
    let bad = (0..8)
        .map(|set| (set, vec![[-1., 2.]; 3]))
        .collect::<Vec<_>>();
    let good = [(0, vec![[0., 0.], [1., 0.], [0., 1.]])];
    let doc = from_primitives(vec![mesh(&bad), mesh(&good), mesh(&[])]);
    let full = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    let core = full
        .findings
        .iter()
        .filter(|f| (f.code as usize) < 8)
        .count();
    let r = analyze(
        &doc,
        Limits {
            findings: core + 7,
            ..Default::default()
        },
        &Cancellation::default(),
    )
    .unwrap();
    for code in [Code::ZeroUvArea, Code::OutsideUnitUv] {
        let c = r.coverage[code as usize];
        assert_eq!(
            (
                c.completed_primitives,
                c.not_applicable_primitives,
                c.omitted_primitives
            ),
            (1, 1, 1)
        );
        assert_eq!(c.omitted_samples(), [MeshId(0)]);
    }
    assert!(
        r.findings
            .iter()
            .any(|f| f.code == Code::MissingUv0 && f.mesh == MeshId(2))
    );
    assert_eq!(
        r.coverage[Code::MissingUv0 as usize].completed_primitives,
        3
    );
}
