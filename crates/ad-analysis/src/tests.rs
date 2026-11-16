use super::*;
use ad_document::{DocumentParts, ErrorCode, ImportLimits, MeshPrimitive, Node, NodeId};
use std::sync::Arc;

#[test]
fn adaptive_zero_area_does_not_report_rounded_cross_cancellation_as_collinear() {
    assert!(!zero_area(
        [1e12, 1e12, 0.],
        [1e-12, 0., 0.],
        [0., 1e-12, 0.]
    ));
    for scale in [f32::from_bits(1), 1., f32::MAX / 4.] {
        assert!(!zero_area([0.; 3], [scale, 0., 0.], [0., scale, 0.]));
        assert!(zero_area([0.; 3], [scale, 0., 0.], [2. * scale, 0., 0.]));
    }
}

fn document(scale: f32, normals: bool) -> AssetDocument {
    let positions = vec![
        [0., 0., 0.],
        [scale, 0., 0.],
        [0., scale, 0.],
        [2. * scale, 0., 0.],
        [0., 0., scale],
    ];
    let mesh = MeshPrimitive {
        positions,
        normals: normals.then(|| vec![[0.; 3]; 5]),
        tangents: None,
        normal_texture: None,
        normal_uvs: None,
        normal_scale: 1.,
        occlusion_texture: None,
        occlusion_uvs: None,
        occlusion_strength: 1.,
        emissive_factor: [0.; 3],
        emissive_texture: None,
        emissive_uvs: None,
        metallic_roughness: [1.; 2],
        indices: vec![0, 1, 2, 1, 2, 0, 0, 2, 1, 0, 1, 3, 0, 0, 1],
        base_color: [1.; 4],
        uvs: None,
        colors: None,
        color_texture: None,
        metallic_roughness_texture: None,
        metallic_roughness_uvs: None,
        alpha_cutoff: None,
    };
    let node = || Node {
        name: "instance".into(),
        local: [
            [1., 0., 0., 0.],
            [0., 1., 0., 0.],
            [0., 0., 1., 0.],
            [0., 0., 0., 1.],
        ],
        children: Vec::new(),
        meshes: Arc::from([MeshId(0)]),
    };
    AssetDocument::new(
        DocumentParts {
            id: DocumentId(17),
            name: "authored test".into(),
            source: Arc::from([1, 2, 3]),
            meshes: vec![mesh],
            nodes: vec![node(), node()],
            roots: vec![NodeId(0), NodeId(1)],
            notes: Vec::new(),
        },
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap()
}

#[test]
fn known_source_defects_keep_winding_indices_and_count_shared_instances_once() {
    let doc = document(1., false);
    let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    assert!(report.matches(&doc));
    assert_eq!((report.primitives, report.triangles), (1, 5));
    let expected = [
        (Code::ZeroArea, 2, vec![3, 4]),
        (Code::DuplicateTriangle, 1, vec![1]),
        (Code::UnusedVertex, 1, vec![4]),
        (Code::MissingNormals, 5, vec![0, 1, 2, 3, 4]),
    ];
    assert_eq!(report.findings.len(), expected.len());
    for (finding, (code, count, samples)) in report.findings.iter().zip(expected) {
        assert_eq!(
            (finding.code, finding.count, &finding.samples),
            (code, count, &samples)
        );
        assert_eq!(finding.mesh, MeshId(0));
        assert_eq!(
            finding.severity,
            if code == Code::MissingNormals {
                Severity::Information
            } else {
                Severity::Warning
            }
        );
    }
    assert_eq!(&**doc.source(), &[1, 2, 3]);
    assert_eq!(
        doc.primitives()[0].indices,
        [0, 1, 2, 1, 2, 0, 0, 2, 1, 0, 1, 3, 0, 0, 1]
    );
}

#[test]
fn exact_area_handles_tiny_large_scales_and_zero_normals_without_heuristic() {
    for scale in [1e-30, 1., 1e30] {
        let doc = document(scale, true);
        let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
        assert_eq!(
            report
                .findings
                .iter()
                .find(|f| f.code == Code::ZeroArea)
                .unwrap()
                .count,
            2
        );
        let normals = report
            .findings
            .iter()
            .find(|f| f.code == Code::ZeroNormal)
            .unwrap();
        assert_eq!((normals.element, normals.count), (Element::Vertex, 5));
    }
}

#[test]
fn cancellation_and_each_budget_refuse_without_partial_report() {
    let doc = document(1., false);
    let cancel = Cancellation::default();
    cancel.cancel();
    assert!(
        matches!(analyze(&doc,Limits::default(),&cancel),Err(e) if e.code==ErrorCode::Cancelled)
    );
    for limits in [
        Limits {
            triangles: 4,
            ..Limits::default()
        },
        Limits {
            scratch_bytes: 84,
            ..Limits::default()
        },
    ] {
        assert!(
            matches!(analyze(&doc,limits,&Cancellation::default()),Err(e) if e.code==ErrorCode::ResourceLimit)
        );
    }
}

#[test]
fn finding_limit_retains_completed_checks_and_marks_only_missing_results_incomplete() {
    let doc = document(1., false);
    let report = analyze(
        &doc,
        Limits {
            findings: 3,
            ..Limits::default()
        },
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(report.findings.len(), 3);
    assert!(!report.is_complete());
    let missing = report
        .coverage
        .iter()
        .find(|c| c.code == Code::MissingNormals)
        .unwrap();
    assert_eq!(
        (
            missing.completed_primitives,
            missing.omitted_primitives,
            missing.reason
        ),
        (0, 1, Some(LimitReason::Findings))
    );
    let zero = report
        .coverage
        .iter()
        .find(|c| c.code == Code::ZeroNormal)
        .unwrap();
    assert_eq!(missing.omitted_samples(), &[MeshId(0)]);
    assert_eq!(
        (
            zero.completed_primitives,
            zero.omitted_primitives,
            zero.reason
        ),
        (1, 0, None)
    );
    assert_eq!(report.findings[0].code, Code::ZeroArea);
}

#[test]
fn samples_keep_lowest_sixteen_and_stale_reports_are_explicit() {
    let mut tally = Tally::default();
    for index in (0..100).rev() {
        tally.observe(index);
    }
    let finding = tally
        .finding(MeshId(0), Code::ZeroArea, Element::Triangle)
        .unwrap();
    assert_eq!(finding.count, 100);
    assert_eq!(finding.samples, (0..16).collect::<Vec<_>>());
    let doc = document(1., false);
    let mut report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    report.document_id = DocumentId(18);
    assert!(!report.matches(&doc));
    report.document_id = doc.id();
    report.revision += 1;
    assert!(!report.matches(&doc));
}
