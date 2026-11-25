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
        AssetError::new(ErrorCode::InvalidFormat, format!("Invalid glTF metadata: {error}"))
    };
    match gltf::json::deserialize::from_slice(json) {
        Ok(root) => Ok(root),
        Err(error) if error.to_string().starts_with("missing field `nodes` at line ") => {
            // glTF allows an empty scene to omit nodes. gltf-json 1.4.1
            // requires the field. Adapt the decoded metadata only.
            let (mut metadata, _) =
                crate::metadata::parse(json, crate::ExportLimits::default().metadata_values, cancel)?;
            let Some(scenes) = metadata.get_mut("scenes").and_then(serde_json::Value::as_array_mut)
            else {
                return Err(invalid(error));
            };
            for scene in scenes.iter_mut().filter_map(serde_json::Value::as_object_mut) {
                cancel.check()?;
                scene.entry("nodes").or_insert_with(|| serde_json::Value::Array(Vec::new()));
            }
            gltf::json::deserialize::from_value(metadata).map_err(invalid)
        }
        Err(error) => Err(invalid(error)),
    }
}

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
    let root = read_root(&glb.json, cancel)?;
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
