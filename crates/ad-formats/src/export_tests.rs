use super::*;

fn fixture(bytes: &[u8]) -> AssetDocument {
    import_glb(
        Arc::from(bytes),
        "export".into(),
        DocumentId(1),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap()
}

#[test]
fn index_export_roundtrips_current_document_without_touching_original_bin() {
    let mut doc = fixture(include_bytes!(
        "../../../tests/fixtures/Geometry_Findings.glb"
    ));
    let original = doc.source().clone();
    let cancel = Cancellation::default();
    let limits = ImportLimits::default();
    let p = doc
        .prepare_triangle_removal(doc.tag(), MeshId(0), &[1], limits, &cancel)
        .unwrap();
    doc.apply_triangle_removal(p, limits, &cancel).unwrap();
    let copy = prepare_glb(&doc, doc.tag(), ExportLimits::default(), limits, &cancel).unwrap();
    assert!(copy.is_current(&doc));
    assert_eq!(copy.tag(), doc.tag());
    let reimport = import_glb(
        copy.bytes().clone(),
        "copy".into(),
        DocumentId(2),
        limits,
        &cancel,
    )
    .unwrap();
    assert_eq!(
        reimport.primitives()[0].indices,
        doc.primitives()[0].indices
    );
    assert_eq!(
        reimport.snapshot(limits, &cancel).unwrap().indices.len() / 3,
        9
    );
    assert!(Arc::ptr_eq(doc.source(), &original));
    let old = gltf::binary::Glb::from_slice(&original).unwrap();
    let new = gltf::binary::Glb::from_slice(copy.bytes()).unwrap();
    assert!(new.bin.unwrap().starts_with(&old.bin.unwrap()));
}

fn bytes(mut json: serde_json::Value, mut bin: Vec<u8>) -> Vec<u8> {
    json["buffers"][0]["byteLength"] = serde_json::json!(bin.len());
    let mut text = serde_json::to_vec(&json).unwrap();
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let mut result = Vec::new();
    for word in [
        0x46546c67u32,
        2,
        (28 + text.len() + bin.len()) as u32,
        text.len() as u32,
        0x4e4f534a,
    ] {
        result.extend(word.to_le_bytes());
    }
    result.extend(text);
    result.extend((bin.len() as u32).to_le_bytes());
    result.extend(0x004e4942u32.to_le_bytes());
    result.extend(bin);
    result
}
fn tiny() -> (serde_json::Value, Vec<u8>) {
    let json = serde_json::json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":48}],
        "bufferViews":[{"buffer":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":12}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
            {"bufferView":1,"componentType":5123,"count":6,"type":"SCALAR"}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],
        "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0});
    let mut bin = Vec::new();
    for v in [0f32, 0., 0., 1., 0., 0., 0., 1., 0.] {
        bin.extend(v.to_le_bytes());
    }
    for i in [0u16, 1, 2, 0, 1, 2] {
        bin.extend(i.to_le_bytes());
    }
    (json, bin)
}
fn remove(doc: &mut AssetDocument, mesh: usize) {
    let c = Cancellation::default();
    let l = ImportLimits::default();
    let p = doc
        .prepare_triangle_removal(doc.tag(), MeshId(mesh), &[0], l, &c)
        .unwrap();
    doc.apply_triangle_removal(p, l, &c).unwrap();
}
fn export(doc: &AssetDocument) -> AssetResult<VerifiedGlb> {
    prepare_glb(
        doc,
        doc.tag(),
        ExportLimits::default(),
        ImportLimits::default(),
        &Cancellation::default(),
    )
}

#[test]
fn unchanged_and_undone_export_reuses_exact_source_but_proofs_never_revive() {
    let (j, b) = tiny();
    let mut doc = fixture(&bytes(j, b));
    let before = export(&doc).unwrap();
    assert!(Arc::ptr_eq(before.bytes(), doc.source()));
    let other = import_glb(
        doc.source().clone(),
        "other".into(),
        doc.id(),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(doc.tag(), other.tag());
    assert!(!before.is_current(&other));
    let c = Cancellation::default();
    let l = ImportLimits::default();
    let p = doc
        .prepare_triangle_removal(doc.tag(), MeshId(0), &[1], l, &c)
        .unwrap();
    let mut change = doc
        .apply_triangle_removal(p, l, &c)
        .unwrap()
        .change
        .unwrap();
    assert!(!before.is_current(&doc));
    doc.exchange_indices(doc.tag(), &mut change, l, &c).unwrap();
    assert!(!before.is_current(&doc));
    let after = export(&doc).unwrap();
    assert_eq!(after.tag().revision, 2);
    assert!(Arc::ptr_eq(after.bytes(), doc.source()));
}

#[test]
fn all_core_material_roles_and_vertex_attributes_roundtrip_after_index_change() {
    for file in [
        include_bytes!("../../../tests/fixtures/Color_Checker.glb").as_slice(),
        include_bytes!("../../../tests/fixtures/MR_Checker.glb").as_slice(),
        include_bytes!("../../../tests/fixtures/Normal_Checker.glb").as_slice(),
        include_bytes!("../../../tests/fixtures/AO_Checker.glb").as_slice(),
        include_bytes!("../../../tests/fixtures/Emission_Checker.glb").as_slice(),
        include_bytes!("../../../tests/fixtures/Pbr_Factors.glb").as_slice(),
    ] {
        let mut d = fixture(file);
        assert!(Arc::ptr_eq(export(&d).unwrap().bytes(), d.source()));
        remove(&mut d, 0);
        let result = export(&d).unwrap();
        let read = fixture(result.bytes());
        roundtrip::compare(&d, &read, &[], &Cancellation::default()).unwrap();
    }
}

#[test]
fn integer_source_indices_and_shared_accessors_remain_independent() {
    for component in [5121, 5123, 5125] {
        let (mut j, mut b) = tiny();
        b.truncate(36);
        for i in [0u32, 1, 2, 0, 1, 2] {
            match component {
                5121 => b.push(i as u8),
                5123 => b.extend((i as u16).to_le_bytes()),
                _ => b.extend(i.to_le_bytes()),
            }
        }
        j["bufferViews"][1]["byteLength"] = serde_json::json!(b.len() - 36);
        j["accessors"][1]["componentType"] = serde_json::json!(component);
        let primitive = j["meshes"][0]["primitives"][0].clone();
        j["meshes"][0]["primitives"]
            .as_array_mut()
            .unwrap()
            .push(primitive);
        let mut d = fixture(&bytes(j, b));
        remove(&mut d, 0);
        let result = export(&d).unwrap();
        let read = fixture(result.bytes());
        assert_eq!(read.primitives()[0].indices, [0, 1, 2]);
        assert_eq!(read.primitives()[1].indices, [0, 1, 2, 0, 1, 2]);
    }
}

#[test]
fn implicit_indices_inactive_primitives_and_multiple_scenes_preserve_source_binding() {
    let (mut j, mut b) = tiny();
    // Six unindexed positions describe two equal triangles.
    let p = b[..36].to_vec();
    b = p.clone();
    b.extend(p);
    j["bufferViews"] = serde_json::json!([{"buffer":0,"byteLength":72}]);
    j["accessors"].as_array_mut().unwrap().truncate(1);
    j["accessors"][0]["count"] = serde_json::json!(6);
    j["meshes"][0]["primitives"][0]
        .as_object_mut()
        .unwrap()
        .remove("indices");
    let inactive = j["meshes"][0].clone();
    j["meshes"].as_array_mut().unwrap().push(inactive);
    j["nodes"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"mesh":1,"name":"inactive"}));
    j["scenes"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"name":"other scene","nodes":[1]}));
    let mut d = fixture(&bytes(j, b));
    remove(&mut d, 1);
    let v = export(&d).unwrap();
    let r = fixture(v.bytes());
    assert_eq!(r.primitives()[0].indices, [0, 1, 2, 3, 4, 5]);
    assert_eq!(r.primitives()[1].indices, [3, 4, 5]);
    assert_eq!(r.nodes().len(), 2);
    assert_eq!(r.roots(), [NodeId(0)]);
    assert_eq!(gltf::Gltf::from_slice(v.bytes()).unwrap().scenes().len(), 2);
}

#[test]
fn opaque_metadata_and_unknown_chunks_copy_exactly_but_block_changed_output() {
    for what in 0..4 {
        let (mut j, b) = tiny();
        match what {
            0 => j["extras"] = serde_json::json!({"author":"retained"}),
            1 => {
                j["extensionsUsed"] = serde_json::json!(["OWNER_unknown"]);
                j["meshes"][0]["extensions"] =
                    serde_json::json!({"OWNER_unknown":{"indexSensitive":true}});
            }
            2 => j["ownerCustomField"] = serde_json::json!({"data":17}),
            _ => {}
        }
        let mut source = bytes(j, b);
        if what == 3 {
            source.extend(4u32.to_le_bytes());
            source.extend(0x12345678u32.to_le_bytes());
            source.extend([1, 2, 3, 4]);
            let len = source.len() as u32;
            source[8..12].copy_from_slice(&len.to_le_bytes());
        }
        let mut d = fixture(&source);
        assert_eq!(&**export(&d).unwrap().bytes(), source.as_slice());
        remove(&mut d, 0);
        assert!(matches!(export(&d),Err(e) if e.code==ErrorCode::Unsupported));
    }
}

#[test]
fn output_json_and_changed_count_budgets_have_explicit_boundaries() {
    let (j, b) = tiny();
    let mut d = fixture(&bytes(j, b));
    remove(&mut d, 0);
    let v = export(&d).unwrap();
    let glb = gltf::binary::Glb::from_slice(v.bytes()).unwrap();
    let json_len = glb.json.len();
    let mut l = ExportLimits {
        output_bytes: v.bytes().len(),
        json_bytes: json_len,
        ..Default::default()
    };
    let i = ImportLimits::default();
    let c = Cancellation::default();
    prepare_glb(&d, d.tag(), l, i, &c).unwrap();
    l.output_bytes -= 1;
    assert!(matches!(prepare_glb(&d,d.tag(),l,i,&c),Err(e) if e.code==ErrorCode::ResourceLimit));
    l.output_bytes += 1;
    l.json_bytes -= 1;
    assert!(matches!(prepare_glb(&d,d.tag(),l,i,&c),Err(e) if e.code==ErrorCode::ResourceLimit));
    for l in [
        ExportLimits {
            changed_primitives: 0,
            ..Default::default()
        },
        ExportLimits {
            metadata_values: 1,
            ..Default::default()
        },
    ] {
        assert!(
            matches!(prepare_glb(&d,d.tag(),l,i,&c),Err(e) if e.code==ErrorCode::ResourceLimit)
        );
    }
}

#[test]
fn strict_metadata_rejects_duplicates_depth_and_item_overflow() {
    let c = Cancellation::default();
    assert!(
        matches!(metadata::parse(br#"{"a":1,"a":2}"#,100,&c),Err(e) if e.code==ErrorCode::Unsupported)
    );
    let deep = format!("{}0{}", "[".repeat(66), "]".repeat(66));
    assert!(
        matches!(metadata::parse(deep.as_bytes(),1000,&c),Err(e) if e.code==ErrorCode::ResourceLimit)
    );
    assert!(matches!(metadata::parse(b"[0,1,2]",3,&c),Err(e) if e.code==ErrorCode::ResourceLimit));
}

#[test]
fn changed_export_refuses_opaque_or_invalid_accessor_bounds() {
    for bound in [
        serde_json::json!({"ownerIndexSensitive": [0,1,2]}),
        serde_json::json!([{"ownerIndexSensitive": 0}]),
        serde_json::json!([0, 1]),
        serde_json::json!([-1]),
        serde_json::json!([0.5]),
    ] {
        let (mut j, b) = tiny();
        j["accessors"][1]["min"] = bound;
        let mut d = fixture(&bytes(j, b));
        // Viewing may accept untyped bounds; copying must remain exact.
        export(&d).unwrap();
        remove(&mut d, 0);
        assert!(matches!(export(&d),Err(e) if e.code==ErrorCode::Unsupported));
    }
}

#[test]
fn stale_or_cancelled_export_never_yields_a_current_proof() {
    let (j, b) = tiny();
    let mut d = fixture(&bytes(j, b));
    let old = d.tag();
    remove(&mut d, 0);
    assert!(
        matches!(prepare_glb(&d,old,Default::default(),Default::default(),&Cancellation::default()),Err(e) if e.code==ErrorCode::Conflict)
    );
    let c = Cancellation::default();
    c.cancel();
    assert!(
        matches!(prepare_glb(&d,d.tag(),Default::default(),Default::default(),&c),Err(e) if e.code==ErrorCode::Cancelled)
    );
}

#[test]
fn mismatched_authoring_data_cannot_be_certified_by_copying_original_bytes() {
    let (j, b) = tiny();
    let original: Arc<[u8]> = bytes(j, b).into();
    let mesh = MeshPrimitive {
        positions: vec![[9., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        indices: vec![0, 1, 2, 0, 1, 2],
        normals: None,
        tangents: None,
        uv_channels: Vec::new(),
        uv_bindings: UvBindings::default(),
        colors: None,
        color_texture: None,
        metallic_roughness_texture: None,
        normal_texture: None,
        occlusion_texture: None,
        emissive_texture: None,
        normal_scale: 1.,
        occlusion_strength: 1.,
        emissive_factor: [0.; 3],
        base_color: [1.; 4],
        metallic_roughness: [1.; 2],
        alpha_cutoff: None,
        blend: false,
    };
    let d = AssetDocument::new(
        DocumentParts {
            id: DocumentId(1),
            name: "mismatch".into(),
            source: original,
            meshes: vec![mesh],
            nodes: vec![Node {
                name: "Object".into(),
                local: [
                    [1., 0., 0., 0.],
                    [0., 1., 0., 0.],
                    [0., 0., 1., 0.],
                    [0., 0., 0., 1.],
                ],
                children: Vec::new(),
                meshes: Arc::from([MeshId(0)]),
            }],
            roots: vec![NodeId(0)],
            notes: Vec::new(),
        },
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    assert!(matches!(export(&d),Err(e) if e.code==ErrorCode::VerificationFailed));
}

fn preflight(view: &ad_document::DocumentView<'_>, limits: ExportLimits) -> AssetResult<()> {
    preflight_changed_export(
        view,
        limits,
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .map(|_| ())
}

#[test]
fn preflight_refuses_unsupported_changed_export_before_apply_and_agrees_with_export() {
    for what in 0..5 {
        let (mut j, b) = tiny();
        match what {
            0 => j["extras"] = serde_json::json!({"author":"retained"}),
            1 => {
                j["extensionsUsed"] = serde_json::json!(["OWNER_unknown"]);
                j["meshes"][0]["extensions"] =
                    serde_json::json!({"OWNER_unknown":{"indexSensitive":true}});
            }
            2 => j["ownerCustomField"] = serde_json::json!({"data":17}),
            _ => {}
        }
        let mut source = bytes(j, b);
        if what == 3 {
            source.extend(4u32.to_le_bytes());
            source.extend(0x12345678u32.to_le_bytes());
            source.extend([1, 2, 3, 4]);
            let len = source.len() as u32;
            source[8..12].copy_from_slice(&len.to_le_bytes());
        }
        let mut d = fixture(&source);
        let c = Cancellation::default();
        let l = ImportLimits::default();
        // The unchanged document is an exact source copy and always eligible.
        preflight(&d.view(), ExportLimits::default()).unwrap();
        let verdict = {
            let plan = d
                .prepare_triangle_removal(d.tag(), MeshId(0), &[0], l, &c)
                .unwrap();
            let view = d
                .candidate_view(plan.candidate(&d, &c).unwrap(), &c)
                .unwrap();
            preflight(&view, ExportLimits::default())
        };
        assert_eq!(d.revision(), 0, "preflight never mutates");
        remove(&mut d, 0);
        let exported = export(&d);
        if what == 4 {
            verdict.unwrap();
            exported.unwrap();
        } else {
            assert!(matches!(verdict, Err(e) if e.code == ErrorCode::Unsupported));
            assert!(matches!(exported, Err(e) if e.code == ErrorCode::Unsupported));
        }
    }
}

#[test]
fn preflight_counts_earlier_edits_with_the_candidate_and_honours_budgets_and_cancel() {
    let mut d = fixture(include_bytes!("../../../tests/fixtures/Edge_Topology.glb"));
    let c = Cancellation::default();
    let l = ImportLimits::default();
    let editable: Vec<usize> = (0..d.primitives().len())
        .filter(|&m| d.primitives()[m].indices.len() >= 6)
        .collect();
    assert!(
        editable.len() >= 2,
        "fixture needs two multi-triangle primitives"
    );
    remove(&mut d, editable[0]);
    let plan = d
        .prepare_triangle_removal(d.tag(), MeshId(editable[1]), &[0], l, &c)
        .unwrap();
    let view = d
        .candidate_view(plan.candidate(&d, &c).unwrap(), &c)
        .unwrap();
    preflight(&view, ExportLimits::default()).unwrap();
    let one = ExportLimits {
        changed_primitives: 1,
        ..Default::default()
    };
    // The current document alone changes one primitive; the candidate a second.
    preflight(&d.view(), one).unwrap();
    assert!(matches!(preflight(&view, one), Err(e) if e.code == ErrorCode::ResourceLimit));
    let tiny_output = ExportLimits {
        output_bytes: 16,
        ..Default::default()
    };
    assert!(matches!(preflight(&view, tiny_output), Err(e) if e.code == ErrorCode::ResourceLimit));
    let cancelled = Cancellation::default();
    cancelled.cancel();
    assert!(matches!(
        preflight_changed_export(&view, ExportLimits::default(), l, &cancelled),
        Err(e) if e.code == ErrorCode::Cancelled
    ));
}

#[test]
fn public_reimport_comparison_accepts_equal_and_refuses_index_differences() {
    let bytes = include_bytes!("../../../tests/fixtures/Geometry_Findings.glb");
    let a = fixture(bytes);
    let mut b = fixture(bytes);
    let c = Cancellation::default();
    crate::compare_reimported(&a, &b, &c).unwrap();
    remove(&mut b, 0);
    assert!(crate::compare_reimported(&a, &b, &c).is_err());
}

/// ADR-0041: a placement rewrites only the root transforms, in each root's
/// own form; children, primitives and BIN stay exact and the proof is strict.
#[test]
fn placement_export_writes_root_transforms_in_their_own_form() {
    use ad_document::Placement;
    let (mut j, b) = tiny();
    let r = std::f32::consts::FRAC_1_SQRT_2;
    j["nodes"] = serde_json::json!([
        {"mesh":0},
        {"mesh":0,"translation":[1,2,3],"rotation":[0,r,0,r],"scale":[2,2,2]},
        {"matrix":[1,0,0,0, 0,1,0,0, 0,0,1,0, 5,0,0,1],"children":[3]},
        {"mesh":0,"translation":[0,0,1]}
    ]);
    j["scenes"] = serde_json::json!([{"nodes":[0,1,2]}]);
    let source = bytes(j, b);
    let mut doc = import_glb(
        Arc::from(source.as_slice()),
        "placed".into(),
        ad_document::DocumentId(9),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    let c = Cancellation::default();
    let before = doc.snapshot(ImportLimits::default(), &c).unwrap();
    let placement = Placement::IDENTITY.then([0., 0., 0.], 1.5, [0.25, -1., 4.]);
    let plan = doc.prepare_placement(doc.tag(), placement, &c).unwrap();
    {
        let candidate = plan.candidate(&doc, &c).unwrap();
        let view = doc.candidate_view(candidate, &c).unwrap();
        let appended =
            preflight_changed_export(&view, ExportLimits::default(), ImportLimits::default(), &c)
                .unwrap();
        assert_eq!(appended, 0, "placement appends no BIN bytes");
    }
    doc.apply_triangle_removal(plan, ImportLimits::default(), &c)
        .unwrap();
    let verified = export(&doc).unwrap();
    let out = verified.bytes();
    assert_ne!(&out[..], &source[..]);
    let n = u32::from_le_bytes(out[12..16].try_into().unwrap()) as usize;
    let json: serde_json::Value = serde_json::from_slice(&out[20..20 + n]).unwrap();
    let nodes = &json["nodes"];
    assert_eq!(
        nodes[0]["translation"],
        serde_json::json!([0.25, -1.0, 4.0])
    );
    assert_eq!(nodes[0]["scale"], serde_json::json!([1.5, 1.5, 1.5]));
    assert_eq!(nodes[1]["scale"], serde_json::json!([3.0, 3.0, 3.0]));
    assert_eq!(
        nodes[1]["rotation"].as_array().unwrap().len(),
        4,
        "rotation kept"
    );
    assert!(
        nodes[2].get("translation").is_none(),
        "matrix stays a matrix"
    );
    assert_eq!(nodes[2]["matrix"][12], serde_json::json!(7.75));
    assert_eq!(
        nodes[3]["translation"],
        serde_json::json!([0, 0, 1]),
        "child unchanged"
    );
    // The original BIN is reused byte for byte.
    let tail = &source[source.len() - 48..];
    assert_eq!(&out[out.len() - 48..], tail);
    let read = import_glb(
        out.clone(),
        "read".into(),
        ad_document::DocumentId(10),
        ImportLimits::default(),
        &c,
    )
    .unwrap();
    crate::compare_reimported(&doc, &read, &c).unwrap();
    let saved = read.snapshot(ImportLimits::default(), &c).unwrap();
    for (a, b) in before.positions.iter().zip(&saved.positions) {
        let want = placement.apply(*a);
        assert!(
            want.iter().zip(b).all(|(w, g)| (w - g).abs() < 1e-4),
            "{want:?} vs {b:?}"
        );
    }
    // A mismatching placement is not certified by the reimport check.
    assert!(crate::compare_reimported(&read, &doc, &c).is_err());
}

/// ADR-0041 amendment: a placement-only export keeps extras, extensions and
/// unknown members verbatim, writes a turned root rotation and proves the
/// reimport; changing geometry of such a file stays refused.
#[test]
fn placement_export_keeps_opaque_metadata_and_writes_rotation() {
    use ad_document::Placement;
    let (mut j, b) = tiny();
    j["nodes"][0]["extras"] = serde_json::json!({"note":"keep"});
    j["nodes"][0]["extensions"] = serde_json::json!({"EXT_custom":{"a":1}});
    j["extensionsUsed"] = serde_json::json!(["EXT_custom"]);
    j["asset"]["extras"] = serde_json::json!({"author":"test"});
    j["unknownTopLevel"] = serde_json::json!([1, 2]);
    let source = bytes(j, b);
    let mut doc = import_glb(
        Arc::from(source.as_slice()),
        "extras".into(),
        ad_document::DocumentId(11),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    let c = Cancellation::default();
    let before = doc.snapshot(ImportLimits::default(), &c).unwrap();
    let turn = [0., (0.3f32).sin(), 0., (0.3f32).cos()];
    let placement = Placement::IDENTITY.transformed([0.; 3], 2., turn, [1., 0., -0.5]);
    let plan = doc.prepare_placement(doc.tag(), placement, &c).unwrap();
    {
        let candidate = plan.candidate(&doc, &c).unwrap();
        let view = doc.candidate_view(candidate, &c).unwrap();
        assert_eq!(
            preflight_changed_export(&view, ExportLimits::default(), ImportLimits::default(), &c)
                .unwrap(),
            0
        );
    }
    doc.apply_triangle_removal(plan, ImportLimits::default(), &c)
        .unwrap();
    let verified = export(&doc).unwrap();
    let out = verified.bytes();
    let n = u32::from_le_bytes(out[12..16].try_into().unwrap()) as usize;
    let json: serde_json::Value = serde_json::from_slice(&out[20..20 + n]).unwrap();
    assert_eq!(
        json["nodes"][0]["extras"],
        serde_json::json!({"note":"keep"})
    );
    assert_eq!(
        json["nodes"][0]["extensions"],
        serde_json::json!({"EXT_custom":{"a":1}})
    );
    assert_eq!(json["extensionsUsed"], serde_json::json!(["EXT_custom"]));
    assert_eq!(
        json["asset"]["extras"],
        serde_json::json!({"author":"test"})
    );
    assert_eq!(json["unknownTopLevel"], serde_json::json!([1, 2]));
    assert_eq!(json["nodes"][0]["rotation"].as_array().unwrap().len(), 4);
    let read = import_glb(
        out.clone(),
        "read".into(),
        ad_document::DocumentId(12),
        ImportLimits::default(),
        &c,
    )
    .unwrap();
    crate::compare_reimported(&doc, &read, &c).unwrap();
    let saved = read.snapshot(ImportLimits::default(), &c).unwrap();
    for (a, b) in before.positions.iter().zip(&saved.positions) {
        let want = placement.apply(*a);
        assert!(
            want.iter().zip(b).all(|(w, g)| (w - g).abs() < 1e-4),
            "{want:?} vs {b:?}"
        );
    }
    // Animations or skins could drive the root transforms: refused.
    for key in ["animations", "skins"] {
        let meta = serde_json::json!({"nodes": [], key: [{}]});
        assert!(crate::metadata::placement_only(&meta, &c).is_err(), "{key}");
    }
    let empty = serde_json::json!({"nodes": [], "animations": [], "skins": []});
    crate::metadata::placement_only(&empty, &c).unwrap();
    // A geometry change of the same file still needs certified metadata.
    let mut changed = fixture(&source);
    remove(&mut changed, 0);
    assert!(export(&changed).is_err());
}

/// ADR-0048: a moved child is written as a matrix, a deleted node leaves the
/// scene lists (the node itself stays unused), the BIN is reused and the
/// reimport proves the saved scene equals the edited one.
#[test]
fn object_edits_export_node_matrices_and_detach_nodes() {
    use ad_document::{NodeId, NodeOp};
    let (mut j, b) = tiny();
    j["nodes"] = serde_json::json!([
        {"mesh":0,"extras":{"keep":true}},
        {"mesh":0,"translation":[1,2,3]},
        {"children":[3, 4]},
        {"mesh":0,"translation":[0,0,1],"rotation":[0,0,0,1],"scale":[1,1,1]},
        {"mesh":0}
    ]);
    j["scenes"] = serde_json::json!([{"nodes":[0,1,2]}]);
    let source = bytes(j, b);
    let mut doc = import_glb(
        Arc::from(source.as_slice()),
        "objects".into(),
        ad_document::DocumentId(21),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap();
    let c = Cancellation::default();
    // Uniform scale 2, a turn of 0.4 rad about Y and a move.
    let (sin, cos) = 0.4f32.sin_cos();
    let moved = [
        [2. * cos, 0., -2. * sin, 0.],
        [0., 2., 0., 0.],
        [2. * sin, 0., 2. * cos, 0.],
        [5., 0.5, -1., 1.],
    ];
    for op in [NodeOp::Local(NodeId(3), moved), NodeOp::Detach(NodeId(1))] {
        let plan = doc.prepare_node_op(doc.tag(), op, &c).unwrap();
        {
            let candidate = plan.candidate(&doc, &c).unwrap();
            let view = doc.candidate_view(candidate, &c).unwrap();
            assert_eq!(
                preflight_changed_export(
                    &view,
                    ExportLimits::default(),
                    ImportLimits::default(),
                    &c
                )
                .unwrap(),
                0
            );
        }
        doc.apply_triangle_removal(plan, ImportLimits::default(), &c)
            .unwrap();
    }
    assert_eq!(doc.scene_roots(), [NodeId(0), NodeId(2)]);
    let edited = doc.snapshot(ImportLimits::default(), &c).unwrap();
    assert_eq!(edited.draws.len(), 3, "node 1 is out of the scene");
    let verified = export(&doc).unwrap();
    let out = verified.bytes();
    let n = u32::from_le_bytes(out[12..16].try_into().unwrap()) as usize;
    let json: serde_json::Value = serde_json::from_slice(&out[20..20 + n]).unwrap();
    assert_eq!(json["scenes"][0]["nodes"], serde_json::json!([0, 2]));
    assert_eq!(json["nodes"][3]["matrix"].as_array().unwrap().len(), 16);
    assert!(json["nodes"][3].get("translation").is_none());
    assert_eq!(
        json["nodes"][0]["extras"],
        serde_json::json!({"keep": true})
    );
    assert_eq!(
        json["nodes"].as_array().unwrap().len(),
        5,
        "the deleted node stays unused"
    );
    let tail = &source[source.len() - 48..];
    assert_eq!(&out[out.len() - 48..], tail);
    let read = import_glb(
        out.clone(),
        "read".into(),
        ad_document::DocumentId(22),
        ImportLimits::default(),
        &c,
    )
    .unwrap();
    crate::compare_reimported(&doc, &read, &c).unwrap();
    let saved = read.snapshot(ImportLimits::default(), &c).unwrap();
    assert_eq!(saved.positions, edited.positions);
    // Undo-like: the original document does not match the edited copy.
    let original = import_glb(
        Arc::from(source.as_slice()),
        "original".into(),
        ad_document::DocumentId(23),
        ImportLimits::default(),
        &c,
    )
    .unwrap();
    assert!(crate::compare_reimported(&original, &read, &c).is_err());
    // A sheared transform and deleting everything are refused.
    let shear = [
        [1., 0.5, 0., 0.],
        [0., 1., 0., 0.],
        [0., 0., 1., 0.],
        [0., 0., 0., 1.],
    ];
    assert!(
        doc.prepare_node_op(doc.tag(), NodeOp::Local(NodeId(3), shear), &c)
            .is_err()
    );
    let mut last = doc;
    for node in [0, 2] {
        let result = last.prepare_node_op(last.tag(), NodeOp::Detach(NodeId(node)), &c);
        if node == 2 {
            assert!(result.is_err(), "the last object stays");
        } else {
            last.apply_triangle_removal(result.unwrap(), ImportLimits::default(), &c)
                .unwrap();
        }
    }
}

/// Multi-selection: several object edits as one edit (one Undo step),
/// verified and exported like single ones; nested or empty groups and a
/// group that deletes everything are refused.
#[test]
fn a_group_of_object_edits_is_one_edit() {
    use ad_document::{NodeId, NodeOp};
    let (mut j, b) = tiny();
    j["nodes"] = serde_json::json!([{"mesh":0},{"mesh":0},{"mesh":0},{"mesh":0}]);
    j["scenes"] = serde_json::json!([{"nodes":[0,1,2,3]}]);
    let source = bytes(j, b);
    let mut doc = fixture(&source);
    let c = Cancellation::default();
    let mut moved = [[0f32; 4]; 4];
    for (i, row) in moved.iter_mut().enumerate() {
        row[i] = 1.;
    }
    moved[3] = [4., 0., 0., 1.];
    let group = NodeOp::Many(
        vec![
            NodeOp::Detach(NodeId(0)),
            NodeOp::Detach(NodeId(1)),
            NodeOp::Local(NodeId(2), moved),
        ]
        .into(),
    );
    let revision = doc.revision();
    let plan = doc.prepare_node_op(doc.tag(), group, &c).unwrap();
    doc.apply_triangle_removal(plan, ImportLimits::default(), &c)
        .unwrap();
    assert_eq!(doc.revision(), revision + 1, "one edit");
    assert_eq!(doc.scene_roots(), [NodeId(2), NodeId(3)]);
    let read = import_glb(
        export(&doc).unwrap().bytes().clone(),
        "group".into(),
        ad_document::DocumentId(24),
        ImportLimits::default(),
        &c,
    )
    .unwrap();
    crate::compare_reimported(&doc, &read, &c).unwrap();
    for refused in [
        NodeOp::Many(Vec::new().into()),
        NodeOp::Many(vec![NodeOp::Many(vec![NodeOp::Detach(NodeId(2))].into())].into()),
        NodeOp::Many(vec![NodeOp::Detach(NodeId(2)), NodeOp::Detach(NodeId(3))].into()),
    ] {
        assert!(doc.prepare_node_op(doc.tag(), refused, &c).is_err());
    }
}

#[test]
fn detaching_a_shared_root_only_changes_the_active_scene() {
    use ad_document::{NodeId, NodeOp};

    let (mut json, bin) = tiny();
    json["nodes"] = serde_json::json!([{"mesh":0},{"mesh":0}]);
    json["scenes"] = serde_json::json!([
        {"nodes":[0],"name":"Hidden","extras":{"marker":17}},
        {"nodes":[0,1],"name":"Active"}
    ]);
    json["scene"] = serde_json::json!(1);
    let mut doc = fixture(&bytes(json.clone(), bin.clone()));
    let c = Cancellation::default();
    let plan = doc
        .prepare_node_op(doc.tag(), NodeOp::Detach(NodeId(0)), &c)
        .unwrap();
    doc.apply_triangle_removal(plan, ImportLimits::default(), &c)
        .unwrap();
    let saved = export(&doc).unwrap();
    let glb = gltf::binary::Glb::from_slice(saved.bytes()).unwrap();
    let meta: serde_json::Value = serde_json::from_slice(&glb.json).unwrap();
    assert_eq!(meta["scene"], 1);
    assert_eq!(meta["scenes"][0], json["scenes"][0]);
    assert_eq!(meta["scenes"][1]["nodes"], serde_json::json!([1]));

    // A change to the inactive scene must also fail independent readback.
    let mut altered = meta;
    altered["scenes"][0]["nodes"] = serde_json::json!([1]);
    let altered = fixture(&bytes(altered, bin));
    let err = crate::compare_reimported(&doc, &altered, &c).unwrap_err();
    assert_eq!(err.code, ErrorCode::VerificationFailed);
}

#[test]
fn detaching_a_child_of_a_shared_parent_is_refused_during_edit_preflight() {
    use ad_document::{NodeId, NodeOp};

    let (mut json, bin) = tiny();
    json["nodes"] = serde_json::json!([
        {"children":[1]},{"mesh":0},{"mesh":0}
    ]);
    json["scenes"] = serde_json::json!([{"nodes":[0,2]},{"nodes":[0]}]);
    json.as_object_mut().unwrap().remove("scene"); // The first scene is active.
    let doc = fixture(&bytes(json, bin));
    let c = Cancellation::default();
    let plan = doc
        .prepare_node_op(doc.tag(), NodeOp::Detach(NodeId(1)), &c)
        .unwrap();
    let candidate = plan.candidate(&doc, &c).unwrap();
    let view = doc.candidate_view(candidate, &c).unwrap();
    let export_limits = ExportLimits::default();
    let import_limits = ImportLimits::default();
    let err = preflight_changed_export(&view, export_limits, import_limits, &c).unwrap_err();
    assert_eq!(err.code, ErrorCode::Unsupported);
    assert!(doc.node_edits().is_empty());
}
