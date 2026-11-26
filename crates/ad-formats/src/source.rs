//! Shared bounded GLB metadata/container preflight. No decoded asset allocation.
use ad_document::{AssetError, AssetResult, Cancellation, ErrorCode, ImportLimits};

pub(crate) struct Parsed<'a> {
    pub document: gltf::Document,
    pub glb: gltf::binary::Glb<'a>,
}
pub(crate) fn parse_for_export<'a>(
    source: &'a [u8],
    limits: ImportLimits,
    cancel: &Cancellation,
) -> AssetResult<Parsed<'a>> {
    let parsed = parse(source, limits, cancel)?;
    if parsed.document.skins().len() > 0 || parsed.document.animations().len() > 0 {
        return Err(AssetError::new(
            ErrorCode::Unsupported,
            "Экспорт GLB со скином или анимацией пока не поддерживается.",
        ));
    }
    Ok(parsed)
}

fn read_root(json: &[u8], cancel: &Cancellation) -> AssetResult<gltf::json::Root> {
    let invalid = |error| {
        AssetError::new(
            ErrorCode::InvalidFormat,
            format!("Invalid glTF metadata: {error}"),
        )
    };
    match gltf::json::deserialize::from_slice(json) {
        Ok(root) => Ok(root),
        Err(error) => {
            // Two exporter slips are read leniently, in the decoded metadata
            // only (the file is never changed): a scene without `nodes`
            // (allowed by glTF, required by gltf-json 1.4.1) and a negative
            // `texCoord` (-1 from some exporters), read as set 0.
            let text = error.to_string();
            if !text.starts_with("missing field `nodes` at line ")
                && !text.contains("invalid value: integer `-")
            {
                return Err(invalid(error));
            }
            let (mut metadata, _) = crate::metadata::parse(
                json,
                crate::ExportLimits::default().metadata_values,
                cancel,
            )?;
            if let Some(scenes) = metadata
                .get_mut("scenes")
                .and_then(serde_json::Value::as_array_mut)
            {
                for scene in scenes
                    .iter_mut()
                    .filter_map(serde_json::Value::as_object_mut)
                {
                    cancel.check()?;
                    scene
                        .entry("nodes")
                        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
                }
            }
            negative_tex_coords(&mut metadata, cancel)?;
            gltf::json::deserialize::from_value(metadata).map_err(|_| invalid(error))
        }
    }
}

/// Sets every negative `texCoord` to 0; returns how many were changed.
pub(crate) fn negative_tex_coords(
    value: &mut serde_json::Value,
    cancel: &Cancellation,
) -> AssetResult<usize> {
    let mut changed = 0;
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        cancel.check()?;
        match value {
            serde_json::Value::Object(map) => {
                for (key, item) in map.iter_mut() {
                    if key == "texCoord" && item.as_i64().is_some_and(|v| v < 0) {
                        *item = serde_json::Value::from(0);
                        changed += 1;
                    } else if item.is_object() || item.is_array() {
                        pending.push(item);
                    }
                }
            }
            serde_json::Value::Array(items) => {
                pending.extend(items.iter_mut().filter(|v| v.is_object() || v.is_array()));
            }
            _ => {}
        }
    }
    Ok(changed)
}

/// Required extensions that only add things this viewer does not show and
/// can be left out without changing the geometry or materials it shows.
const IGNORABLE_REQUIRED: [&str; 1] = ["KHR_lights_punctual"];

pub(crate) fn parse<'a>(
    source: &'a [u8],
    limits: ImportLimits,
    cancel: &Cancellation,
) -> AssetResult<Parsed<'a>> {
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
    let glb = gltf::binary::Glb::from_slice(source)
        .map_err(|e| invalid(&format!("Invalid GLB container: {e}")))?;
    let mut root = read_root(&glb.json, cancel)?;
    if root
        .extensions_required
        .iter()
        .any(|name| !IGNORABLE_REQUIRED.contains(&name.as_str()))
    {
        return Err(unsupported(
            "This build does not support required glTF extensions",
        ));
    }
    // gltf validates required extensions against what it implements itself;
    // the ignorable ones are left out of the decoded metadata only.
    root.extensions_required
        .retain(|name| !IGNORABLE_REQUIRED.contains(&name.as_str()));
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
    for animation in &root.animations {
        cancel.check()?;
        if animation
            .channels
            .iter()
            .any(|channel| channel.sampler.value() >= animation.samplers.len())
        {
            return Err(invalid("Animation channel references a missing sampler"));
        }
    }
    let doc = gltf::Document::from_json(root)
        .map_err(|e| invalid(&format!("glTF validation failed: {e}")))?;
    cancel.check()?;
    if doc.extensions_used().any(|name| {
        matches!(
            name,
            "KHR_draco_mesh_compression"
                | "EXT_meshopt_compression"
                | "KHR_mesh_quantization"
                | "KHR_texture_transform"
        )
    }) {
        return Err(unsupported(
            "Compressed/quantized geometry is not supported in this build",
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
    Ok(Parsed { document: doc, glb })
}
