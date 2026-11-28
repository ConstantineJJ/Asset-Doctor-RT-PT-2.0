use super::{
    ExportLimits, Limited, array, budget, metadata, padded, roundtrip, source, strict_chunks,
};
use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentView, ErrorCode, ImportLimits,
    Influence, Interpolation, MeshId, Track, TrackKind,
};
use serde_json::{Value, json};
use std::sync::Arc;

fn invalid() -> AssetError {
    AssetError::new(
        ErrorCode::InvalidFormat,
        "Cannot bind the edited animation to the source GLB",
    )
}

pub fn verify_animation_export(
    view: &DocumentView<'_>,
    limits: ExportLimits,
    import: ImportLimits,
    cancel: &Cancellation,
) -> AssetResult<()> {
    verify(view, limits, import, cancel).map(|_| ())
}

pub(super) fn verify(
    view: &DocumentView<'_>,
    limits: ExportLimits,
    import: ImportLimits,
    cancel: &Cancellation,
) -> AssetResult<Arc<[u8]>> {
    let document = view.document();
    let max = limits
        .output_bytes
        .min(import.source_bytes)
        .min(u32::MAX as usize);
    let source_bytes = document.source();
    if source_bytes.len() > max {
        return Err(budget());
    }
    let bytes = if view.animations_changed() || view.skeleton_changed() {
        encode(view, source_bytes, max, limits, import, cancel)?
    } else {
        source_bytes.clone()
    };
    let decoded = crate::import_glb(
        bytes.clone(),
        document.name().into(),
        document.id(),
        import,
        cancel,
    )?;
    compare(view, &decoded, cancel)?;
    Ok(bytes)
}

#[derive(Clone, Copy)]
enum TrackData<'a> {
    Times(&'a [f32]),
    Values(&'a [[f32; 4]], usize),
    /// RIG-03: a skin's inverse bind matrices (column-major).
    Matrices(&'a [[[f32; 4]; 4]]),
    /// RIG-04: JOINTS_0 (unsigned short) and WEIGHTS_0 (float) of a primitive.
    Joints(&'a [Influence]),
    Weights(&'a [Influence]),
}
impl TrackData<'_> {
    /// (count, components, accessor type, component type, bytes per component)
    fn shape(&self) -> (usize, usize, &'static str, u32, usize) {
        match self {
            Self::Times(times) => (times.len(), 1, "SCALAR", 5126, 4),
            Self::Values(values, 4) => (values.len(), 4, "VEC4", 5126, 4),
            Self::Values(values, _) => (values.len(), 3, "VEC3", 5126, 4),
            Self::Matrices(matrices) => (matrices.len(), 16, "MAT4", 5126, 4),
            Self::Joints(influences) => (influences.len(), 4, "VEC4", 5123, 2),
            Self::Weights(influences) => (influences.len(), 4, "VEC4", 5126, 4),
        }
    }
}
fn add_accessor<'a>(
    meta: &mut Value,
    suffix: &mut Vec<TrackData<'a>>,
    bin_len: &mut usize,
    max: usize,
    data: TrackData<'a>,
) -> AssetResult<usize> {
    let (count, width, kind, component, bytes) = data.shape();
    let len = count.checked_mul(width * bytes).ok_or_else(budget)?;
    let offset = *bin_len;
    *bin_len = offset
        .checked_add(len)
        .filter(|n| *n <= max)
        .ok_or_else(budget)?;
    let views = array(meta, "bufferViews")?;
    let view_index = views.len();
    views.push(json!({ "buffer": 0, "byteOffset": offset, "byteLength": len }));
    let accessors = array(meta, "accessors")?;
    let index = accessors.len();
    let mut accessor = json!({
        "bufferView": view_index, "componentType": component, "type": kind, "count": count
    });
    if let TrackData::Times(times) = data {
        accessor["min"] = json!([times[0]]);
        accessor["max"] = json!([times[count - 1]]);
    }
    accessors.push(accessor);
    suffix.try_reserve(1).map_err(|_| budget())?;
    suffix.push(data);
    Ok(index)
}

fn encode(
    view: &DocumentView<'_>,
    source_bytes: &[u8],
    max: usize,
    limits: ExportLimits,
    import: ImportLimits,
    cancel: &Cancellation,
) -> AssetResult<Arc<[u8]>> {
    let parsed = source::parse(source_bytes, import, cancel)?;
    let bin = parsed.glb.bin.as_deref().ok_or_else(invalid)?;
    strict_chunks(source_bytes, parsed.glb.json.len(), bin.len())?;
    let max_json = limits.json_bytes.min(import.json_bytes);
    if parsed.glb.json.len() > max_json {
        return Err(budget());
    }
    let (mut meta, _) = metadata::parse(&parsed.glb.json, limits.metadata_values, cancel)?;
    let root = meta.as_object_mut().ok_or_else(invalid)?;
    let original = match root.remove("animations") {
        None => Vec::new(),
        Some(Value::Array(clips)) => clips,
        _ => return Err(invalid()),
    };
    if original.len() != view.document().source_clip_count() {
        return Err(invalid());
    }
    let mut animations = Vec::new();
    animations
        .try_reserve_exact(view.animation_clips().len())
        .map_err(|_| budget())?;
    let mut suffix = Vec::new();
    let mut bin_len = bin.len();
    for (index, clip) in view.animation_clips().enumerate() {
        cancel.check()?;
        let mut animation = match clip.source_clip() {
            Some(index) => original.get(index).cloned().ok_or_else(invalid)?,
            None => json!({"channels": [], "samplers": []}),
        };
        if !clip.is_edited() {
            if clip.source_clip() != Some(index)
                && animation
                    .get("name")
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty)
            {
                animation["name"] = json!(clip.name);
            }
            animations.push(animation);
            continue;
        }
        animation["name"] = json!(clip.name);
        let original_channels = std::mem::take(array(&mut animation, "channels")?);
        let mut channels = Vec::new();
        channels
            .try_reserve_exact(clip.tracks().len())
            .map_err(|_| budget())?;
        let samplers = array(&mut animation, "samplers")?;
        for track in clip.tracks() {
            cancel.check()?;
            let (path, width) = match track.kind() {
                TrackKind::Translation => ("translation", 3),
                TrackKind::Rotation => ("rotation", 4),
                TrackKind::Scale => ("scale", 3),
            };
            let mut channel = match track.source_channel() {
                Some(id) => original_channels.get(id).cloned().ok_or_else(invalid)?,
                None => json!({"target": {"node": track.node().0, "path": path}}),
            };
            if track.is_edited() {
                let mut sampler = if let Some(id) = channel.get("sampler").and_then(Value::as_u64) {
                    samplers.get(id as usize).cloned().ok_or_else(invalid)?
                } else {
                    json!({})
                };
                let input = add_accessor(
                    &mut meta,
                    &mut suffix,
                    &mut bin_len,
                    max,
                    TrackData::Times(track.times()),
                )?;
                let output = add_accessor(
                    &mut meta,
                    &mut suffix,
                    &mut bin_len,
                    max,
                    TrackData::Values(track.values(), width),
                )?;
                sampler["input"] = json!(input);
                sampler["output"] = json!(output);
                sampler["interpolation"] = json!(match track.interpolation() {
                    Interpolation::Step => "STEP",
                    Interpolation::Linear => "LINEAR",
                    Interpolation::CubicSpline => "CUBICSPLINE",
                });
                channel["sampler"] = json!(samplers.len());
                samplers.push(sampler);
            }
            channels.push(channel);
        }
        *array(&mut animation, "channels")? = channels;
        animations.push(animation);
    }
    if !animations.is_empty() {
        meta["animations"] = Value::Array(animations);
    }
    if view.skeleton_changed() {
        write_skeleton(view, &mut meta, &mut suffix, &mut bin_len, max, cancel)?;
    }
    array(&mut meta, "buffers")?
        .first_mut()
        .ok_or_else(invalid)?["byteLength"] = json!(bin_len);
    let mut output = Limited {
        bytes: Vec::new(),
        max: max_json,
        cancel,
        failure: None,
    };
    serde_json::to_writer(&mut output, &meta).map_err(|_| {
        AssetError::new(
            output.failure.unwrap_or(ErrorCode::InvalidFormat),
            "Cannot encode animation metadata",
        )
    })?;
    let (readback, _) = metadata::parse(&output.bytes, limits.metadata_values, cancel)?;
    if readback != meta {
        return Err(roundtrip::mismatch());
    }
    let json_len = padded(output.bytes.len());
    let total = 28usize
        .checked_add(json_len)
        .and_then(|n| n.checked_add(bin_len))
        .filter(|&n| n <= max && json_len <= max_json)
        .ok_or_else(budget)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(total).map_err(|_| budget())?;
    for word in [0x46546c67u32, 2, total as u32, json_len as u32, 0x4e4f534a] {
        bytes.extend(word.to_le_bytes());
    }
    bytes.extend(&output.bytes);
    bytes.resize(20 + json_len, b' ');
    bytes.extend((bin_len as u32).to_le_bytes());
    bytes.extend(0x004e4942u32.to_le_bytes());
    for chunk in bin.chunks(65536) {
        cancel.check()?;
        bytes.extend(chunk);
    }
    for data in suffix {
        match data {
            TrackData::Times(times) => {
                for chunk in times.chunks(4096) {
                    cancel.check()?;
                    for value in chunk {
                        bytes.extend(value.to_le_bytes());
                    }
                }
            }
            TrackData::Values(values, width) => {
                for chunk in values.chunks(4096) {
                    cancel.check()?;
                    for value in chunk {
                        for component in &value[..width] {
                            bytes.extend(component.to_le_bytes());
                        }
                    }
                }
            }
            TrackData::Matrices(matrices) => {
                for chunk in matrices.chunks(1024) {
                    cancel.check()?;
                    for value in chunk.iter().flatten().flatten() {
                        bytes.extend(value.to_le_bytes());
                    }
                }
            }
            TrackData::Joints(influences) => {
                for chunk in influences.chunks(4096) {
                    cancel.check()?;
                    for joint in chunk.iter().flat_map(|i| i.joints) {
                        bytes.extend(joint.to_le_bytes());
                    }
                }
            }
            TrackData::Weights(influences) => {
                for chunk in influences.chunks(4096) {
                    cancel.check()?;
                    for weight in chunk.iter().flat_map(|i| i.weights) {
                        bytes.extend(weight.to_le_bytes());
                    }
                }
            }
        }
    }
    if bytes.len() != total {
        return Err(roundtrip::mismatch());
    }
    Ok(Arc::from(bytes))
}

/// RIG-03: renamed nodes get their names; added nodes (after the file's
/// own) are appended with their translation, linked from their parents'
/// children; a skin whose joint table grew gets its joints and a new
/// inverse-bind accessor. Indices of the file's nodes never change.
fn write_skeleton<'a>(
    view: &DocumentView<'a>,
    meta: &mut Value,
    suffix: &mut Vec<TrackData<'a>>,
    bin_len: &mut usize,
    max: usize,
    cancel: &Cancellation,
) -> AssetResult<()> {
    let source = view.document().source_node_count();
    let nodes = view.nodes();
    let list = array(meta, "nodes")?;
    if list.len() != source || nodes.len() < source {
        return Err(invalid());
    }
    for &index in view.renamed() {
        cancel.check()?;
        let node = list.get_mut(index).ok_or_else(invalid)?;
        node["name"] = json!(nodes[index].name);
    }
    // RIG-03: a new rest transform is written as TRS.
    for &index in view.rested() {
        cancel.check()?;
        if index >= source {
            continue; // added joints are written below from their local
        }
        let rest = view
            .rest_transform(ad_document::NodeId(index))
            .ok_or_else(invalid)?;
        let node = list
            .get_mut(index)
            .and_then(Value::as_object_mut)
            .ok_or_else(invalid)?;
        node.remove("matrix");
        node.insert("translation".into(), json!(rest.translation));
        node.insert("rotation".into(), json!(rest.rotation));
        node.insert("scale".into(), json!(rest.scale));
    }
    list.try_reserve_exact(nodes.len() - source)
        .map_err(|_| budget())?;
    for node in &nodes[source..] {
        cancel.check()?;
        let m = node.local;
        let translation_only =
            (0..3).all(|c| (0..4).all(|r| m[c][r] == f32::from(u8::from(c == r)))) && m[3][3] == 1.;
        if !translation_only || !node.meshes.is_empty() {
            return Err(invalid());
        }
        let mut entry = json!({"name": node.name, "translation": [m[3][0], m[3][1], m[3][2]]});
        if !node.children.is_empty() {
            entry["children"] = json!(node.children.iter().map(|c| c.0).collect::<Vec<_>>());
        }
        list.push(entry);
    }
    // Children that differ from the file (an added joint, a deleted one).
    for (index, node) in nodes[..source].iter().enumerate() {
        cancel.check()?;
        let children: Vec<u64> = node.children.iter().map(|c| c.0 as u64).collect();
        let written: Vec<u64> = list[index]
            .get("children")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default();
        if children != written {
            let entry = list[index].as_object_mut().ok_or_else(invalid)?;
            if children.is_empty() {
                entry.remove("children");
            } else {
                entry.insert("children".into(), json!(children));
            }
        }
    }
    let Some(rig) = view.rig() else {
        return Ok(());
    };
    // RIG-04: rewritten vertex joints and weights. Primitive ordinals run
    // over meshes, then their primitives, in file order.
    if !rig.edited_influences().is_empty() {
        let mut places = Vec::new();
        for (mesh, entry) in meta
            .get("meshes")
            .and_then(Value::as_array)
            .ok_or_else(invalid)?
            .iter()
            .enumerate()
        {
            let count = entry
                .get("primitives")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            places.extend((0..count).map(|primitive| (mesh, primitive)));
        }
        for &ordinal in rig.edited_influences() {
            cancel.check()?;
            let influences = rig
                .primitive_influences(MeshId(ordinal))
                .ok_or_else(invalid)?;
            let (mesh, primitive) = *places.get(ordinal).ok_or_else(invalid)?;
            let joints = add_accessor(meta, suffix, bin_len, max, TrackData::Joints(influences))?;
            let weights = add_accessor(meta, suffix, bin_len, max, TrackData::Weights(influences))?;
            let attributes = array(meta, "meshes")?[mesh]
                .get_mut("primitives")
                .and_then(|p| p.get_mut(primitive))
                .and_then(|p| p.get_mut("attributes"))
                .and_then(Value::as_object_mut)
                .ok_or_else(invalid)?;
            attributes.insert("JOINTS_0".into(), json!(joints));
            attributes.insert("WEIGHTS_0".into(), json!(weights));
        }
    }
    for (index, skin) in rig.skins.iter().enumerate() {
        cancel.check()?;
        let written = meta
            .get("skins")
            .and_then(|skins| skins.get(index))
            .and_then(|skin| skin.get("joints"))
            .and_then(Value::as_array)
            .ok_or_else(invalid)?
            .len();
        if written == skin.joints.len() && view.rested().is_empty() {
            continue;
        }
        let accessor = add_accessor(
            meta,
            suffix,
            bin_len,
            max,
            TrackData::Matrices(&skin.inverse_bind),
        )?;
        let entry = &mut array(meta, "skins")?[index];
        entry["joints"] = json!(skin.joints.iter().map(|j| j.0).collect::<Vec<_>>());
        entry["inverseBindMatrices"] = json!(accessor);
    }
    Ok(())
}

pub(crate) fn compare(
    view: &DocumentView<'_>,
    loaded: &AssetDocument,
    cancel: &Cancellation,
) -> AssetResult<()> {
    roundtrip::compare_with_nodes(view.document(), view.nodes(), loaded, &[], cancel)?;
    match (view.rig(), loaded.rig()) {
        (None, None) => {}
        (Some(a), Some(b)) if a.matches(b, cancel)? => {}
        _ => return Err(roundtrip::mismatch()),
    }
    if view.animation_clips().len() != loaded.animation_clips().len() {
        return Err(roundtrip::mismatch());
    }
    for (a, b) in view.animation_clips().zip(loaded.animation_clips()) {
        cancel.check()?;
        if a.name != b.name || a.tracks().len() != b.tracks().len() {
            return Err(roundtrip::mismatch());
        }
        for (a, b) in a.tracks().iter().zip(b.tracks()) {
            cancel.check()?;
            if a.kind() != b.kind()
                || a.node() != b.node()
                || a.interpolation() != b.interpolation()
            {
                return Err(roundtrip::mismatch());
            }
            roundtrip::equal(a.times(), b.times(), cancel)?;
            compare_values(a, b, cancel)?;
        }
    }
    Ok(())
}

fn compare_values(a: &Track, b: &Track, cancel: &Cancellation) -> AssetResult<()> {
    if a.kind() != TrackKind::Rotation {
        return roundtrip::equal(a.values(), b.values(), cancel);
    }
    if a.values().len() != b.values().len() {
        return Err(roundtrip::mismatch());
    }
    let cubic = a.interpolation() == Interpolation::CubicSpline;
    for (chunk, (a, b)) in a
        .values()
        .chunks(4096)
        .zip(b.values().chunks(4096))
        .enumerate()
    {
        cancel.check()?;
        for (offset, (a, b)) in a.iter().zip(b).enumerate() {
            let key = !cubic || (chunk * 4096 + offset) % 3 == 1;
            let matches = if key {
                a.iter().zip(b).all(|(a, b)| (a - b).abs() <= 1e-6)
            } else {
                a == b
            };
            if !matches {
                return Err(roundtrip::mismatch());
            }
        }
    }
    Ok(())
}
