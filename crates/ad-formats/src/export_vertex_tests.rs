//! ADR-0033: export of primitives whose vertex streams were compacted.
use super::*;
use serde_json::{Value, json};

fn glb(json: Value, mut bin: Vec<u8>) -> Vec<u8> {
    let mut json = json;
    json["buffers"][0]["byteLength"] = json!(bin.len());
    let mut text = serde_json::to_vec(&json).unwrap();
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let mut out = Vec::new();
    for word in [
        0x46546c67u32,
        2,
        (28 + text.len() + bin.len()) as u32,
        text.len() as u32,
        0x4e4f534a,
    ] {
        out.extend(word.to_le_bytes());
    }
    out.extend(text);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(0x004e4942u32.to_le_bytes());
    out.extend(bin);
    out
}

/// Five vertices (1 unused), interleaved POSITION/NORMAL, TANGENT, normalized
/// U8 and U16 UVs, normalized U16 VEC3 colors and a custom `_ID` attribute
/// holding each vertex's ordinal.
fn rich() -> Vec<u8> {
    let mut bin = Vec::new();
    for v in 0..5 {
        for x in [v as f32, (v * v) as f32 * 0.5, -1.] {
            bin.extend(x.to_le_bytes());
        }
        for x in [0f32, 0., 1.] {
            bin.extend(x.to_le_bytes());
        }
    }
    for v in 0..5 {
        for x in [1f32, v as f32 * 0.1, 0., 1.] {
            bin.extend(x.to_le_bytes());
        }
    }
    for v in 0..5u8 {
        bin.extend([v * 50, 255 - v * 40, 0, 0]);
    }
    for v in 0..5u16 {
        bin.extend((v * 1000).to_le_bytes());
        bin.extend((65535 - v * 7).to_le_bytes());
    }
    for v in 0..5u16 {
        for x in [v * 9000, 1, 65535] {
            bin.extend(x.to_le_bytes());
        }
        bin.extend([0, 0]);
    }
    for v in 0..5 {
        bin.extend((v as f32).to_le_bytes());
    }
    for i in [0u16, 2, 3, 0, 3, 4] {
        bin.extend(i.to_le_bytes());
    }
    let json = json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":0}],
        "bufferViews":[
            {"buffer":0,"byteOffset":0,"byteLength":120,"byteStride":24,"target":34962},
            {"buffer":0,"byteOffset":120,"byteLength":80,"target":34962},
            {"buffer":0,"byteOffset":200,"byteLength":20,"byteStride":4,"target":34962},
            {"buffer":0,"byteOffset":220,"byteLength":20,"target":34962},
            {"buffer":0,"byteOffset":240,"byteLength":40,"byteStride":8,"target":34962},
            {"buffer":0,"byteOffset":280,"byteLength":20,"target":34962},
            {"buffer":0,"byteOffset":300,"byteLength":12,"target":34963}],
        "accessors":[
            {"bufferView":0,"componentType":5126,"count":5,"type":"VEC3","min":[0,0,-1],"max":[4,8,-1]},
            {"bufferView":0,"byteOffset":12,"componentType":5126,"count":5,"type":"VEC3"},
            {"bufferView":1,"componentType":5126,"count":5,"type":"VEC4"},
            {"bufferView":2,"componentType":5121,"normalized":true,"count":5,"type":"VEC2"},
            {"bufferView":3,"componentType":5123,"normalized":true,"count":5,"type":"VEC2"},
            {"bufferView":4,"componentType":5123,"normalized":true,"count":5,"type":"VEC3"},
            {"bufferView":5,"componentType":5126,"count":5,"type":"SCALAR"},
            {"bufferView":6,"componentType":5123,"count":6,"type":"SCALAR"}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1,"TANGENT":2,
            "TEXCOORD_0":3,"TEXCOORD_1":4,"COLOR_0":5,"_ID":6},"indices":7}]}],
        "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0});
    glb(json, bin)
}

fn open(bytes: Vec<u8>) -> AssetDocument {
    import_glb(
        Arc::from(bytes),
        "vertices".into(),
        DocumentId(4),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap()
}
fn compact(doc: &mut AssetDocument, mesh: usize, removed: &[u32]) {
    let (c, l) = (Cancellation::default(), ImportLimits::default());
    let p = doc
        .prepare_vertex_removal(doc.tag(), MeshId(mesh), removed, l, &c)
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
/// Parsed output metadata plus BIN.
fn parts(bytes: &[u8]) -> (Value, Vec<u8>) {
    let g = gltf::binary::Glb::from_slice(bytes).unwrap();
    (
        serde_json::from_slice(&g.json).unwrap(),
        g.bin.unwrap().into_owned(),
    )
}
fn floats(meta: &Value, bin: &[u8], accessor: u64) -> Vec<f32> {
    let a = &meta["accessors"][accessor as usize];
    let v = &meta["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
    let start =
        (v["byteOffset"].as_u64().unwrap_or(0) + a["byteOffset"].as_u64().unwrap_or(0)) as usize;
    let count = a["count"].as_u64().unwrap() as usize;
    (0..count)
        .map(|i| {
            let o = start + i * 4;
            f32::from_le_bytes(bin[o..o + 4].try_into().unwrap())
        })
        .collect()
}

#[test]
fn compacted_primitive_gathers_every_attribute_with_its_encoding() {
    let mut doc = open(rich());
    let original = doc.source().clone();
    compact(&mut doc, 0, &[1]);
    let copy = export(&doc).unwrap();
    assert!(copy.is_current(&doc));
    let (meta, bin) = parts(copy.bytes());
    let old = gltf::binary::Glb::from_slice(&original).unwrap();
    assert!(bin.starts_with(&old.bin.unwrap()));
    let attributes = &meta["meshes"][0]["primitives"][0]["attributes"];
    let accessor = |name: &str| &meta["accessors"][attributes[name].as_u64().unwrap() as usize];
    for name in [
        "POSITION",
        "NORMAL",
        "TANGENT",
        "TEXCOORD_0",
        "TEXCOORD_1",
        "COLOR_0",
        "_ID",
    ] {
        assert!(
            attributes[name].as_u64().unwrap() >= 8,
            "{name} not rewritten"
        );
        assert_eq!(accessor(name)["count"], 4, "{name}");
    }
    // Encodings are kept; vertex elements are 4-byte aligned.
    assert_eq!(accessor("TEXCOORD_0")["componentType"], 5121);
    assert_eq!(accessor("TEXCOORD_0")["normalized"], true);
    assert_eq!(accessor("COLOR_0")["type"], "VEC3");
    let view =
        |name: &str| &meta["bufferViews"][accessor(name)["bufferView"].as_u64().unwrap() as usize];
    assert_eq!(view("TEXCOORD_0")["byteStride"], 4);
    assert_eq!(view("COLOR_0")["byteStride"], 8);
    assert!(view("TEXCOORD_1").get("byteStride").is_none());
    // POSITION bounds are exact for the survivors (vertex 1 removed).
    assert_eq!(accessor("POSITION")["min"], json!([0.0, 0.0, -1.0]));
    assert_eq!(accessor("POSITION")["max"], json!([4.0, 8.0, -1.0]));
    // The undecoded custom attribute follows the same vertex map.
    assert_eq!(
        floats(&meta, &bin, attributes["_ID"].as_u64().unwrap()),
        [0., 2., 3., 4.]
    );
    // The original accessors stay in place.
    assert_eq!(meta["accessors"][6]["count"], 5);
}

#[test]
fn compacting_after_bounds_change_writes_exact_position_bounds() {
    let mut doc = open(rich());
    // Remove the second triangle, then vertex 4 becomes unused too.
    let (c, l) = (Cancellation::default(), ImportLimits::default());
    let p = doc
        .prepare_triangle_removal(doc.tag(), MeshId(0), &[1], l, &c)
        .unwrap();
    doc.apply_triangle_removal(p, l, &c).unwrap();
    compact(&mut doc, 0, &[1, 4]);
    let copy = export(&doc).unwrap();
    let (meta, _) = parts(copy.bytes());
    let position = meta["meshes"][0]["primitives"][0]["attributes"]["POSITION"]
        .as_u64()
        .unwrap() as usize;
    assert_eq!(meta["accessors"][position]["max"], json!([3.0, 4.5, -1.0]));
    let reopened = open(copy.bytes().to_vec());
    assert_eq!(reopened.primitives()[0].positions.len(), 3);
    assert_eq!(reopened.primitives()[0].indices, [0, 1, 2]);
}

#[test]
fn index_removal_after_compaction_and_shared_accessor_siblings_export() {
    // Geometry_Findings: primitive #1 shares POSITION with #0 and uses 0..=2.
    let mut doc = open(include_bytes!("../../../tests/fixtures/Geometry_Findings.glb").to_vec());
    compact(&mut doc, 1, &[3, 4]);
    let (c, l) = (Cancellation::default(), ImportLimits::default());
    let p = doc
        .prepare_triangle_removal(doc.tag(), MeshId(0), &[1], l, &c)
        .unwrap();
    doc.apply_triangle_removal(p, l, &c).unwrap();
    let copy = export(&doc).unwrap();
    let (meta, _) = parts(copy.bytes());
    // Sibling #0 keeps the original POSITION accessor; #1 has its own.
    assert_eq!(
        meta["meshes"][0]["primitives"][0]["attributes"]["POSITION"],
        0
    );
    assert_ne!(
        meta["meshes"][1]["primitives"][0]["attributes"]["POSITION"],
        0
    );
    let reopened = open(copy.bytes().to_vec());
    assert_eq!(reopened.primitives()[1].positions.len(), 3);
    assert_eq!(reopened.primitives()[0].positions.len(), 5);
}

/// Four vertices; v3 repeats v0 in POSITION. `_ID` is equal for v0/v3 when
/// `same_custom`, otherwise it differs (an attribute the document never reads).
fn twins(same_custom: bool) -> Vec<u8> {
    let mut bin = Vec::new();
    for p in [[0f32, 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 0.]] {
        for x in p {
            bin.extend(x.to_le_bytes());
        }
    }
    for id in [7f32, 8., 9., if same_custom { 7. } else { 70. }] {
        bin.extend(id.to_le_bytes());
    }
    for i in [0u16, 1, 2, 3, 2, 1] {
        bin.extend(i.to_le_bytes());
    }
    let json = json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":0}],
        "bufferViews":[
            {"buffer":0,"byteOffset":0,"byteLength":48,"target":34962},
            {"buffer":0,"byteOffset":48,"byteLength":16,"target":34962},
            {"buffer":0,"byteOffset":64,"byteLength":12,"target":34963}],
        "accessors":[
            {"bufferView":0,"componentType":5126,"count":4,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
            {"bufferView":1,"componentType":5126,"count":4,"type":"SCALAR"},
            {"bufferView":2,"componentType":5123,"count":6,"type":"SCALAR"}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0,"_ID":1},"indices":2}]}],
        "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0});
    glb(json, bin)
}

#[test]
fn merged_twins_export_only_when_every_source_attribute_byte_matches() {
    let (c, l) = (Cancellation::default(), ImportLimits::default());
    for same in [true, false] {
        let mut doc = open(twins(same));
        let p = doc
            .prepare_vertex_merge(doc.tag(), MeshId(0), &[[3, 0]], l, &c)
            .unwrap();
        let preflight = {
            let view = doc
                .candidate_view(p.candidate(&doc, &c).unwrap(), &c)
                .unwrap();
            preflight_changed_export(&view, ExportLimits::default(), l, &c)
        };
        if !same {
            // The document cannot see `_ID`; export refuses before Apply.
            assert!(matches!(preflight, Err(e) if e.code == ErrorCode::Unsupported));
            continue;
        }
        preflight.unwrap();
        doc.apply_triangle_removal(p, l, &c).unwrap();
        let copy = export(&doc).unwrap();
        let reopened = open(copy.bytes().to_vec());
        assert_eq!(reopened.primitives()[0].positions.len(), 3);
        assert_eq!(reopened.primitives()[0].indices, [0, 1, 2, 0, 2, 1]);
        let (meta, bin) = parts(copy.bytes());
        let id = meta["meshes"][0]["primitives"][0]["attributes"]["_ID"]
            .as_u64()
            .unwrap();
        assert_eq!(floats(&meta, &bin, id), [7., 8., 9.]);
    }
}

/// ADR-0036/0037: a rewritten NORMAL stream is written from document values;
/// other attributes keep their source accessors.
#[test]
fn rewritten_normals_export_bit_exact_and_siblings_keep_their_accessors() {
    let (c, l) = (Cancellation::default(), ImportLimits::default());
    let up = [0f32, 0., 1.];
    // #0 has no NORMAL (gains one); #1 replaces two zero normals.
    for (mesh, normals) in [
        (0usize, vec![up; 4]),
        (1, vec![[0., 0.6, 0.8], up, up, [0., 0.6, 0.8]]),
    ] {
        let mut doc = open(include_bytes!("../../../tests/fixtures/Normals_Repair.glb").to_vec());
        let p = doc
            .prepare_normals(doc.tag(), MeshId(mesh), normals.clone(), mesh == 1, l, &c)
            .unwrap();
        let appended = {
            let view = doc
                .candidate_view(p.candidate(&doc, &c).unwrap(), &c)
                .unwrap();
            preflight_changed_export(&view, ExportLimits::default(), l, &c).unwrap()
        };
        assert_eq!(appended, 4 * 12 + 6 * 4);
        doc.apply_triangle_removal(p, l, &c).unwrap();
        let copy = export(&doc).unwrap();
        let (meta, _) = parts(copy.bytes());
        let prims = |m: usize| &meta["meshes"][m]["primitives"][0]["attributes"];
        assert_eq!(
            prims(mesh)["POSITION"],
            0,
            "positions keep the source accessor"
        );
        let normal = prims(mesh)["NORMAL"].as_u64().unwrap() as usize;
        assert_eq!(meta["accessors"][normal]["componentType"], 5126);
        let reopened = open(copy.bytes().to_vec());
        assert_eq!(
            reopened.primitives()[mesh].normals.as_deref(),
            Some(&normals[..])
        );
        let other = 1 - mesh;
        assert_eq!(
            reopened.primitives()[other].normals.is_some(),
            doc.primitives()[other].normals.is_some()
        );
    }
}

#[test]
fn undone_compaction_exports_the_exact_source_and_preflight_reports_growth() {
    let mut doc = open(rich());
    let (c, l) = (Cancellation::default(), ImportLimits::default());
    let p = doc
        .prepare_vertex_removal(doc.tag(), MeshId(0), &[1], l, &c)
        .unwrap();
    let appended = {
        let view = doc
            .candidate_view(p.candidate(&doc, &c).unwrap(), &c)
            .unwrap();
        preflight_changed_export(&view, ExportLimits::default(), l, &c).unwrap()
    };
    // 4 vertices: 12+12+16+4+4+8+4 bytes each, plus 6 U32 indices.
    assert_eq!(appended, 4 * 60 + 6 * 4);
    assert_eq!(
        preflight_changed_export(&doc.view(), ExportLimits::default(), l, &c).unwrap(),
        0
    );
    let mut change = doc
        .apply_triangle_removal(p, l, &c)
        .unwrap()
        .change
        .unwrap();
    doc.exchange_indices(doc.tag(), &mut change, l, &c).unwrap();
    let copy = export(&doc).unwrap();
    assert!(Arc::ptr_eq(copy.bytes(), doc.source()));
}
