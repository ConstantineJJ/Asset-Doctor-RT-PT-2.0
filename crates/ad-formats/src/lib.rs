//! Self-contained file format boundary. No OS dialogs, network or Vulkan.
mod color;
mod export;
mod metadata;
mod roundtrip;
mod uv;
pub use color::decode_image_file;
mod hdr;
pub use export::{
    ExportLimits, VerifiedGlb, preflight_changed_export, prepare_glb, verify_animation_export,
};
pub use hdr::{HdrImage, decode_hdr_file, is_hdr};

/// Independent comparison of a freshly reimported document with the document
/// it was saved from: nodes, roots, transforms, every source primitive's
/// indices/attributes/UV sets and bindings, core materials, samplers, images
/// and texture statistics. Used after a Save Copy is read back from disk.
pub fn compare_reimported(
    original: &ad_document::AssetDocument,
    reimported: &ad_document::AssetDocument,
    cancel: &ad_document::Cancellation,
) -> ad_document::AssetResult<()> {
    if original.is_read_only() || reimported.is_read_only() {
        return export::compare_rig_readback(&original.view(), reimported, cancel);
    }
    let roots = export::expected_roots(original, ad_document::ImportLimits::default(), cancel)?;
    roundtrip::compare(original, reimported, &roots, cancel)
}
#[cfg(test)]
mod export_tests;
#[cfg(test)]
mod export_vertex_tests;
mod morph;
#[cfg(test)]
mod morph_tests;
#[cfg(test)]
mod rig_tests;
mod skin;
mod skin_animation;
mod source;
use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentId, DocumentParts, ErrorCode,
    ImportLimits, MeshId, MeshPrimitive, Node, NodeId, TextureEncoding, UvBindings, geometry_bytes,
    source_uv_bytes,
};
use std::sync::Arc;

pub fn import_glb(
    source: Arc<[u8]>,
    name: String,
    id: DocumentId,
    limits: ImportLimits,
    cancel: &Cancellation,
) -> AssetResult<AssetDocument> {
    let invalid = |message: &str| AssetError::new(ErrorCode::InvalidFormat, message);
    let unsupported = |message: &str| AssetError::new(ErrorCode::Unsupported, message);
    let budget = || {
        AssetError::new(
            ErrorCode::ResourceLimit,
            "Import exceeds the configured resource budget",
        )
    };
    let parsed = source::parse(&source, limits, cancel)?;
    let doc = parsed.document;
    let bin = parsed
        .glb
        .bin
        .as_deref()
        .ok_or_else(|| invalid("GLB has no embedded BIN geometry"))?;
    let skinned_meshes: std::collections::HashSet<_> = doc
        .nodes()
        .filter(|node| node.skin().is_some())
        .filter_map(|node| node.mesh().map(|mesh| mesh.index()))
        .collect();
    let mut remaining = limits.decoded_bytes;
    let skins = doc
        .skins()
        .map(|skin| skin::read_skin(skin, bin, &mut remaining, cancel))
        .collect::<AssetResult<Vec<_>>>()?;
    let clips = doc
        .animations()
        .map(|clip| skin_animation::read_clip(clip, bin, &mut remaining, cancel))
        .collect::<AssetResult<Vec<_>>>()?;
    // ADR-0065: morph targets per primitive, in document order.
    let mut morph_primitives = Vec::new();
    let mut influences = Vec::new();
    let mut meshes = Vec::new();
    let mut mesh_ids = Vec::new();
    let mut decoded = 0usize;
    let mut material_images = color::MaterialImages::default();
    // Material textures whose UV set the primitive does not have (an export
    // slip): left out of that primitive, with a note.
    let mut textures_without_uv = 0usize;
    for mesh in doc.meshes() {
        let mut ids = Vec::new();
        for primitive in mesh.primitives() {
            cancel.check()?;
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                return Err(unsupported("This build supports triangle primitives"));
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
            let uv_count = primitive
                .attributes()
                .filter(|(semantic, _)| matches!(semantic, gltf::Semantic::TexCoords(_)))
                .count();
            let uv_bytes = source_uv_bytes(position.count(), uv_count)?;
            decoded = decoded
                .checked_add(geometry_bytes(position.count(), index_count)?)
                .and_then(|v| v.checked_add(uv_bytes))
                .ok_or_else(budget)?;
            let skinned = skinned_meshes.contains(&mesh.index());
            if skinned {
                decoded = position
                    .count()
                    .checked_mul(size_of::<ad_document::Influence>())
                    .and_then(|bytes| decoded.checked_add(bytes))
                    .ok_or_else(budget)?;
            }
            if decoded > remaining || position.count() > u32::MAX as usize {
                return Err(budget());
            }
            if !skins.is_empty() {
                influences.push(
                    skinned
                        .then(|| skin::read_influences(&primitive, bin, position.count(), cancel))
                        .transpose()?,
                );
            }
            let reader = primitive.reader(|_| Some(bin));
            let mut tangents = None;
            if let Some(accessor) = primitive.get(&gltf::Semantic::Tangents) {
                if accessor.data_type() != gltf::accessor::DataType::F32
                    || accessor.dimensions() != gltf::accessor::Dimensions::Vec4
                    || accessor.normalized()
                    || accessor.count() != position.count()
                {
                    return Err(invalid(
                        "Tangents must use matching non-normalized F32 VEC4 accessors",
                    ));
                }
                let mut values = Vec::new();
                values
                    .try_reserve_exact(position.count())
                    .map_err(|_| budget())?;
                for (i, value) in reader
                    .read_tangents()
                    .ok_or_else(|| invalid("Cannot read tangents"))?
                    .enumerate()
                {
                    if i % 4096 == 0 {
                        cancel.check()?;
                    }
                    values.push(value);
                }
                tangents = Some(values);
            }
            let mut normals = None;
            if let Some(accessor) = primitive.get(&gltf::Semantic::Normals) {
                if accessor.data_type() != gltf::accessor::DataType::F32
                    || accessor.dimensions() != gltf::accessor::Dimensions::Vec3
                    || accessor.normalized()
                    || accessor.count() != position.count()
                {
                    return Err(invalid(
                        "Normals must use matching non-normalized F32 VEC3 accessors",
                    ));
                }
                let mut values = Vec::new();
                values
                    .try_reserve_exact(position.count())
                    .map_err(|_| budget())?;
                for (i, value) in reader
                    .read_normals()
                    .ok_or_else(|| invalid("Cannot read normals"))?
                    .enumerate()
                {
                    if i % 4096 == 0 {
                        cancel.check()?;
                    }
                    values.push(value);
                }
                normals = Some(values);
            }
            let uv_channels = uv::read_channels(&primitive, bin, position.count(), cancel)?;
            let has_set = |set: u32| uv_channels.iter().any(|channel| channel.set == set);
            let mut dropped = 0usize;
            let mut keep = |set: u32| {
                let kept = has_set(set);
                dropped += usize::from(!kept);
                kept
            };
            let color_texture = primitive
                .material()
                .pbr_metallic_roughness()
                .base_color_texture()
                .filter(|i| keep(i.tex_coord()));
            let mr_texture = primitive
                .material()
                .pbr_metallic_roughness()
                .metallic_roughness_texture()
                .filter(|i| keep(i.tex_coord()));
            let normal_info = primitive
                .material()
                .normal_texture()
                .filter(|i| keep(i.tex_coord()));
            let emissive_info = primitive
                .material()
                .emissive_texture()
                .filter(|i| keep(i.tex_coord()));
            let emissive_factor = primitive.material().emissive_factor();
            let occlusion_info = primitive
                .material()
                .occlusion_texture()
                .filter(|i| keep(i.tex_coord()));
            textures_without_uv += dropped;
            let occlusion_strength = occlusion_info.as_ref().map_or(1., |info| info.strength());
            let normal_scale = normal_info.as_ref().map_or(1., |info| info.scale());
            let uv_bindings = UvBindings {
                color: color_texture.as_ref().map(|i| i.tex_coord()),
                metallic_roughness: mr_texture.as_ref().map(|i| i.tex_coord()),
                normal: normal_info.as_ref().map(|i| i.tex_coord()),
                occlusion: occlusion_info.as_ref().map(|i| i.tex_coord()),
                emissive: emissive_info.as_ref().map(|i| i.tex_coord()),
            };
            if uv_bindings
                .sets()
                .into_iter()
                .flatten()
                .any(|set| !uv_channels.iter().any(|channel| channel.set == set))
            {
                return Err(invalid(
                    "Material texture needs its declared TEXCOORD attribute",
                ));
            }
            let mut colors = None;
            if let Some(accessor) = primitive.get(&gltf::Semantic::Colors(0)) {
                color::validate_attribute(accessor, position.count(), true)?;
                let mut values = Vec::new();
                values
                    .try_reserve_exact(position.count())
                    .map_err(|_| budget())?;
                for (i, value) in reader
                    .read_colors(0)
                    .ok_or_else(|| invalid("Cannot read vertex colors"))?
                    .into_rgba_f32()
                    .enumerate()
                {
                    if i % 4096 == 0 {
                        cancel.check()?;
                    }
                    values.push(value);
                }
                colors = Some(values);
            }
            let color_texture = color_texture
                .map(|info| {
                    material_images.load(info.texture(), TextureEncoding::Srgb, bin, limits, cancel)
                })
                .transpose()?;
            let metallic_roughness_texture = mr_texture
                .map(|info| {
                    material_images.load(
                        info.texture(),
                        TextureEncoding::Linear,
                        bin,
                        limits,
                        cancel,
                    )
                })
                .transpose()?;
            let normal_texture = normal_info
                .map(|info| {
                    material_images.load(
                        info.texture(),
                        TextureEncoding::Linear,
                        bin,
                        limits,
                        cancel,
                    )
                })
                .transpose()?;
            let emissive_texture = emissive_info
                .map(|info| {
                    material_images.load(info.texture(), TextureEncoding::Srgb, bin, limits, cancel)
                })
                .transpose()?;
            let alpha_cutoff = (primitive.material().alpha_mode()
                == gltf::material::AlphaMode::Mask)
                .then(|| primitive.material().alpha_cutoff().unwrap_or(0.5));
            let blend = primitive.material().alpha_mode() == gltf::material::AlphaMode::Blend;
            let occlusion_texture = occlusion_info
                .map(|info| {
                    material_images.load(
                        info.texture(),
                        TextureEncoding::Linear,
                        bin,
                        limits,
                        cancel,
                    )
                })
                .transpose()?;
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
            morph_primitives.push(morph::read_targets(
                &mesh,
                &primitive,
                bin,
                position.count(),
                &mut decoded,
                remaining,
                cancel,
            )?);
            ids.push(MeshId(meshes.len()));
            meshes.push(MeshPrimitive {
                positions,
                normals,
                tangents,
                normal_texture,
                normal_scale,
                uv_channels,
                uv_bindings,
                occlusion_texture,
                occlusion_strength,
                emissive_factor,
                emissive_texture,
                metallic_roughness: [
                    primitive
                        .material()
                        .pbr_metallic_roughness()
                        .metallic_factor(),
                    primitive
                        .material()
                        .pbr_metallic_roughness()
                        .roughness_factor(),
                ],
                indices,
                base_color: primitive
                    .material()
                    .pbr_metallic_roughness()
                    .base_color_factor(),
                colors,
                color_texture,
                metallic_roughness_texture,
                alpha_cutoff,
                blend,
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
    let mut notes = vec!["Обе стороны граней видимы. PBR использует параметры и карты металла/шероховатости без окружения и теней.".into()];
    if doc.materials().any(|m| m.normal_texture().is_some()) {
        notes.push("Карты нормалей используют авторские касательные, иначе UV-основу по производным. Это приближение без гарантии совпадения с MikkTSpace на швах.".into());
    }
    if doc
        .extensions_used()
        .any(|name| name == "KHR_materials_emissive_strength")
    {
        notes.push("Усиление свечения KHR_materials_emissive_strength не отображается; используются только основные параметры glTF.".into());
    }
    if doc.scenes().len() > 1 {
        notes.push("Отображается только основная/первая сцена.".into());
    }
    if doc
        .meshes()
        .any(|m| m.primitives().any(|p| p.morph_targets().len() > 0))
    {
        notes.push("Морф-цели показываются с весами из файла и клипов; геометрию таких моделей не правят, при сохранении цели остаются как в файле.".into());
    }
    if textures_without_uv > 0 {
        notes.push(format!(
            "{textures_without_uv} текстур(ы) материалов ссылаются на UV-набор, которого нет у примитива; они не показываются."
        ));
    }
    if doc
        .extensions_used()
        .any(|name| name == "KHR_lights_punctual")
    {
        notes.push("Источники света из файла (KHR_lights_punctual) не показываются; сцену освещает лампа программы.".into());
    }
    if let Ok(mut raw) = serde_json::from_slice::<serde_json::Value>(&parsed.glb.json)
        && source::negative_tex_coords(&mut raw, cancel).is_ok_and(|n| n > 0)
    {
        notes.push(
            "В файле есть texCoord = -1 (ошибка экспорта); такие ссылки прочитаны как UV-набор 0."
                .into(),
        );
    }
    cancel.check()?;
    let bases = if clips.is_empty() && skins.is_empty() {
        Vec::new()
    } else {
        doc.nodes()
            .map(|node| match node.transform() {
                gltf::scene::Transform::Decomposed {
                    translation,
                    rotation,
                    scale,
                } => Some(ad_document::Transform {
                    translation,
                    rotation,
                    scale,
                }),
                gltf::scene::Transform::Matrix { .. } => None,
            })
            .collect()
    };
    let document = AssetDocument::new(
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
    )?
    .with_animation_clips(clips, bases)?
    .with_morphs(
        ad_document::Morphs {
            primitives: morph_primitives,
            nodes: doc
                .nodes()
                .map(|node| node.weights().map(std::sync::Arc::from))
                .collect(),
        },
        cancel,
    )?;
    if skins.is_empty() {
        Ok(document)
    } else {
        document.with_rig(
            ad_document::Rig {
                skins,
                node_skins: doc
                    .nodes()
                    .map(|node| node.skin().map(|skin| skin.index()))
                    .collect(),
                influences,
            },
            limits,
            cancel,
        )
    }
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
    fn emission_preserves_srgb_image_identity_independent_uvs_factor_only_and_mask() {
        let source: Arc<[u8]> =
            Arc::from(include_bytes!("../../../tests/fixtures/Emission_Checker.glb").as_slice());
        let doc = import_glb(
            source.clone(),
            "emission".into(),
            DocumentId(1),
            ImportLimits::default(),
            &Cancellation::default(),
        )
        .unwrap();
        let scene = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        assert!(Arc::ptr_eq(doc.source(), &source));
        assert_eq!(scene.emissive_uvs[0], [0., 0.]);
        assert_ne!(scene.emissive_uvs[0], scene.uvs[0]);
        assert_eq!(
            (
                doc.texture_stats().images,
                doc.texture_stats().sampled_images,
                doc.texture_stats().mip_bytes
            ),
            (1, 2, 40)
        );
        assert!(Arc::ptr_eq(
            scene.draws[0].emissive_texture.as_ref().unwrap(),
            scene.draws[0].color_texture.as_ref().unwrap()
        ));
        assert_eq!(scene.draws[0].emissive_factor, [1., 0.5, 0.25]);
        assert_eq!(scene.draws[1].emissive_factor, [0.; 3]);
        assert!(scene.draws[2].emissive_texture.is_none());
        assert_eq!(scene.draws[2].emissive_factor, [0.2, 0.6, 1.]);
        assert_eq!(scene.draws[3].alpha_cutoff, Some(0.5));
        assert!(matches!(
            import_glb(
                Arc::from(
                    include_bytes!("../../../tests/fixtures/Bad_Emission_Image.glb").as_slice()
                ),
                "bad emission".into(),
                DocumentId(1),
                ImportLimits::default(),
                &Cancellation::default()
            ),
            Err(AssetError {
                code: ErrorCode::InvalidFormat,
                ..
            })
        ));
        assert_eq!(
            load(&json_doc())
                .unwrap()
                .snapshot(ImportLimits::default(), &Cancellation::default())
                .unwrap()
                .draws[0]
                .emissive_factor,
            [0.; 3]
        );
    }
    #[test]
    fn malformed_emission_only_map_is_admitted_instead_of_silently_ignored() {
        let mut json = json_doc();
        json["accessors"]
            .as_array_mut()
            .unwrap()
            .push(json!({"bufferView":0,"componentType":5126,"count":3,"type":"VEC2"}));
        json["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"] = json!(2);
        json["images"] = json!([{"bufferView":0,"mimeType":"image/png"}]);
        json["textures"] = json!([{"source":0}]);
        json["materials"][0]["emissiveTexture"] = json!({"index":0});
        json["materials"][0]["emissiveFactor"] = json!([1., 0.5, 0.25]);
        assert!(matches!(
            load(&json),
            Err(AssetError {
                code: ErrorCode::InvalidFormat,
                ..
            })
        ));
    }
    #[test]
    fn ao_uses_independent_linear_uvs_shared_orm_image_and_source_strength() {
        let source: Arc<[u8]> =
            Arc::from(include_bytes!("../../../tests/fixtures/AO_Checker.glb").as_slice());
        let doc = import_glb(
            source.clone(),
            "AO".into(),
            DocumentId(1),
            ImportLimits::default(),
            &Cancellation::default(),
        )
        .unwrap();
        let scene = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        assert!(Arc::ptr_eq(doc.source(), &source));
        assert_eq!(scene.occlusion_uvs[0], [1., 0.]);
        assert_ne!(scene.occlusion_uvs[0], scene.metallic_roughness_uvs[0]);
        assert_eq!(
            scene
                .draws
                .iter()
                .map(|d| d.occlusion_strength)
                .collect::<Vec<_>>(),
            [1., 0.5, 0., 1.]
        );
        assert!(Arc::ptr_eq(
            scene.draws[0].occlusion_texture.as_ref().unwrap(),
            scene.draws[0].metallic_roughness_texture.as_ref().unwrap()
        ));
        assert_eq!(
            (
                doc.texture_stats().images,
                doc.texture_stats().sampled_images,
                doc.texture_stats().mip_bytes
            ),
            (1, 2, 40)
        );
        assert!(matches!(
            import_glb(
                Arc::from(include_bytes!("../../../tests/fixtures/Bad_AO_Image.glb").as_slice()),
                "bad AO".into(),
                DocumentId(1),
                ImportLimits::default(),
                &Cancellation::default()
            ),
            Err(AssetError {
                code: ErrorCode::InvalidFormat,
                ..
            })
        ));
        let mut json = json_doc();
        json["images"] = json!([{"bufferView":0,"mimeType":"image/png"}]);
        json["textures"] = json!([{"source":0}]);
        json["materials"][0]["occlusionTexture"] = json!({"index":0,"texCoord":2});
        texture_without_uv_is_left_out(&json);
    }
    #[test]
    fn occlusion_only_bad_image_is_not_silently_ignored() {
        let mut json = json_doc();
        json["accessors"]
            .as_array_mut()
            .unwrap()
            .push(json!({"bufferView":0,"componentType":5126,"count":3,"type":"VEC2"}));
        json["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"] = json!(2);
        json["images"] = json!([{"bufferView":0,"mimeType":"image/png"}]);
        json["textures"] = json!([{"source":0}]);
        json["materials"][0]["occlusionTexture"] = json!({"index":0});
        assert!(matches!(
            load(&json),
            Err(AssetError {
                code: ErrorCode::InvalidFormat,
                ..
            })
        ));
    }
    #[test]
    fn normal_texture_shares_linear_mr_image_and_preserves_tangent_uv_scale_contract() {
        let source: Arc<[u8]> =
            Arc::from(include_bytes!("../../../tests/fixtures/Normal_Checker.glb").as_slice());
        let doc = import_glb(
            source.clone(),
            "normals".into(),
            DocumentId(1),
            ImportLimits::default(),
            &Cancellation::default(),
        )
        .unwrap();
        let scene = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        assert!(Arc::ptr_eq(doc.source(), &source));
        assert_eq!(
            (
                doc.texture_stats().images,
                doc.texture_stats().sampled_images,
                doc.texture_stats().mip_bytes
            ),
            (1, 1, 20)
        );
        assert_eq!(scene.normal_uvs[0], [1., 1.]);
        assert_ne!(scene.normal_uvs[0], scene.metallic_roughness_uvs[0]);
        assert_eq!(scene.tangents[0], [0., 0., -1., 1.]);
        assert_eq!(scene.fallback_tangent_vertices, 561 * 3);
        assert_eq!(scene.tangents[561], [0.; 4]);
        assert_eq!(scene.tangents[561 * 3], [0.; 4]);
        assert_eq!(
            scene
                .draws
                .iter()
                .map(|d| d.normal_scale)
                .collect::<Vec<_>>(),
            [1., 0.5, 0., -1.]
        );
        assert!(Arc::ptr_eq(
            scene.draws[0].normal_texture.as_ref().unwrap(),
            scene.draws[0].metallic_roughness_texture.as_ref().unwrap()
        ));
        assert!(doc.notes().iter().any(|n| n.contains("MikkTSpace")));
    }
    #[test]
    fn malformed_normal_images_tangents_and_missing_normal_uvs_are_rejected() {
        assert!(matches!(
            import_glb(
                Arc::from(
                    include_bytes!("../../../tests/fixtures/Bad_Normal_Image.glb").as_slice()
                ),
                "bad normal".into(),
                DocumentId(1),
                ImportLimits::default(),
                &Cancellation::default()
            ),
            Err(AssetError {
                code: ErrorCode::InvalidFormat,
                ..
            })
        ));
        let mut json = json_doc();
        for index in [0, 1, 999] {
            json["meshes"][0]["primitives"][0]["attributes"]["TANGENT"] = json!(index);
            assert!(load(&json).is_err());
        }
        json["meshes"][0]["primitives"][0]["attributes"]
            .as_object_mut()
            .unwrap()
            .remove("TANGENT");
        json["images"] = json!([{"bufferView":0,"mimeType":"image/png"}]);
        json["textures"] = json!([{"source":0}]);
        json["materials"][0]["normalTexture"] = json!({"index":0,"texCoord":2});
        texture_without_uv_is_left_out(&json);
    }
    #[test]
    fn authored_normals_and_material_factors_survive_import_and_bad_layouts_are_refused() {
        let mut json = json_doc();
        json["meshes"][0]["primitives"][0]["attributes"]["NORMAL"] = json!(0);
        json["materials"][0]["pbrMetallicRoughness"]["metallicFactor"] = json!(0.2);
        json["materials"][0]["pbrMetallicRoughness"]["roughnessFactor"] = json!(0.7);
        let doc = load(&json).unwrap();
        let snapshot = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        assert_eq!(snapshot.normals, [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]);
        assert_eq!(snapshot.fallback_normal_vertices, 1);
        assert_eq!(snapshot.draws[0].metallic_roughness, [0.2, 0.7]);
        for bad_index in [1, 999] {
            json["meshes"][0]["primitives"][0]["attributes"]["NORMAL"] = json!(bad_index);
            assert!(load(&json).is_err());
        }
        let scene = load(&json_doc())
            .unwrap()
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        assert_eq!(scene.draws[0].metallic_roughness, [1.; 2]);
        assert_eq!(scene.fallback_normal_vertices, scene.positions.len());
    }
    #[test]
    fn mr_texture_uses_independent_uvs_and_shares_pixels_across_transfer_roles() {
        let source: Arc<[u8]> =
            Arc::from(include_bytes!("../../../tests/fixtures/MR_Checker.glb").as_slice());
        let doc = import_glb(
            source.clone(),
            "MR".into(),
            DocumentId(1),
            ImportLimits::default(),
            &Cancellation::default(),
        )
        .unwrap();
        let scene = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        let stats = doc.texture_stats();
        assert_eq!(
            (
                stats.images,
                stats.textures,
                stats.sampled_images,
                stats.rgba_bytes,
                stats.mip_bytes
            ),
            (1, 1, 2, 16, 40)
        );
        assert_eq!(scene.uvs[0], [0.25, 0.25]);
        assert_eq!(scene.metallic_roughness_uvs[0], [0., 1.]);
        assert!(Arc::ptr_eq(
            scene.draws[0].color_texture.as_ref().unwrap(),
            scene.draws[0].metallic_roughness_texture.as_ref().unwrap()
        ));
        assert!(Arc::ptr_eq(
            scene.draws[0].metallic_roughness_texture.as_ref().unwrap(),
            scene.draws[2].metallic_roughness_texture.as_ref().unwrap()
        ));
        assert_eq!(
            &*scene.draws[0]
                .metallic_roughness_texture
                .as_ref()
                .unwrap()
                .image
                .rgba,
            &[
                255, 64, 0, 0, 0, 128, 255, 255, 128, 255, 128, 0, 255, 0, 255, 255
            ]
        );
        assert_eq!(scene.draws[1].metallic_roughness, [0.5, 0.5]);
        assert!(scene.draws[2].color_texture.is_none());
        assert!(Arc::ptr_eq(doc.source(), &source));
        for limits in [
            ImportLimits {
                texture_bytes: 39,
                ..Default::default()
            },
            ImportLimits {
                textures: 1,
                ..Default::default()
            },
        ] {
            assert!(matches!(
                import_glb(
                    source.clone(),
                    "MR".into(),
                    DocumentId(2),
                    limits,
                    &Cancellation::default()
                ),
                Err(AssetError {
                    code: ErrorCode::ResourceLimit,
                    ..
                })
            ));
        }
    }
    #[test]
    fn malformed_mr_images_missing_or_invalid_uvs_are_explicit_errors() {
        assert!(matches!(
            import_glb(
                Arc::from(include_bytes!("../../../tests/fixtures/Bad_MR_Image.glb").as_slice()),
                "bad MR".into(),
                DocumentId(1),
                ImportLimits::default(),
                &Cancellation::default()
            ),
            Err(AssetError {
                code: ErrorCode::InvalidFormat,
                ..
            })
        ));
        let mut json = json_doc();
        json["images"] = json!([{"bufferView":0,"mimeType":"image/png"}]);
        json["textures"] = json!([{"source":0}]);
        json["materials"][0]["pbrMetallicRoughness"]["metallicRoughnessTexture"] =
            json!({"index":0,"texCoord":1});
        texture_without_uv_is_left_out(&json);
        json["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_1"] = json!(0);
        assert!(load(&json).is_err());
    }
    #[test]
    fn material_fixture_preserves_factor_variation_and_optional_extension_notice() {
        let source =
            Arc::<[u8]>::from(include_bytes!("../../../tests/fixtures/Pbr_Factors.glb").as_slice());
        let doc = import_glb(
            source.clone(),
            "factors".into(),
            DocumentId(1),
            ImportLimits::default(),
            &Cancellation::default(),
        )
        .unwrap();
        let scene = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        assert_eq!(scene.fallback_normal_vertices, 6);
        assert_eq!(scene.draws[0].metallic_roughness, [0., 0.15]);
        assert_eq!(scene.draws[5].metallic_roughness, [1., 0.95]);
        assert_eq!(scene.normals[0], [0., 1., 0.]);
        assert!(Arc::ptr_eq(&source, doc.source()));
        let mut json = json_doc();
        json["images"] = json!([{"bufferView":0,"mimeType":"image/png"}]);
        json["textures"] = json!([{"source":0}]);
        json["extensionsUsed"] = json!(["KHR_materials_emissive_strength"]);
        // Ignored maps are not decoded or treated as a base-color map.
        let doc = load(&json).unwrap();
        assert_eq!(doc.texture_stats().images, 0);
        assert!(doc.notes().iter().any(|n| n.contains("не отображается")));
    }
    #[test]
    fn embedded_color_fixture_decodes_once_and_preserves_linear_attributes_and_source() {
        let source: Arc<[u8]> =
            Arc::from(include_bytes!("../../../tests/fixtures/Color_Checker.glb").as_slice());
        let doc = import_glb(
            source.clone(),
            "color".into(),
            DocumentId(1),
            ImportLimits::default(),
            &Cancellation::default(),
        )
        .unwrap();
        assert!(Arc::ptr_eq(&source, doc.source()));
        let stats = doc.texture_stats();
        assert_eq!(
            (
                stats.textures,
                stats.images,
                stats.rgba_bytes,
                stats.mip_bytes
            ),
            (2, 1, 16, 20)
        );
        let scene = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        assert_eq!(scene.uvs[0], [0., 1.]);
        assert_eq!(scene.colors[1], [0.5, 1., 0.5, 1.]);
        let first = scene.draws[0].color_texture.as_ref().unwrap();
        let second = scene.draws[1].color_texture.as_ref().unwrap();
        assert!(Arc::ptr_eq(&first.image, &second.image));
        assert_eq!(
            &*first.image.rgba,
            &[
                255, 0, 0, 255, 0, 255, 0, 0, 0, 0, 255, 255, 255, 255, 255, 0
            ]
        );
        assert_eq!(scene.draws[1].alpha_cutoff, Some(0.5));
    }
    #[test]
    fn invalid_images_and_texture_caps_fail_before_publication() {
        for (bytes, expected) in [
            (
                include_bytes!("../../../tests/fixtures/Bad_Image.glb").as_slice(),
                ErrorCode::InvalidFormat,
            ),
            (
                include_bytes!("../../../tests/fixtures/Huge_Image_Header.glb").as_slice(),
                ErrorCode::ResourceLimit,
            ),
        ] {
            let result = import_glb(
                Arc::from(bytes),
                "bad".into(),
                DocumentId(1),
                ImportLimits::default(),
                &Cancellation::default(),
            );
            assert!(matches!(result,Err(AssetError {code,..}) if code==expected));
        }
        let bytes = Arc::<[u8]>::from(
            include_bytes!("../../../tests/fixtures/Color_Checker.glb").as_slice(),
        );
        for limits in [
            ImportLimits {
                texture_bytes: 19,
                ..Default::default()
            },
            ImportLimits {
                textures: 1,
                ..Default::default()
            },
        ] {
            assert!(matches!(
                import_glb(
                    bytes.clone(),
                    "bad".into(),
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
    }
    #[test]
    fn invalid_color_and_missing_uv_layouts_do_not_panic() {
        let mut json = json_doc();
        json["meshes"][0]["primitives"][0]["attributes"]["COLOR_0"] = json!(1);
        assert!(load(&json).is_err());
        let mut json = json_doc();
        json["images"] = json!([{"bufferView":0,"mimeType":"image/png"}]);
        json["textures"] = json!([{"source":0}]);
        json["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"] = json!({"index":0});
        texture_without_uv_is_left_out(&json);
    }
    /// 2026-11-25: a material texture whose UV set the primitive lacks (an
    /// export slip) no longer refuses the model: that texture is left out
    /// and a note says so; its image is not even decoded.
    fn texture_without_uv_is_left_out(json: &Value) {
        let doc = load(json).unwrap();
        assert!(
            doc.notes()
                .iter()
                .any(|n| n.contains("UV-набор, которого нет")),
            "{:?}",
            doc.notes()
        );
        let scene = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        assert!(scene.draws.iter().all(|d| d.color_texture.is_none()
            && d.metallic_roughness_texture.is_none()
            && d.normal_texture.is_none()
            && d.occlusion_texture.is_none()
            && d.emissive_texture.is_none()));
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
        // ADR-0065: a morph target opens (here it moves each vertex by its
        // own position, so weight 0.5 scales the triangle by 1.5).
        let mut json = json_doc();
        json["meshes"][0]["primitives"][0]["targets"] = json!([{"POSITION":0}]);
        json["meshes"][0]["weights"] = json!([0.5]);
        let doc = load(&json).unwrap();
        assert!(doc.is_read_only(), "morphing geometry is not edited");
        let scene = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        assert_eq!(scene.positions[1], [3.5, 0., 0.]);
        // A target with a quantized accessor is refused, not misread.
        let mut json = json_doc();
        json["meshes"][0]["primitives"][0]["targets"] = json!([{"POSITION":1}]);
        assert!(load(&json).is_err());
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
    fn blend_materials_lights_and_negative_tex_coords_open() {
        let c = Cancellation::default();
        // BLEND: opens, marked for the blended pass.
        let mut json = json_doc();
        json["materials"][0]["alphaMode"] = json!("BLEND");
        json["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"] = json!([1, 1, 1, 0.4]);
        let doc = load(&json).unwrap();
        let scene = doc.snapshot(ImportLimits::default(), &c).unwrap();
        assert!(
            scene
                .draws
                .iter()
                .all(|d| d.blend && d.alpha_cutoff.is_none())
        );
        assert!((scene.draws[0].base_color[3] - 0.4).abs() < 1e-6);
        // A required KHR_lights_punctual is left out with a note; another
        // required extension is still refused.
        let mut lit = json_doc();
        lit["extensionsUsed"] = json!(["KHR_lights_punctual"]);
        lit["extensionsRequired"] = json!(["KHR_lights_punctual"]);
        let doc = load(&lit).unwrap();
        assert!(
            doc.notes()
                .iter()
                .any(|n| n.contains("KHR_lights_punctual"))
        );
        let mut other = json_doc();
        other["extensionsUsed"] = json!(["EXT_unknown"]);
        other["extensionsRequired"] = json!(["EXT_unknown"]);
        assert!(matches!(
            load(&other),
            Err(AssetError {
                code: ErrorCode::Unsupported,
                ..
            })
        ));
        // texCoord -1 (an exporter slip) reads as set 0, with a note.
        let mut slip = json_doc();
        slip["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"] =
            json!({"index": 0, "texCoord": -1});
        if slip.get("textures").is_some() {
            let doc = load(&slip).unwrap();
            assert!(doc.notes().iter().any(|n| n.contains("texCoord")));
        } else {
            // The base fixture has no texture: the slip is still read.
            let mut metadata = slip.clone();
            assert_eq!(source::negative_tex_coords(&mut metadata, &c).unwrap(), 1);
            assert_eq!(
                metadata["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"]["texCoord"],
                json!(0)
            );
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
