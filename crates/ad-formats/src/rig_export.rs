use super::{
    ExportLimits, Limited, array, budget, metadata, padded, roundtrip, source, strict_chunks,
};
use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentView, ErrorCode, ImportLimits,
    Interpolation, Track, TrackKind,
};
use serde_json::{Value, json};
use std::sync::Arc;

fn invalid() -> AssetError {
    AssetError::new(ErrorCode::InvalidFormat, "Cannot bind the edited animation to the source GLB")
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
    let max = limits.output_bytes.min(import.source_bytes).min(u32::MAX as usize);
    let source_bytes = document.source();
    if source_bytes.len() > max {
        return Err(budget());
    }
    let bytes = if view.animations_changed() {
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
}
impl TrackData<'_> {
    fn shape(&self) -> (usize, usize, &'static str) {
        match self {
            Self::Times(times) => (times.len(), 1, "SCALAR"),
            Self::Values(values, 4) => (values.len(), 4, "VEC4"),
            Self::Values(values, _) => (values.len(), 3, "VEC3"),
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
    let (count, width, kind) = data.shape();
    let len = count.checked_mul(width * 4).ok_or_else(budget)?;
    let offset = *bin_len;
    *bin_len = offset.checked_add(len).filter(|n| *n <= max).ok_or_else(budget)?;
    let views = array(meta, "bufferViews")?;
    let view_index = views.len();
    views.push(json!({ "buffer": 0, "byteOffset": offset, "byteLength": len }));
    let accessors = array(meta, "accessors")?;
    let index = accessors.len();
    let mut accessor = json!({
        "bufferView": view_index, "componentType": 5126, "type": kind, "count": count
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
        channels.try_reserve_exact(clip.tracks().len()).map_err(|_| budget())?;
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
    array(&mut meta, "buffers")?.first_mut().ok_or_else(invalid)?["byteLength"] = json!(bin_len);
    let mut output = Limited { bytes: Vec::new(), max: max_json, cancel, failure: None };
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
    let total = 28usize.checked_add(json_len).and_then(|n| n.checked_add(bin_len))
        .filter(|&n| n <= max && json_len <= max_json).ok_or_else(budget)?;
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
        }
    }
    if bytes.len() != total {
        return Err(roundtrip::mismatch());
    }
    Ok(Arc::from(bytes))
}

pub(crate) fn compare(
    view: &DocumentView<'_>,
    loaded: &AssetDocument,
    cancel: &Cancellation,
) -> AssetResult<()> {
    roundtrip::compare(view.document(), loaded, &[], cancel)?;
    match (view.document().rig(), loaded.rig()) {
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
            if a.kind() != b.kind() || a.node() != b.node() || a.interpolation() != b.interpolation() {
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
    for (chunk, (a, b)) in a.values().chunks(4096).zip(b.values().chunks(4096)).enumerate() {
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
