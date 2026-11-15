//! Self-contained file format boundary. No OS dialogs, network or Vulkan.
use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentId, DocumentParts, ErrorCode,
    ImportLimits, MeshId, MeshPrimitive, Node, NodeId, geometry_bytes,
};
use std::sync::Arc;

pub fn import_glb(
    source: Arc<[u8]>,
    name: String,
    id: DocumentId,
    limits: ImportLimits,
    cancel: &Cancellation,
) -> AssetResult<AssetDocument> {
    cancel.check()?;
    let invalid = |message: &str| AssetError::new(ErrorCode::InvalidFormat, message);
    let unsupported = |message: &str| AssetError::new(ErrorCode::Unsupported, message);
    let budget = || {
        AssetError::new(
            ErrorCode::ResourceLimit,
            "Import exceeds the configured resource budget",
        )
    };
    if source.len() > limits.source_bytes {
        return Err(budget());
    }
    if source.len() < 20 || &source[..4] != b"glTF" {
        return Err(invalid("Choose a self-contained GLB 2 file"));
    }
    let word = |offset| {
        u32::from_le_bytes(
            source[offset..offset + 4]
                .try_into()
                .expect("Header length checked"),
        )
    };
    if word(4) != 2 || word(8) as usize != source.len() || word(16) != 0x4e4f534a {
        return Err(invalid("Invalid or truncated GLB header"));
    }
    if word(12) as usize > limits.json_bytes {
        return Err(budget());
    }
    let glb = gltf::binary::Glb::from_slice(&source)
        .map_err(|e| invalid(&format!("Invalid GLB container: {e}")))?;
    let root: gltf::json::Root = gltf::json::deserialize::from_slice(&glb.json)
        .map_err(|e| invalid(&format!("Invalid glTF metadata: {e}")))?;
    if !root.extensions_required.is_empty() {
        return Err(unsupported(
            "This build does not support required glTF extensions",
        ));
    }
    if root.nodes.len() > limits.nodes {
        return Err(budget());
    }
    // gltf-json 1.4.1's POSITION validation hook indexes this reference before
    // returning validation errors. Preflight it before invoking that dependency.
    for mesh in &root.meshes {
        for primitive in &mesh.primitives {
            cancel.check()?;
            if primitive
                .attributes
                .values()
                .any(|id| id.value() >= root.accessors.len())
            {
                return Err(invalid("Primitive attribute references a missing accessor"));
            }
        }
    }
    let doc = gltf::Document::from_json(root)
        .map_err(|e| invalid(&format!("glTF validation failed: {e}")))?;
    cancel.check()?;
    if doc.extensions_used().any(|name| {
        matches!(
            name,
            "KHR_draco_mesh_compression" | "EXT_meshopt_compression" | "KHR_mesh_quantization"
        )
    }) {
        return Err(unsupported(
            "Compressed/quantized geometry is not supported in this build",
        ));
    }
    if doc.skins().len() > 0 || doc.animations().len() > 0 {
        return Err(unsupported(
            "This build opens static meshes; skinned or animated assets are not supported yet",
        ));
    }
    let bin = glb
        .bin
        .as_deref()
        .ok_or_else(|| invalid("GLB has no embedded BIN geometry"))?;
    if doc.buffers().len() != 1 {
        return Err(unsupported("This build requires one embedded GLB buffer"));
    }
    for buffer in doc.buffers() {
        if !matches!(buffer.source(), gltf::buffer::Source::Bin) {
            return Err(unsupported(
                "External/data-URI buffers are not supported; use self-contained GLB BIN geometry",
            ));
        }
        if buffer.length() > bin.len() {
            return Err(invalid("Embedded buffer length exceeds BIN chunk"));
        }
    }
    for image in doc.images() {
        if let gltf::image::Source::Uri { uri, .. } = image.source()
            && !uri.starts_with("data:")
        {
            return Err(unsupported(
                "External textures are not supported; embed resources in the GLB",
            ));
        }
    }
    for view in doc.views() {
        if view
            .offset()
            .checked_add(view.length())
            .is_none_or(|end| end > view.buffer().length() || end > bin.len())
        {
            return Err(invalid("Buffer view is outside its embedded buffer"));
        }
    }
    for accessor in doc.accessors() {
        if accessor.sparse().is_some() {
            return Err(unsupported(
                "Sparse accessors are not supported in this build",
            ));
        }
        let view = accessor
            .view()
            .ok_or_else(|| unsupported("An accessor has no buffer view"))?;
        let stride = view.stride().unwrap_or(accessor.size());
        let end = accessor
            .count()
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|bytes| bytes.checked_add(accessor.size()))
            .and_then(|bytes| bytes.checked_add(accessor.offset()));
        if accessor.count() == 0
            || stride < accessor.size()
            || end.is_none_or(|end| end > view.length())
        {
            return Err(invalid("Accessor count/stride exceeds its buffer view"));
        }
    }
    let mut meshes = Vec::new();
    let mut mesh_ids = Vec::new();
    let mut decoded = 0usize;
    for mesh in doc.meshes() {
        let mut ids = Vec::new();
        for primitive in mesh.primitives() {
            cancel.check()?;
            if primitive.material().alpha_mode() != gltf::material::AlphaMode::Opaque {
                return Err(unsupported(
                    "Transparent/masked materials are not supported by this geometry viewer yet",
                ));
            }
            if primitive.mode() != gltf::mesh::Mode::Triangles
                || primitive.morph_targets().len() > 0
            {
                return Err(unsupported(
                    "This build supports static triangle primitives without morph targets",
                ));
            }
            let position = primitive
                .get(&gltf::Semantic::Positions)
                .ok_or_else(|| invalid("Mesh has no POSITION attribute"))?;
            if position.data_type() != gltf::accessor::DataType::F32
                || position.dimensions() != gltf::accessor::Dimensions::Vec3
                || position.normalized()
            {
                return Err(invalid(
                    "Positions must use non-normalized F32 VEC3 accessors",
                ));
            }
            if let Some(indices) = primitive.indices()
                && (indices.dimensions() != gltf::accessor::Dimensions::Scalar
                    || indices.normalized()
                    || !matches!(
                        indices.data_type(),
                        gltf::accessor::DataType::U8
                            | gltf::accessor::DataType::U16
                            | gltf::accessor::DataType::U32
                    ))
            {
                return Err(invalid(
                    "Indices must use non-normalized unsigned SCALAR accessors",
                ));
            }
            let index_count = primitive
                .indices()
                .map_or(position.count(), |indices| indices.count());
            decoded = decoded
                .checked_add(geometry_bytes(position.count(), index_count)?)
                .ok_or_else(budget)?;
            if decoded > limits.decoded_bytes || position.count() > u32::MAX as usize {
                return Err(budget());
            }
            let reader = primitive.reader(|_| Some(bin));
            let mut positions = Vec::new();
            positions
                .try_reserve_exact(position.count())
                .map_err(|_| budget())?;
            for (i, p) in reader
                .read_positions()
                .ok_or_else(|| invalid("Cannot read mesh positions"))?
                .enumerate()
            {
                if i % 4096 == 0 {
                    cancel.check()?;
                }
                positions.push(p);
            }
            let mut indices = Vec::new();
            indices
                .try_reserve_exact(index_count)
                .map_err(|_| budget())?;
            if let Some(read) = reader.read_indices() {
                for (i, index) in read.into_u32().enumerate() {
                    if i % 4096 == 0 {
                        cancel.check()?;
                    }
                    indices.push(index);
                }
            } else {
                indices.extend(0..position.count() as u32);
            }
            ids.push(MeshId(meshes.len()));
            meshes.push(MeshPrimitive {
                positions,
                indices,
                base_color: primitive
                    .material()
                    .pbr_metallic_roughness()
                    .base_color_factor(),
            });
        }
        mesh_ids.push(Arc::<[MeshId]>::from(ids));
    }
    let nodes = doc
        .nodes()
        .map(|node| Node {
            name: node.name().unwrap_or("Object").into(),
            local: node.transform().matrix(),
            children: node.children().map(|child| NodeId(child.index())).collect(),
            meshes: node
                .mesh()
                .map_or_else(|| Arc::from([]), |mesh| mesh_ids[mesh.index()].clone()),
        })
        .collect();
    let scene = doc
        .default_scene()
        .or_else(|| doc.scenes().next())
        .ok_or_else(|| invalid("GLB has no scene"))?;
    let roots = scene.nodes().map(|node| NodeId(node.index())).collect();
    let mut notes =
        vec!["Base-color geometry view; PBR textures/vertex colors are not displayed yet".into()];
    if doc.scenes().len() > 1 {
        notes.push("Only the default/first scene is displayed".into());
    }
    cancel.check()?;
    AssetDocument::new(
        DocumentParts {
            id,
            name,
            source: source.clone(),
            meshes,
            nodes,
            roots,
            notes,
        },
        limits,
        cancel,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ad_document::{AssetError, ErrorCode};
    use serde_json::{Value, json};
    fn json_doc() -> Value {
        json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":42}],
        "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":6}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
            {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],
        "materials":[{"pbrMetallicRoughness":{"baseColorFactor":[0.25,0.6,0.9,1.0]}}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1,"material":0}]}],
        "nodes":[{"mesh":0,"translation":[2,0,0]}],"scenes":[{"nodes":[0]}],"scene":0})
    }
    fn bytes(json: &Value) -> Arc<[u8]> {
        let mut text = serde_json::to_vec(json).unwrap();
        while !text.len().is_multiple_of(4) {
            text.push(b' ');
        }
        let mut bin = Vec::new();
        for v in [0f32, 0., 0., 1., 0., 0., 0., 1., 0.] {
            bin.extend(v.to_le_bytes());
        }
        for i in [0u16, 1, 2] {
            bin.extend(i.to_le_bytes());
        }
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let mut result = Vec::new();
        for word in [
            0x46546c67u32,
            2,
            (12 + 8 + text.len() + 8 + bin.len()) as u32,
            text.len() as u32,
            0x4e4f534a,
        ] {
            result.extend(word.to_le_bytes());
        }
        result.extend(text);
        result.extend((bin.len() as u32).to_le_bytes());
        result.extend(0x004e4942u32.to_le_bytes());
        result.extend(bin);
        result.into()
    }
    fn load(json: &Value) -> AssetResult<AssetDocument> {
        import_glb(
            bytes(json),
            "triangle.glb".into(),
            DocumentId(11),
            ImportLimits::default(),
            &Cancellation::default(),
        )
    }
    #[test]
    fn static_glb_preserves_source_hierarchy_color_and_authored_translation() {
        let source = bytes(&json_doc());
        let before = source.to_vec();
        let doc = import_glb(
            source.clone(),
            "triangle.glb".into(),
            DocumentId(11),
            ImportLimits::default(),
            &Cancellation::default(),
        )
        .unwrap();
        let snapshot = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        assert_eq!(snapshot.positions[0], [2., 0., 0.]);
        assert_eq!(snapshot.indices, [0, 1, 2]);
        assert_eq!(snapshot.draws[0].base_color, [0.25, 0.6, 0.9, 1.]);
        assert_eq!(&**doc.source(), before);
        assert!(Arc::ptr_eq(doc.source(), &source));
    }
    #[test]
    fn truncated_container_is_an_explicit_error() {
        let data = bytes(&json_doc());
        assert!(
            import_glb(
                Arc::from(&data[..data.len() - 3]),
                "bad".into(),
                DocumentId(1),
                ImportLimits::default(),
                &Cancellation::default()
            )
            .is_err()
        );
    }
    #[test]
    fn external_buffer_is_rejected_without_reading_it() {
        let mut json = json_doc();
        json["buffers"][0]["uri"] = json!("https://example.invalid/missing.bin");
        assert!(matches!(
            load(&json),
            Err(AssetError {
                code: ErrorCode::Unsupported,
                ..
            })
        ));
    }
    #[test]
    fn required_extensions_and_morphs_are_not_silently_dropped() {
        let mut json = json_doc();
        json["extensionsRequired"] = json!(["EXT_not_supported"]);
        json["extensionsUsed"] = json!(["EXT_not_supported"]);
        assert!(matches!(
            load(&json),
            Err(AssetError {
                code: ErrorCode::Unsupported,
                ..
            })
        ));
        let mut json = json_doc();
        json["meshes"][0]["primitives"][0]["targets"] = json!([{"POSITION":0}]);
        assert!(matches!(
            load(&json),
            Err(AssetError {
                code: ErrorCode::Unsupported,
                ..
            })
        ));
    }
    #[test]
    fn resource_caps_and_cancellation_refuse_before_publication() {
        let source = bytes(&json_doc());
        for limits in [
            ImportLimits {
                source_bytes: 1,
                ..Default::default()
            },
            ImportLimits {
                json_bytes: 1,
                ..Default::default()
            },
            ImportLimits {
                decoded_bytes: 1,
                ..Default::default()
            },
        ] {
            assert!(matches!(
                import_glb(
                    source.clone(),
                    "x".into(),
                    DocumentId(1),
                    limits,
                    &Cancellation::default()
                ),
                Err(AssetError {
                    code: ErrorCode::ResourceLimit,
                    ..
                })
            ));
        }
        let cancel = Cancellation::default();
        cancel.cancel();
        assert!(matches!(
            import_glb(
                source,
                "x".into(),
                DocumentId(1),
                ImportLimits::default(),
                &cancel
            ),
            Err(AssetError {
                code: ErrorCode::Cancelled,
                ..
            })
        ));
    }
    #[test]
    fn invalid_accessor_range_is_rejected() {
        let mut json = json_doc();
        json["accessors"][0]["count"] = json!(9999);
        assert!(load(&json).is_err());
    }
    #[test]
    fn transparent_materials_are_explicitly_unsupported_until_alpha_rendering_exists() {
        for mode in ["BLEND", "MASK"] {
            let mut json = json_doc();
            json["materials"][0]["alphaMode"] = json!(mode);
            assert!(matches!(
                load(&json),
                Err(AssetError {
                    code: ErrorCode::Unsupported,
                    ..
                })
            ));
        }
    }
    #[test]
    fn malformed_position_reference_and_index_layout_are_errors_not_panics() {
        let mut bad = json_doc();
        bad["meshes"][0]["primitives"][0]["attributes"]["POSITION"] = json!(999);
        assert!(load(&bad).is_err());
        for (kind, dimensions) in [(5126, "SCALAR"), (5122, "SCALAR"), (5123, "VEC2")] {
            let mut bad = json_doc();
            bad["accessors"][1]["componentType"] = json!(kind);
            bad["accessors"][1]["type"] = json!(dimensions);
            assert!(load(&bad).is_err());
        }
        let mut bad = json_doc();
        bad["accessors"][0]["type"] = json!("VEC4");
        assert!(load(&bad).is_err());
    }
    #[test]
    fn repeated_mesh_nodes_share_primitive_ids_and_snapshot_refuses_draw_expansion() {
        let mut json = json_doc();
        json["nodes"] = json!([{"mesh":0}, {"mesh":0}]);
        json["scenes"][0]["nodes"] = json!([0, 1]);
        let document = load(&json).unwrap();
        assert!(Arc::ptr_eq(
            &document.nodes()[0].meshes,
            &document.nodes()[1].meshes
        ));
        let limits = ImportLimits {
            draws: 1,
            ..Default::default()
        };
        assert!(matches!(
            document.snapshot(limits, &Cancellation::default()),
            Err(AssetError {
                code: ErrorCode::ResourceLimit,
                ..
            })
        ));
    }
}
