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
    from_primitives(vec![primitive(
        positions,
        normals,
        vec![0, 1, 2, 1, 2, 0, 0, 2, 1, 0, 1, 3, 0, 0, 1],
    )])
}

pub(super) fn primitive(
    positions: Vec<[f32; 3]>,
    normals: bool,
    indices: Vec<u32>,
) -> MeshPrimitive {
    let vertex_count = positions.len();
    MeshPrimitive {
        positions,
        normals: normals.then(|| vec![[0.; 3]; vertex_count]),
        tangents: None,
        normal_texture: None,
        normal_scale: 1.,
        occlusion_texture: None,
        occlusion_strength: 1.,
        emissive_factor: [0.; 3],
        emissive_texture: None,
        metallic_roughness: [1.; 2],
        indices,
        uv_channels: Vec::new(),
        uv_bindings: ad_document::UvBindings::default(),
        base_color: [1.; 4],
        colors: None,
        color_texture: None,
        metallic_roughness_texture: None,
        alpha_cutoff: None,
    }
}

pub(super) fn from_primitives(meshes: Vec<MeshPrimitive>) -> AssetDocument {
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
            meshes,
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
    assert_eq!(report.findings.len(), expected.len() + 3); // Edges, components and missing UV0.
    for (finding, (code, count, samples)) in report.findings.iter().zip(expected) {
        assert_eq!(
            (finding.code, finding.count, &finding.samples),
            (
                code,
                count,
                &Samples::Indices {
                    element: if matches!(code, Code::ZeroArea | Code::DuplicateTriangle) {
                        Element::Triangle
                    } else {
                        Element::Vertex
                    },
                    indices: samples
                }
            )
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
        assert_eq!((normals.code, normals.count), (Code::ZeroNormal, 5));
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
    assert_eq!(
        finding.samples,
        Samples::Indices {
            element: Element::Triangle,
            indices: (0..16).collect::<Vec<_>>()
        }
    );
    let doc = document(1., false);
    let mut report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    report.document_id = DocumentId(18);
    assert!(!report.matches(&doc));
    report.document_id = doc.id();
    report.revision += 1;
    assert!(!report.matches(&doc));
}

fn topology(faces: &[&[u32]]) -> AssetDocument {
    from_primitives(
        faces
            .iter()
            .map(|indices| {
                primitive(
                    vec![
                        [0., 0., 0.],
                        [1., 0., 0.],
                        [0., 1., 0.],
                        [0., 0., 1.],
                        [1., 1., 1.],
                    ],
                    false,
                    indices.to_vec(),
                )
            })
            .collect(),
    )
}
fn edge_counts(report: &Report) -> (usize, usize) {
    assert!(report.is_complete());
    let count = |code| {
        report
            .findings
            .iter()
            .filter(|f| f.code == code)
            .map(|f| f.count)
            .sum()
    };
    (count(Code::BoundaryEdge), count(Code::HighIncidenceEdge))
}
#[test]
fn indexed_edge_oracles_keep_source_triangle_incidence_and_primitive_boundaries() {
    for (indices, expected) in [
        (&[0, 1, 2][..], (3, 0)),
        (&[0, 1, 2, 0, 2, 3][..], (4, 0)),
        (&[0, 1, 2, 0, 1, 3, 0, 2, 3, 1, 2, 3][..], (0, 0)),
        (&[0, 1, 2, 0, 1, 3, 0, 1, 4][..], (6, 1)),
        (&[0, 1, 2, 1, 2, 0][..], (0, 0)),
        (&[0, 1, 2, 0, 2, 1][..], (0, 0)),
        (&[0, 1, 2, 1, 2, 0, 0, 2, 1][..], (0, 3)),
    ] {
        let doc = topology(&[indices]);
        let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
        assert_eq!(edge_counts(&report), expected, "{indices:?}");
    }
    // The second primitive is inactive but analyzed independently. Instances do not multiply counts.
    let doc = topology(&[&[0, 1, 2], &[0, 2, 3]]);
    let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    assert_eq!(edge_counts(&report), (6, 0));
    let boundary = report
        .findings
        .iter()
        .find(|f| f.code == Code::BoundaryEdge)
        .unwrap();
    assert_eq!(boundary.severity, Severity::Information);
    assert_eq!(
        boundary.samples,
        Samples::Edges(vec![[0, 1], [0, 2], [1, 2]])
    );
}
#[test]
fn repeated_index_faces_are_excluded_but_distinct_collinear_indices_remain_eligible() {
    let doc = topology(&[&[0, 1, 2, 0, 0, 1]]);
    let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    assert_eq!(edge_counts(&report), (3, 0));
    for code in [Code::BoundaryEdge, Code::HighIncidenceEdge] {
        assert_eq!(report.coverage[code as usize].excluded_triangles, 1);
    }
    let doc = from_primitives(vec![primitive(
        vec![[0., 0., 0.], [1., 0., 0.], [2., 0., 0.]],
        false,
        vec![0, 1, 2],
    )]);
    let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    assert_eq!(edge_counts(&report), (3, 0));
    assert!(report.findings.iter().any(|f| f.code == Code::ZeroArea));
}
#[test]
fn edge_scratch_and_result_refusals_preserve_earlier_core_results() {
    let doc = topology(&[&[0, 1, 2, 0, 1, 3, 0, 1, 4]]);
    let report = analyze(
        &doc,
        Limits {
            scratch_bytes: 100,
            ..Default::default()
        },
        &Cancellation::default(),
    )
    .unwrap();
    assert!(!report.is_complete());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code == Code::MissingNormals)
    );
    assert!(report.coverage[..5].iter().all(Coverage::is_complete));
    assert_eq!(
        report.coverage[Code::DisconnectedComponents as usize].completed_primitives,
        1
    );
    assert!(report.coverage[Code::DisconnectedComponents as usize].is_complete());
    for code in [Code::BoundaryEdge, Code::HighIncidenceEdge] {
        let c = &report.coverage[code as usize];
        assert_eq!(
            (c.completed_primitives, c.omitted_primitives, c.reason),
            (0, 1, Some(LimitReason::Scratch))
        );
        assert_eq!(c.omitted_samples(), &[MeshId(0)]);
    }
    let doc = topology(&[&[0, 1, 2]]);
    let report = analyze(
        &doc,
        Limits {
            findings: 2,
            ..Default::default()
        },
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(report.findings.len(), 2); // Unused vertices and missing normals retained.
    assert_eq!(
        report.coverage[Code::BoundaryEdge as usize].reason,
        Some(LimitReason::Findings)
    );
    assert!(report.coverage[Code::HighIncidenceEdge as usize].is_complete());
}

#[test]
fn separate_coincident_indices_are_not_welded_and_edge_samples_are_bounded() {
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for i in 0..20u32 {
        positions.extend_from_slice(&[[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]);
        indices.extend_from_slice(&[i * 3, i * 3 + 1, i * 3 + 2]);
    }
    let doc = from_primitives(vec![primitive(positions, false, indices)]);
    let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    assert_eq!(edge_counts(&report), (60, 0));
    let f = report
        .findings
        .iter()
        .find(|f| f.code == Code::BoundaryEdge)
        .unwrap();
    let Samples::Edges(pairs) = &f.samples else {
        panic!("Typed edge evidence expected")
    };
    assert_eq!(pairs.len(), 16);
    assert_eq!(pairs[0], [0, 1]);
    assert_eq!(pairs[15], [15, 16]);
    assert!(pairs.windows(2).all(|p| p[0] < p[1]));
}

#[test]
fn edge_scratch_boundary_is_exact_and_later_primitives_survive_a_refusal() {
    let faces = &[0, 1, 2, 0, 1, 3, 0, 1, 4];
    let doc = topology(&[faces]);
    for (scratch, complete) in [(107, false), (108, true)] {
        let report = analyze(
            &doc,
            Limits {
                scratch_bytes: scratch,
                ..Default::default()
            },
            &Cancellation::default(),
        )
        .unwrap();
        assert_eq!(report.is_complete(), complete);
    }
    let doc = topology(&[faces, &[0, 1, 2]]);
    let report = analyze(
        &doc,
        Limits {
            scratch_bytes: 100,
            ..Default::default()
        },
        &Cancellation::default(),
    )
    .unwrap();
    let c = &report.coverage[Code::BoundaryEdge as usize];
    assert_eq!(
        (c.completed_primitives, c.omitted_primitives, c.reason),
        (1, 1, Some(LimitReason::Scratch))
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code == Code::BoundaryEdge && f.mesh == MeshId(1))
    );
}

#[test]
fn omitted_scope_samples_remain_bounded_when_output_budget_is_zero() {
    let faces = vec![&[0, 1, 2][..]; 20];
    let doc = topology(&faces);
    let report = analyze(
        &doc,
        Limits {
            findings: 0,
            ..Default::default()
        },
        &Cancellation::default(),
    )
    .unwrap();
    assert!(report.findings.is_empty());
    assert!(!report.is_complete());
    let c = &report.coverage[Code::BoundaryEdge as usize];
    assert_eq!(c.omitted_primitives, 20);
    assert_eq!(
        c.omitted_samples(),
        &(0..16).map(MeshId).collect::<Vec<_>>()
    );
    assert_eq!(
        report.coverage[Code::HighIncidenceEdge as usize].completed_primitives,
        20
    );
}

#[test]
fn all_repeated_index_faces_have_complete_empty_edge_results_with_exclusions() {
    let doc = topology(&[&[0, 0, 1, 1, 1, 2]]);
    let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    assert_eq!(edge_counts(&report), (0, 0));
    for code in [Code::BoundaryEdge, Code::HighIncidenceEdge] {
        let c = &report.coverage[code as usize];
        assert!(c.is_complete());
        assert_eq!(c.completed_primitives, 1);
        assert_eq!(c.excluded_triangles, 2);
    }
}

#[test]
fn components_connect_through_a_vertex_but_never_through_coincident_positions() {
    let points = vec![
        [0., 0., 0.],
        [1., 0., 0.],
        [0., 1., 0.],
        [1., 1., 0.],
        [0., 0., 1.],
        [0., 0., 0.],
        [1., 0., 0.],
        [0., 1., 0.],
    ];
    let doc = from_primitives(vec![primitive(
        points,
        false,
        vec![0, 1, 2, 2, 3, 4, 5, 6, 7],
    )]);
    let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    let finding = report
        .findings
        .iter()
        .find(|f| f.code == Code::DisconnectedComponents)
        .unwrap();
    assert_eq!(finding.severity, Severity::Information);
    assert_eq!(finding.count, 2);
    assert_eq!(
        finding.samples,
        Samples::Components(vec![
            ComponentSample {
                representative_vertex: 0,
                vertices: 5,
                triangles: 2
            },
            ComponentSample {
                representative_vertex: 5,
                vertices: 3,
                triangles: 1
            },
        ])
    );
    assert!(report.is_complete());
}

#[test]
fn component_budget_refusal_keeps_all_previous_rule_results() {
    let points = (0..20).map(|i| [i as f32, 0., 0.]).collect();
    let doc = from_primitives(vec![primitive(points, false, vec![0, 1, 2])]);
    for (scratch, complete) in [(239, false), (240, true)] {
        let report = analyze(
            &doc,
            Limits {
                scratch_bytes: scratch,
                ..Default::default()
            },
            &Cancellation::default(),
        )
        .unwrap();
        assert_eq!(report.is_complete(), complete);
        assert!(report.coverage[..7].iter().all(Coverage::is_complete));
        let coverage = &report.coverage[Code::DisconnectedComponents as usize];
        assert_eq!(
            coverage.reason,
            if complete {
                None
            } else {
                Some(LimitReason::Scratch)
            }
        );
        assert!(report.findings.iter().any(|f| f.code == Code::ZeroArea));
        assert!(report.findings.iter().any(|f| f.code == Code::BoundaryEdge));
    }
}

#[test]
fn component_representatives_and_sizes_do_not_depend_on_union_root_or_face_order() {
    let expected = Samples::Components(vec![
        ComponentSample {
            representative_vertex: 0,
            vertices: 5,
            triangles: 2,
        },
        ComponentSample {
            representative_vertex: 2,
            vertices: 3,
            triangles: 1,
        },
    ]);
    for indices in [
        vec![3, 4, 5, 5, 1, 0, 6, 7, 2],
        vec![6, 7, 2, 5, 1, 0, 3, 4, 5],
    ] {
        let doc = from_primitives(vec![primitive(
            (0..8).map(|i| [i as f32, 0., 0.]).collect(),
            false,
            indices,
        )]);
        let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
        let finding = report
            .findings
            .iter()
            .find(|f| f.code == Code::DisconnectedComponents)
            .unwrap();
        assert_eq!(finding.samples, expected);
    }
}

#[test]
fn component_counts_ignore_unused_and_excluded_only_vertices_but_keep_collinear_faces() {
    let points = (0..12).map(|i| [i as f32, 0., 0.]).collect();
    let doc = from_primitives(vec![primitive(
        points,
        false,
        vec![0, 1, 2, 1, 2, 0, 0, 2, 1, 3, 4, 5, 6, 6, 7],
    )]);
    let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    let finding = report
        .findings
        .iter()
        .find(|f| f.code == Code::DisconnectedComponents)
        .unwrap();
    assert_eq!(finding.count, 2);
    assert_eq!(
        finding.samples,
        Samples::Components(vec![
            ComponentSample {
                representative_vertex: 0,
                vertices: 3,
                triangles: 3
            },
            ComponentSample {
                representative_vertex: 3,
                vertices: 3,
                triangles: 1
            }
        ])
    );
    assert_eq!(
        report.coverage[Code::DisconnectedComponents as usize].excluded_triangles,
        1
    );
    let unused = report
        .findings
        .iter()
        .find(|f| f.code == Code::UnusedVertex)
        .unwrap();
    assert_eq!(unused.count, 4); //8..11;6/7 are source-referenced but not eligible components.
}

#[test]
fn component_samples_are_bounded_and_result_refusal_preserves_earlier_findings() {
    let points = (0..60).map(|i| [i as f32, 0., 0.]).collect();
    let indices = (0..60).collect();
    let doc = from_primitives(vec![primitive(points, false, indices)]);
    let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    let finding = report
        .findings
        .iter()
        .find(|f| f.code == Code::DisconnectedComponents)
        .unwrap();
    assert_eq!(finding.count, 20);
    let Samples::Components(samples) = &finding.samples else {
        panic!("Typed components expected")
    };
    assert_eq!(samples.len(), 16);
    assert_eq!(samples[15].representative_vertex, 45);
    // Zero-area, missing normals and boundaries precede G008; refuse only its result.
    let partial = analyze(
        &doc,
        Limits {
            findings: 3,
            ..Default::default()
        },
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(partial.findings.len(), 3);
    assert_eq!(
        partial.coverage[Code::DisconnectedComponents as usize].reason,
        Some(LimitReason::Findings)
    );
}

#[test]
fn components_keep_inactive_primitive_scope_and_do_not_multiply_instances() {
    let mesh = || {
        primitive(
            (0..6).map(|i| [i as f32, 0., 0.]).collect(),
            false,
            vec![0, 1, 2, 3, 4, 5],
        )
    };
    let doc = from_primitives(vec![mesh(), mesh()]);
    let report = analyze(&doc, Limits::default(), &Cancellation::default()).unwrap();
    let findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code == Code::DisconnectedComponents)
        .collect();
    assert_eq!(findings.len(), 2);
    assert_eq!((findings[0].mesh, findings[1].mesh), (MeshId(0), MeshId(1)));
    assert!(findings.iter().all(|f| f.count == 2));
}

#[test]
fn all_excluded_components_complete_without_allocating_a_forest() {
    let doc = topology(&[&[0, 0, 1, 1, 1, 2]]);
    let report = analyze(
        &doc,
        Limits {
            scratch_bytes: 37,
            ..Default::default()
        },
        &Cancellation::default(),
    )
    .unwrap();
    assert!(report.is_complete());
    assert_eq!(
        report.coverage[Code::DisconnectedComponents as usize].excluded_triangles,
        2
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.code == Code::DisconnectedComponents)
    );
}
