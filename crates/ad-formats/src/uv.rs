use ad_document::{AssetError, AssetResult, Cancellation, ErrorCode, UvChannel, source_uv_bytes};

pub(crate) fn read_channels(
    primitive: &gltf::Primitive<'_>,
    bin: &[u8],
    vertices: usize,
    cancel: &Cancellation,
) -> AssetResult<Vec<UvChannel>> {
    let count = primitive
        .attributes()
        .filter(|(s, _)| matches!(s, gltf::Semantic::TexCoords(_)))
        .count();
    source_uv_bytes(vertices, count)?;
    let budget = || {
        AssetError::new(
            ErrorCode::ResourceLimit,
            "Cannot allocate source UV channels",
        )
    };
    let reader = primitive.reader(|_| Some(bin));
    let mut channels = Vec::new();
    channels.try_reserve_exact(count).map_err(|_| budget())?;
    for (semantic, accessor) in primitive.attributes() {
        let gltf::Semantic::TexCoords(set) = semantic else {
            continue;
        };
        cancel.check()?;
        crate::color::validate_attribute(accessor, vertices, false)?;
        let mut values = Vec::new();
        values.try_reserve_exact(vertices).map_err(|_| budget())?;
        for (i, value) in reader
            .read_tex_coords(set)
            .ok_or_else(|| {
                AssetError::new(ErrorCode::InvalidFormat, "Cannot read source UV channel")
            })?
            .into_f32()
            .enumerate()
        {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            if value.iter().any(|v| !v.is_finite()) {
                return Err(AssetError::new(
                    ErrorCode::InvalidFormat,
                    "Source UV coordinates must be finite",
                ));
            }
            values.push(value);
        }
        channels.push(UvChannel { set, values });
    }
    channels.sort_unstable_by_key(|c| c.set);
    Ok(channels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ad_document::{DocumentId, ImportLimits, geometry_bytes};
    use serde_json::{Value, json};
    use std::sync::Arc;

    fn fixture() -> (Value, Vec<u8>) {
        let bytes = include_bytes!("../../../tests/fixtures/MR_Checker.glb");
        let size = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let mut json: Value = serde_json::from_slice(&bytes[20..20 + size]).unwrap();
        for material in json["materials"].as_array_mut().unwrap() {
            let pbr = material["pbrMetallicRoughness"].as_object_mut().unwrap();
            pbr.remove("baseColorTexture");
            pbr.remove("metallicRoughnessTexture");
        }
        (json, bytes[28 + size..].to_vec())
    }
    fn encode(mut json: Value, mut bin: Vec<u8>) -> Arc<[u8]> {
        json["buffers"][0]["byteLength"] = json!(bin.len());
        let mut text = serde_json::to_vec(&json).unwrap();
        while !text.len().is_multiple_of(4) {
            text.push(b' ');
        }
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let mut bytes = Vec::new();
        for word in [
            0x46546c67u32,
            2,
            (28 + text.len() + bin.len()) as u32,
            text.len() as u32,
            0x4e4f534a,
        ] {
            bytes.extend(word.to_le_bytes());
        }
        bytes.extend(text);
        bytes.extend((bin.len() as u32).to_le_bytes());
        bytes.extend(0x004e4942u32.to_le_bytes());
        bytes.extend(bin);
        bytes.into()
    }
    fn load(bytes: Arc<[u8]>, limits: ImportLimits) -> AssetResult<ad_document::AssetDocument> {
        crate::import_glb(
            bytes,
            "uv.glb".into(),
            DocumentId(1),
            limits,
            &Cancellation::default(),
        )
    }
    #[test]
    fn unselected_source_channels_survive_with_exact_byte_admission() {
        let (json, bin) = fixture();
        let bytes = encode(json, bin);
        let doc = load(bytes.clone(), ImportLimits::default()).unwrap();
        assert_eq!(doc.source().as_ref(), bytes.as_ref());
        let mut required = 0;
        for mesh in doc.primitives() {
            assert_eq!(
                mesh.uv_channels.iter().map(|c| c.set).collect::<Vec<_>>(),
                [0, 1]
            );
            assert_eq!(mesh.uv_bindings, ad_document::UvBindings::default());
            assert!(mesh.color_texture.is_none() && mesh.metallic_roughness_texture.is_none());
            required += geometry_bytes(mesh.positions.len(), mesh.indices.len()).unwrap()
                + source_uv_bytes(mesh.positions.len(), 2).unwrap();
        }
        let mut limits = ImportLimits {
            decoded_bytes: required,
            ..ImportLimits::default()
        };
        load(bytes.clone(), limits).unwrap();
        limits.decoded_bytes -= 1;
        assert_eq!(
            load(bytes, limits).err().unwrap().code,
            ErrorCode::ResourceLimit
        );
    }
    #[test]
    fn normalized_unused_integer_sets_keep_their_source_ids() {
        for (component, value, expected) in [
            (5121, vec![128, 64, 0, 0], [128. / 255., 64. / 255.]),
            (
                5123,
                vec![0, 128, 0, 64],
                [32768. / 65535., 16384. / 65535.],
            ),
        ] {
            let (mut json, mut bin) = fixture();
            let count = json["accessors"][0]["count"].as_u64().unwrap() as usize;
            let offset = bin.len();
            for _ in 0..count {
                bin.extend(&value);
            }
            let view = json["bufferViews"].as_array().unwrap().len();
            json["bufferViews"].as_array_mut().unwrap().push(json!({
                "buffer":0,"byteOffset":offset,"byteLength":count*4,"byteStride":4}));
            let accessor = json["accessors"].as_array().unwrap().len();
            json["accessors"].as_array_mut().unwrap().push(json!({
                "bufferView":view,"componentType":component,"normalized":true,"count":count,"type":"VEC2"}));
            json["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_2"] = json!(accessor);
            let doc = load(encode(json, bin), ImportLimits::default()).unwrap();
            let mesh = &doc.primitives()[0];
            assert_eq!(mesh.source_uv(2).unwrap()[0], expected);
            assert_eq!(mesh.uv_bindings, ad_document::UvBindings::default());
        }
    }
    #[test]
    fn malformed_unselected_uvs_and_channel_overflow_are_explicit_errors() {
        let (mut json, mut bin) = fixture();
        let accessor = json["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"]
            .as_u64()
            .unwrap() as usize;
        let view = json["accessors"][accessor]["bufferView"].as_u64().unwrap() as usize;
        let offset = json["bufferViews"][view]["byteOffset"]
            .as_u64()
            .unwrap_or(0) as usize
            + json["accessors"][accessor]["byteOffset"]
                .as_u64()
                .unwrap_or(0) as usize;
        bin[offset..offset + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(
            load(encode(json.clone(), bin.clone()), ImportLimits::default())
                .err()
                .unwrap()
                .code,
            ErrorCode::InvalidFormat
        );
        // Channel admission precedes UV decoding, including the NaN above.
        for set in 2..9 {
            json["meshes"][0]["primitives"][0]["attributes"][format!("TEXCOORD_{set}")] =
                json!(accessor);
        }
        assert_eq!(
            load(encode(json, bin), ImportLimits::default())
                .err()
                .unwrap()
                .code,
            ErrorCode::ResourceLimit
        );
    }
}
