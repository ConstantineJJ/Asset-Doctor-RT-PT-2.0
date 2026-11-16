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
        roundtrip::compare(&d, &read, &Cancellation::default()).unwrap();
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
