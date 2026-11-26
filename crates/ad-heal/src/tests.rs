use super::*;
use ad_commands::{HistoryLimits, Session};
use ad_document::{
    AssetDocument, DocumentParts, ErrorCode, MeshPrimitive, Node, NodeId, UvBindings,
};
use ad_document::{Cancellation, DocumentId, ImportLimits, MeshId};
use std::sync::Arc;

fn fixture() -> ad_document::AssetDocument {
    ad_formats::import_glb(
        Arc::from(include_bytes!("../../../tests/fixtures/Geometry_Findings.glb").as_slice()),
        "Heal".into(),
        DocumentId(1),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap()
}
fn session(document: ad_document::AssetDocument) -> Session {
    Session::new(document, HistoryLimits::default(), ImportLimits::default()).unwrap()
}

fn domain(parts: Vec<Vec<[u32; 3]>>) -> AssetDocument {
    let meshes = parts
        .into_iter()
        .map(|faces| MeshPrimitive {
            positions: vec![
                [0., 0., 0.],
                [1., 0., 0.],
                [0., 1., 0.],
                [0., 0., 0.],
                [1., 0., 0.],
                [0., 1., 0.],
            ],
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
            blend: false,
        })
        .collect::<Vec<_>>();
    let ids: Arc<[MeshId]> = Arc::from([MeshId(0)]);
    let local = [
        [1., 0., 0., 0.],
        [0., 1., 0., 0.],
        [0., 0., 1., 0.],
        [0., 0., 0., 1.],
    ];
    let other = if meshes.len() > 1 {
        MeshId(1)
    } else {
        MeshId(0)
    };
    AssetDocument::new(
        DocumentParts {
            id: DocumentId(1),
            name: "domain".into(),
            source: Arc::from(b"preserved domain source".as_slice()),
            meshes,
            nodes: vec![
                Node {
                    name: "Root".into(),
                    local,
                    children: vec![NodeId(1)],
                    meshes: ids.clone(),
                },
                Node {
                    name: "Shared".into(),
                    local,
                    children: Vec::new(),
                    meshes: ids,
                },
                Node {
                    name: "Inactive".into(),
                    local,
                    children: Vec::new(),
                    meshes: Arc::from([other]),
                },
            ],
            roots: vec![NodeId(0)],
            notes: Vec::new(),
        },
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap()
}

#[test]
fn fixture_plan_prepares_without_mutation_and_uses_shared_history() {
    let c = Cancellation::default();
    let mut s = session(fixture());
    let source = s.document().source().clone();
    let positions = s.document().primitives()[0].positions.as_ptr();
    let original = s.document().primitives()[0].indices.clone();
    let plan = plan_duplicates(
        s.document(),
        s.document().tag(),
        MeshId(0),
        Default::default(),
        &c,
    )
    .unwrap();
    assert_eq!(plan.removed_triangles(), [1]);
    assert_eq!(plan.preview().before_triangles, 5);
    assert_eq!(plan.preview().after_triangles, 4);
    let command = plan.prepare(&s, Default::default(), &c).unwrap();
    assert_eq!(s.document().primitives()[0].indices, original);
    assert!(!s.can_undo());
    s.commit(command, &c).unwrap();
    let analysis = ad_analysis::analyze(s.document(), Default::default(), &c).unwrap();
    assert!(analysis.is_complete());
    assert!(
        !analysis
            .findings
            .iter()
            .any(|f| f.mesh == MeshId(0) && f.code == ad_analysis::Code::DuplicateTriangle)
    );
    let proof = ad_formats::prepare_glb(
        s.document(),
        s.document().tag(),
        Default::default(),
        Default::default(),
        &c,
    )
    .unwrap();
    assert!(proof.is_current(s.document()));
    assert!(Arc::ptr_eq(s.document().source(), &source));
    assert_eq!(s.document().primitives()[0].positions.as_ptr(), positions);
    s.undo(s.document().tag(), &c).unwrap();
    assert_eq!(s.document().primitives()[0].indices, original);
    assert!(!plan.is_current(s.document()));
    s.redo(s.document().tag(), &c).unwrap();
    assert_eq!(s.document().primitives()[0].indices.len(), 12);
}

#[test]
fn full_removal_set_exceeds_report_samples_and_keeps_rotations_winding_and_indices_distinct() {
    let mut faces = vec![[1, 2, 0]; 41];
    faces[3] = [2, 0, 1];
    faces.extend([[0, 2, 1], [3, 4, 5]]);
    let s = session(domain(vec![faces]));
    let c = Cancellation::default();
    let report = ad_analysis::analyze(s.document(), Default::default(), &c).unwrap();
    assert!(report.is_complete());
    let finding = report
        .findings
        .iter()
        .find(|f| f.code == ad_analysis::Code::DuplicateTriangle)
        .unwrap();
    assert_eq!(finding.count, 40);
    assert!(
        matches!(&finding.samples,ad_analysis::Samples::Indices{indices,..} if indices.len()==16)
    );
    let plan = plan_duplicates(
        s.document(),
        s.document().tag(),
        MeshId(0),
        Default::default(),
        &c,
    )
    .unwrap();
    assert_eq!(plan.removed_triangles(), (1..41).collect::<Vec<_>>());
    assert_eq!(plan.preview().after_triangles, 3);
    assert_eq!(plan.preview().source_node_references, 3);
    assert_eq!(
        plan.prepare(&s, Default::default(), &c)
            .unwrap()
            .indices_after(s.document(), &c)
            .unwrap(),
        [1, 2, 0, 0, 2, 1, 3, 4, 5]
    );
}

#[test]
fn repeated_index_rotations_keep_one_degenerate_representative() {
    let s = session(domain(vec![vec![
        [0, 0, 1],
        [0, 1, 0],
        [1, 0, 0],
        [0, 1, 1],
        [1, 0, 1],
    ]]));
    let c = Cancellation::default();
    let plan = plan_duplicates(
        s.document(),
        s.document().tag(),
        MeshId(0),
        Default::default(),
        &c,
    )
    .unwrap();
    assert_eq!(plan.removed_triangles(), [1, 2, 4]);
    assert_eq!(
        plan.prepare(&s, Default::default(), &c)
            .unwrap()
            .indices_after(s.document(), &c)
            .unwrap(),
        [0, 0, 1, 0, 1, 1]
    );
}

#[test]
fn primitive_scope_includes_inactive_and_zero_reference_sources_without_cross_merging() {
    let mut s = session(domain(vec![
        vec![[0, 1, 2], [0, 1, 2]],
        vec![[0, 1, 2]],
        vec![[0, 1, 2], [1, 2, 0]],
    ]));
    let c = Cancellation::default();
    let first = plan_duplicates(
        s.document(),
        s.document().tag(),
        MeshId(0),
        Default::default(),
        &c,
    )
    .unwrap();
    let inactive = plan_duplicates(
        s.document(),
        s.document().tag(),
        MeshId(1),
        Default::default(),
        &c,
    )
    .unwrap();
    let unreferenced = plan_duplicates(
        s.document(),
        s.document().tag(),
        MeshId(2),
        Default::default(),
        &c,
    )
    .unwrap();
    assert_eq!(first.preview().source_node_references, 2);
    assert_eq!(inactive.preview().source_node_references, 1);
    assert!(inactive.removed_triangles().is_empty());
    assert_eq!(unreferenced.preview().source_node_references, 0);
    s.commit(
        unreferenced.prepare(&s, Default::default(), &c).unwrap(),
        &c,
    )
    .unwrap();
    assert_eq!(s.document().primitives()[0].indices.len(), 6);
    assert_eq!(s.document().primitives()[1].indices.len(), 3);
    assert_eq!(s.document().primitives()[2].indices.len(), 3);
}

#[test]
fn planner_and_verification_limits_have_exact_boundaries() {
    let s = session(domain(vec![vec![[0, 1, 2]; 4]]));
    let c = Cancellation::default();
    let limits = PlanLimits {
        triangles: 4,
        scratch_bytes: 64,
        result_bytes: 12,
    };
    let plan = plan_duplicates(s.document(), s.document().tag(), MeshId(0), limits, &c).unwrap();
    assert_eq!(plan.retained_bytes(), 12);
    for smaller in [
        PlanLimits {
            triangles: 3,
            ..limits
        },
        PlanLimits {
            scratch_bytes: 63,
            ..limits
        },
        PlanLimits {
            result_bytes: 11,
            ..limits
        },
    ] {
        assert!(
            matches!(plan_duplicates(s.document(),s.document().tag(),MeshId(0),smaller,&c),Err(e) if e.code==ErrorCode::ResourceLimit)
        );
    }
    plan.prepare(&s, VerificationLimits { scratch_bytes: 76 }, &c)
        .unwrap();
    assert!(
        matches!(plan.prepare(&s,VerificationLimits{scratch_bytes:75},&c),Err(e) if e.code==ErrorCode::ResourceLimit)
    );
    assert!(!s.can_undo());
    assert!(!s.is_modified());
}

#[test]
fn invalid_cancelled_stale_and_foreign_plans_never_mutate_history() {
    let c = Cancellation::default();
    let mut s = session(fixture());
    let foreign = session(fixture());
    assert!(
        matches!(plan_duplicates(s.document(),s.document().tag(),MeshId(99),Default::default(),&c),Err(e) if e.code==ErrorCode::InvalidGeometry)
    );
    let plan = plan_duplicates(
        s.document(),
        s.document().tag(),
        MeshId(0),
        Default::default(),
        &c,
    )
    .unwrap();
    assert_eq!(s.document().tag(), foreign.document().tag());
    assert!(
        matches!(plan.prepare(&foreign,Default::default(),&c),Err(e) if e.code==ErrorCode::Conflict)
    );
    let cancelled = Cancellation::default();
    cancelled.cancel();
    assert!(
        matches!(plan.prepare(&s,Default::default(),&cancelled),Err(e) if e.code==ErrorCode::Cancelled)
    );
    assert!(
        matches!(plan_duplicates(s.document(),s.document().tag(),MeshId(0),Default::default(),&cancelled),Err(e) if e.code==ErrorCode::Cancelled)
    );
    s.commit(plan.prepare(&s, Default::default(), &c).unwrap(), &c)
        .unwrap();
    s.undo(s.document().tag(), &c).unwrap();
    assert!(matches!(plan.prepare(&s,Default::default(),&c),Err(e) if e.code==ErrorCode::Conflict));
    assert!(
        matches!(plan_duplicates(s.document(),plan.preview().tag,MeshId(0),Default::default(),&c),Err(e) if e.code==ErrorCode::Conflict)
    );
    assert!(s.can_redo());
    assert!(!s.is_modified());
}

#[test]
fn refusals_preserve_redo_and_a_full_history_accepts_noop() {
    let c = Cancellation::default();
    let mut s = Session::new(
        domain(vec![vec![[0, 1, 2], [1, 2, 0]], vec![[0, 1, 2], [1, 2, 0]]]),
        HistoryLimits {
            entries: 1,
            ..Default::default()
        },
        Default::default(),
    )
    .unwrap();
    let plan = plan_duplicates(
        s.document(),
        s.document().tag(),
        MeshId(0),
        Default::default(),
        &c,
    )
    .unwrap();
    s.commit(plan.prepare(&s, Default::default(), &c).unwrap(), &c)
        .unwrap();
    let noop = plan_duplicates(
        s.document(),
        s.document().tag(),
        MeshId(0),
        Default::default(),
        &c,
    )
    .unwrap();
    let usage = s.history_usage();
    let outcome = s
        .commit(
            noop.prepare(&s, VerificationLimits { scratch_bytes: 0 }, &c)
                .unwrap(),
            &c,
        )
        .unwrap();
    assert!(!outcome.changed);
    assert_eq!(usage, s.history_usage());
    let pending = plan_duplicates(
        s.document(),
        s.document().tag(),
        MeshId(1),
        Default::default(),
        &c,
    )
    .unwrap();
    assert!(
        matches!(pending.prepare(&s,Default::default(),&c),Err(e) if e.code==ErrorCode::ResourceLimit)
    );
    assert_eq!(usage, s.history_usage());
    s.undo(s.document().tag(), &c).unwrap();
    let usage = s.history_usage();
    let plan = plan_duplicates(
        s.document(),
        s.document().tag(),
        MeshId(0),
        Default::default(),
        &c,
    )
    .unwrap();
    assert!(
        matches!(plan.prepare(&s,VerificationLimits{scratch_bytes:0},&c),Err(e) if e.code==ErrorCode::ResourceLimit)
    );
    assert_eq!(usage, s.history_usage());
    assert!(s.can_redo());
}

#[test]
fn cancellation_inside_dense_planning_and_verification_discards_partial_work() {
    let before = [0, 1, 2].repeat(4097);
    let limits = PlanLimits::default();
    let mut polls = 0;
    let result = scan(&before, limits, || {
        polls += 1;
        if polls == 10 {
            Err(ad_document::AssetError::new(
                ErrorCode::Cancelled,
                "mid-group",
            ))
        } else {
            Ok(())
        }
    });
    assert!(matches!(result,Err(e) if e.code==ErrorCode::Cancelled));
    assert_eq!(polls, 10);
    let removed = (1..4097).collect::<Vec<u32>>();
    polls = 0;
    let result = verify::check(&before, &removed, &[0, 1, 2], Default::default(), || {
        polls += 1;
        if polls == 21 {
            Err(ad_document::AssetError::new(
                ErrorCode::Cancelled,
                "mid-postcheck-group",
            ))
        } else {
            Ok(())
        }
    });
    assert!(matches!(result,Err(e) if e.code==ErrorCode::Cancelled));
    assert_eq!(polls, 21);
}

#[test]
fn independent_postcheck_rejects_wrong_survivor_missing_group_and_altered_faces() {
    let before = [0, 1, 2, 0, 1, 2, 0, 2, 1, 3, 4, 5];
    for (removed, after) in [
        (vec![0], vec![0, 1, 2, 0, 2, 1, 3, 4, 5]), // same bytes, wrong earliest ordinal
        (vec![2], vec![0, 1, 2, 0, 1, 2, 3, 4, 5]), // distinct winding removed; duplicate remains
        (vec![1], vec![0, 1, 2, 0, 2, 1, 3, 5, 4]), // changed surviving winding
        (vec![1, 3], vec![0, 1, 2, 0, 2, 1]),       // distinct source group omitted
        (vec![], before.to_vec()),                  // independent array still contains a duplicate
        (vec![9], vec![0, 1, 2, 0, 2, 1, 3, 4, 5]), // out of range
    ] {
        assert!(
            matches!(verify::check(&before,&removed,&after,Default::default(),||Ok(())),Err(e) if e.code==ErrorCode::VerificationFailed)
        );
    }
    verify::check(
        &before,
        &[1],
        &[0, 1, 2, 0, 2, 1, 3, 4, 5],
        Default::default(),
        || Ok(()),
    )
    .unwrap();
}

#[test]
fn deterministic_group_oracle_covers_interleaved_rotations_without_using_planner_keys() {
    // Explicit equivalence classes, independent of either production key function.
    let faces = [
        ([0, 1, 2], 0),
        ([1, 2, 0], 0),
        ([2, 0, 1], 0),
        ([0, 2, 1], 1),
        ([2, 1, 0], 1),
        ([1, 0, 2], 1),
        ([3, 4, 5], 2),
        ([0, 0, 1], 3),
        ([0, 1, 0], 3),
        ([1, 0, 0], 3),
    ];
    let c = Cancellation::default();
    let mut state = 714u32;
    for _ in 0..256 {
        let mut source = Vec::new();
        let mut expected = Vec::new();
        let mut survivors = Vec::new();
        let mut seen = [false; 4];
        for i in 0..32 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let (face, group) = faces[(state as usize >> 8) % faces.len()];
            source.push(face);
            if seen[group] {
                expected.push(i);
            } else {
                seen[group] = true;
                survivors.extend(face);
            }
        }
        let s = session(domain(vec![source]));
        let plan = plan_duplicates(
            s.document(),
            s.document().tag(),
            MeshId(0),
            Default::default(),
            &c,
        )
        .unwrap();
        assert_eq!(plan.removed_triangles(), expected);
        assert_eq!(
            plan.prepare(&s, Default::default(), &c)
                .unwrap()
                .indices_after(s.document(), &c)
                .unwrap(),
            survivors
        );
    }
}

#[test]
fn reference_descriptors_share_the_planner_scratch_budget_as_a_separate_phase() {
    let d = domain(vec![vec![[0, 1, 2], [1, 2, 0]]]);
    let c = Cancellation::default();
    // Two face records use32bytes; three borrowed list descriptors need48.
    let limits = PlanLimits {
        scratch_bytes: 48,
        ..Default::default()
    };
    let p = plan_duplicates(&d, d.tag(), MeshId(0), limits, &c).unwrap();
    assert_eq!(p.preview().source_node_references, 3);
    assert!(
        matches!(plan_duplicates(&d,d.tag(),MeshId(0),PlanLimits{scratch_bytes:47,..limits},&c),Err(e) if e.code==ErrorCode::ResourceLimit)
    );
}

#[test]
fn textured_heal_keeps_authored_attributes_maps_and_roundtrips_before_and_after_undo() {
    let c = Cancellation::default();
    let limits = ImportLimits::default();
    let d = ad_formats::import_glb(
        Arc::from(include_bytes!("../../../tests/fixtures/Duplicate_Heal.glb").as_slice()),
        "textured".into(),
        DocumentId(3),
        limits,
        &c,
    )
    .unwrap();
    let mut s = session(d);
    let source = s.document().source().clone();
    let p = &s.document().primitives()[0];
    let original = p.indices.clone();
    assert!(p.normal_texture.is_some());
    assert!(p.normals.is_some());
    assert!(p.tangents.is_some());
    let positions = p.positions.as_ptr();
    let normals = p.normals.as_ref().unwrap().as_ptr();
    let tangents = p.tangents.as_ref().unwrap().as_ptr();
    let texture = p.normal_texture.clone().unwrap();
    let report = ad_analysis::analyze(s.document(), Default::default(), &c).unwrap();
    assert!(report.is_complete());
    assert_eq!(
        report
            .findings
            .iter()
            .find(|f| f.mesh == MeshId(0) && f.code == ad_analysis::Code::DuplicateTriangle)
            .unwrap()
            .count,
        32
    );
    let plan = plan_duplicates(
        s.document(),
        s.document().tag(),
        MeshId(0),
        Default::default(),
        &c,
    )
    .unwrap();
    assert_eq!(plan.removed_triangles().len(), 32);
    s.commit(plan.prepare(&s, Default::default(), &c).unwrap(), &c)
        .unwrap();
    let p = &s.document().primitives()[0];
    assert_eq!(p.positions.as_ptr(), positions);
    assert_eq!(p.normals.as_ref().unwrap().as_ptr(), normals);
    assert_eq!(p.tangents.as_ref().unwrap().as_ptr(), tangents);
    assert!(Arc::ptr_eq(p.normal_texture.as_ref().unwrap(), &texture));
    assert_eq!(p.indices.len() + 96, original.len());
    let report = ad_analysis::analyze(s.document(), Default::default(), &c).unwrap();
    assert!(report.is_complete());
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.mesh == MeshId(0) && f.code == ad_analysis::Code::DuplicateTriangle)
    );
    ad_formats::prepare_glb(
        s.document(),
        s.document().tag(),
        Default::default(),
        limits,
        &c,
    )
    .unwrap();
    s.undo(s.document().tag(), &c).unwrap();
    assert_eq!(s.document().primitives()[0].indices, original);
    let undo = ad_formats::prepare_glb(
        s.document(),
        s.document().tag(),
        Default::default(),
        limits,
        &c,
    )
    .unwrap();
    assert!(Arc::ptr_eq(undo.bytes(), &source));
    s.redo(s.document().tag(), &c).unwrap();
    ad_formats::prepare_glb(
        s.document(),
        s.document().tag(),
        Default::default(),
        limits,
        &c,
    )
    .unwrap();
}
