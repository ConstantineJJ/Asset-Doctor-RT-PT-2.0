use crate::{metadata, roundtrip, source};
use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentStamp, DocumentTag, DocumentView,
    ErrorCode, ImportLimits, MeshId,
};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    sync::Arc,
};

#[derive(Clone, Copy)]
pub struct ExportLimits {
    pub output_bytes: usize,
    pub json_bytes: usize,
    pub metadata_values: usize,
    pub changed_primitives: usize,
}
impl Default for ExportLimits {
    fn default() -> Self {
        Self {
            output_bytes: 192 << 20,
            json_bytes: 16 << 20,
            metadata_values: 200_000,
            changed_primitives: 4096,
        }
    }
}
/// Reimported and compared bytes, not a confirmed saved file.
pub struct VerifiedGlb {
    stamp: DocumentStamp,
    bytes: Arc<[u8]>,
}
impl VerifiedGlb {
    pub fn tag(&self) -> DocumentTag {
        self.stamp.tag()
    }
    pub fn bytes(&self) -> &Arc<[u8]> {
        &self.bytes
    }
    pub fn is_current(&self, doc: &AssetDocument) -> bool {
        self.stamp.is_current(doc)
    }
    pub fn verify_current(&self, doc: &AssetDocument, cancel: &Cancellation) -> AssetResult<()> {
        cancel.check()?;
        if !self.is_current(doc) {
            return Err(AssetError::new(
                ErrorCode::Conflict,
                "Verified export no longer belongs to the current document version",
            ));
        }
        Ok(())
    }
}
fn budget() -> AssetError {
    AssetError::new(
        ErrorCode::ResourceLimit,
        "Export exceeds encoded output or metadata limits",
    )
}
fn invalid() -> AssetError {
    AssetError::new(
        ErrorCode::InvalidFormat,
        "Cannot bind source GLB primitives for export",
    )
}
struct Binding {
    mesh: usize,
    primitive: usize,
    current: usize,
    /// ADR-0033: vertex streams were compacted; every attribute is gathered.
    vertices: bool,
    /// ADR-0036: NORMAL values were rewritten; written from the document.
    normals: bool,
}
/// One gathered vertex attribute of a compacted primitive (ADR-0033): the
/// source accessor's elements, tightly packed with a 4-byte element stride.
struct Gather {
    key: String,
    accessor: Value,
    source_offset: usize,
    source_stride: usize,
    element: usize,
    stride: usize,
}
fn padded(element: usize) -> usize {
    element.div_ceil(4) * 4
}
/// Gathers for every attribute of a compacted binding, in metadata key order.
fn gathers(
    parsed: &source::Parsed<'_>,
    meta: &Value,
    binding: &Binding,
) -> AssetResult<Vec<Gather>> {
    let attributes = meta
        .get("meshes")
        .and_then(|m| m.get(binding.mesh))
        .and_then(|m| m.get("primitives"))
        .and_then(|p| p.get(binding.primitive))
        .and_then(|p| p.get("attributes"))
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(attributes.len())
        .map_err(|_| budget())?;
    for (key, id) in attributes {
        let id = id.as_u64().ok_or_else(invalid)? as usize;
        let accessor = parsed.document.accessors().nth(id).ok_or_else(invalid)?;
        let view = accessor.view().ok_or_else(metadata::unsupported)?;
        if accessor.sparse().is_some() {
            return Err(metadata::unsupported());
        }
        let element = accessor.size();
        let mut json = json!({
            "componentType": accessor.data_type().as_gl_enum(),
            "type": meta["accessors"][id]["type"].clone(),
        });
        if accessor.normalized() {
            json["normalized"] = json!(true);
        }
        result.push(Gather {
            key: key.clone(),
            accessor: json,
            source_offset: view
                .offset()
                .checked_add(accessor.offset())
                .ok_or_else(invalid)?,
            source_stride: view.stride().unwrap_or(element),
            element,
            stride: padded(element),
        });
    }
    Ok(result)
}

fn bindings(
    parsed: &source::Parsed<'_>,
    view: &DocumentView<'_>,
    limits: ExportLimits,
    cancel: &Cancellation,
) -> AssetResult<Vec<Binding>> {
    let bin = parsed.glb.bin.as_deref().ok_or_else(invalid)?;
    let mut changed = Vec::new();
    let mut ordinal = 0;
    for mesh in parsed.document.meshes() {
        for primitive in mesh.primitives() {
            cancel.check()?;
            if primitive.mode() != gltf::mesh::Mode::Triangles
                || primitive.morph_targets().len() > 0
            {
                return Err(metadata::unsupported());
            }
            let current = view
                .primitives()
                .get(ordinal)
                .ok_or_else(roundtrip::mismatch)?;
            let current_indices = view.indices(MeshId(ordinal));
            let position = primitive
                .get(&gltf::Semantic::Positions)
                .ok_or_else(invalid)?;
            if position.count() > u32::MAX as usize {
                return Err(roundtrip::mismatch());
            }
            // ADR-0033: a compacted primitive maps its vertices to a strictly
            // increasing subsequence of the source vertices.
            let map = view.source_vertices(MeshId(ordinal));
            match map {
                None if position.count() != current.positions.len() => {
                    return Err(roundtrip::mismatch());
                }
                Some(m)
                    if m.len() != current.positions.len()
                        || m.windows(2).any(|w| w[0] >= w[1])
                        || m.last().is_some_and(|&v| v as usize >= position.count()) =>
                {
                    return Err(roundtrip::mismatch());
                }
                _ => {}
            }
            // ADR-0034: a merged vertex may stand for a source vertex only if
            // every source attribute element (decoded or not) is byte-identical.
            let mut elements = Vec::new();
            if map.is_some() {
                elements
                    .try_reserve_exact(primitive.attributes().len())
                    .map_err(|_| budget())?;
                for (_, accessor) in primitive.attributes() {
                    let view = accessor.view().ok_or_else(metadata::unsupported)?;
                    elements.push((
                        view.offset() + accessor.offset(),
                        view.stride().unwrap_or(accessor.size()),
                        accessor.size(),
                    ));
                }
            }
            let same_bytes = |a: u32, b: u32| {
                a == b
                    || elements.iter().all(|&(start, stride, size)| {
                        let at = |v: u32| {
                            let begin = start + v as usize * stride;
                            bin.get(begin..begin + size)
                        };
                        at(a).is_some() && at(a) == at(b)
                    })
            };
            let equivalent = |current: [u32; 3], source: [u32; 3]| {
                (0..3).all(|i| same_bytes(current[i], source[i]))
            };
            let source_of = |face: &[u32; 3]| -> Option<[u32; 3]> {
                match map {
                    None => Some(*face),
                    Some(m) => Some([
                        *m.get(face[0] as usize)?,
                        *m.get(face[1] as usize)?,
                        *m.get(face[2] as usize)?,
                    ]),
                }
            };
            let count = primitive.indices().map_or(position.count(), |v| v.count());
            if !count.is_multiple_of(3) {
                return Err(invalid());
            }
            if let Some(i) = primitive.indices()
                && (i.dimensions() != gltf::accessor::Dimensions::Scalar
                    || i.normalized()
                    || !matches!(
                        i.data_type(),
                        gltf::accessor::DataType::U8
                            | gltf::accessor::DataType::U16
                            | gltf::accessor::DataType::U32
                    ))
            {
                return Err(invalid());
            }
            let reader = primitive.reader(|_| Some(bin));
            let mut read = reader.read_indices().map(|i| i.into_u32());
            if primitive.indices().is_some() && read.is_none() {
                return Err(invalid());
            }
            let mut implied = 0..position.count() as u32;
            let mut next = || match &mut read {
                Some(i) => i.next(),
                None => implied.next(),
            };
            let wanted = current_indices.as_chunks::<3>().0;
            let mut kept = 0;
            let normals = view.normals_changed(MeshId(ordinal));
            if normals && current.normals.is_none() {
                return Err(roundtrip::mismatch());
            }
            let mut same = count == current_indices.len() && map.is_none() && !normals;
            for face in 0..count / 3 {
                if face % 1024 == 0 {
                    cancel.check()?;
                }
                let triangle = [
                    next().ok_or_else(invalid)?,
                    next().ok_or_else(invalid)?,
                    next().ok_or_else(invalid)?,
                ];
                if triangle.iter().any(|&v| v as usize >= position.count()) {
                    return Err(invalid());
                }
                same &= wanted.get(face).and_then(source_of) == Some(triangle);
                if wanted
                    .get(kept)
                    .and_then(source_of)
                    .is_some_and(|w| equivalent(w, triangle))
                {
                    kept += 1;
                }
            }
            if kept != wanted.len() || wanted.is_empty() {
                return Err(AssetError::new(
                    ErrorCode::Unsupported,
                    "Export supports only nonempty order-preserving triangle removal",
                ));
            }
            if !same {
                if changed.len() >= limits.changed_primitives {
                    return Err(budget());
                }
                changed.try_reserve(1).map_err(|_| budget())?;
                changed.push(Binding {
                    mesh: mesh.index(),
                    primitive: primitive.index(),
                    current: ordinal,
                    vertices: map.is_some(),
                    normals,
                });
            }
            ordinal += 1;
        }
    }
    if ordinal != view.primitives().len() {
        return Err(roundtrip::mismatch());
    }
    Ok(changed)
}
struct Limited<'a> {
    bytes: Vec<u8>,
    max: usize,
    cancel: &'a Cancellation,
    failure: Option<ErrorCode>,
}
impl Write for Limited<'_> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let failure = if self.cancel.check().is_err() {
            Some(ErrorCode::Cancelled)
        } else if self
            .bytes
            .len()
            .checked_add(data.len())
            .is_none_or(|n| n > self.max)
            || self.bytes.try_reserve(data.len()).is_err()
        {
            Some(ErrorCode::ResourceLimit)
        } else {
            None
        };
        if let Some(code) = failure {
            self.failure = Some(code);
            return Err(io::Error::other("bounded export serialization stopped"));
        }
        self.bytes.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn array<'a>(value: &'a mut Value, key: &str) -> AssetResult<&'a mut Vec<Value>> {
    value
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .ok_or_else(invalid)
}
fn strict_chunks(source: &[u8], json_len: usize, bin_len: usize) -> AssetResult<()> {
    let second = 20usize.checked_add(json_len).ok_or_else(budget)?;
    let end = second
        .checked_add(8)
        .and_then(|v| v.checked_add(bin_len))
        .ok_or_else(budget)?;
    if !json_len.is_multiple_of(4)
        || !bin_len.is_multiple_of(4)
        || end != source.len()
        || source.get(second + 4..second + 8) != Some(0x004e4942u32.to_le_bytes().as_slice())
    {
        return Err(metadata::unsupported());
    }
    Ok(())
}

/// Checks shared by changed-export preflight and real export: strict chunks,
/// bounded core-only metadata and the appended BIN size of every changed
/// primitive seen through `view`. Returns parsed metadata and the BIN length.
fn changed_admission(
    parsed: &source::Parsed<'_>,
    view: &DocumentView<'_>,
    changed: &[Binding],
    limits: ExportLimits,
    import: ImportLimits,
    cancel: &Cancellation,
) -> AssetResult<(Value, usize, usize)> {
    let document = view.document();
    let bin = parsed.glb.bin.as_deref().ok_or_else(invalid)?;
    strict_chunks(document.source(), parsed.glb.json.len(), bin.len())?;
    let (meta, values) = metadata::parse(&parsed.glb.json, limits.metadata_values, cancel)?;
    metadata::core_only(&meta, cancel)?;
    let mut items = values;
    let mut bin_len = bin.len();
    for binding in changed {
        cancel.check()?;
        items = items.checked_add(11).ok_or_else(budget)?;
        bin_len = bin_len
            .checked_add(
                view.indices(MeshId(binding.current))
                    .len()
                    .checked_mul(4)
                    .ok_or_else(budget)?,
            )
            .ok_or_else(budget)?;
        let count = view.primitive(MeshId(binding.current)).positions.len();
        if binding.normals {
            items = items.checked_add(24).ok_or_else(budget)?;
            bin_len = count
                .checked_mul(12)
                .and_then(|n| bin_len.checked_add(n))
                .ok_or_else(budget)?;
        }
        if binding.vertices {
            for gather in gathers(parsed, &meta, binding)? {
                if binding.normals && gather.key == "NORMAL" {
                    continue;
                }
                items = items.checked_add(24).ok_or_else(budget)?;
                bin_len = gather
                    .stride
                    .checked_mul(count)
                    .and_then(|n| bin_len.checked_add(n))
                    .ok_or_else(budget)?;
            }
        }
    }
    if items > limits.metadata_values {
        return Err(budget());
    }
    let max = limits
        .output_bytes
        .min(import.source_bytes)
        .min(u32::MAX as usize);
    if bin_len.checked_add(28).is_none_or(|n| n > max) {
        return Err(budget());
    }
    Ok((meta, bin_len, max))
}

/// G002-06 / ADR-0031: before a user Apply, check that saving the candidate
/// view would stay inside the accepted static export subset (ADR-0028):
/// stable source bindings, order-preserving removal only, strict chunks,
/// core-only metadata and output/metadata/changed-primitive budgets. The
/// candidate is not serialized or reimported here; Save Copy still performs
/// the full verified export. Never mutates the document. Returns the BIN
/// bytes the copy would append to the original (0 for an exact copy).
pub fn preflight_changed_export(
    view: &DocumentView<'_>,
    limits: ExportLimits,
    import: ImportLimits,
    cancel: &Cancellation,
) -> AssetResult<usize> {
    cancel.check()?;
    let document = view.document();
    let parsed = source::parse(document.source(), import, cancel)?;
    if parsed.glb.json.len() > limits.json_bytes {
        return Err(budget());
    }
    let changed = bindings(&parsed, view, limits, cancel)?;
    if changed.is_empty() {
        if document.source().len() > limits.output_bytes {
            return Err(budget());
        }
        return Ok(0);
    }
    let (_, bin_len, _) = changed_admission(&parsed, view, &changed, limits, import, cancel)?;
    let original = parsed.glb.bin.as_deref().map_or(0, <[u8]>::len);
    cancel.check()?;
    Ok(bin_len - original)
}

/// Bytes appended after the original BIN, in metadata order.
enum Append<'a> {
    Indices(&'a [u32]),
    Normals(&'a [[f32; 3]]),
    Gather(Gather, &'a [u32]),
}
fn primitive_json<'a>(
    meta: &'a mut Value,
    binding: &Binding,
) -> AssetResult<&'a mut serde_json::Map<String, Value>> {
    array(meta, "meshes")?
        .get_mut(binding.mesh)
        .and_then(|m| m.get_mut("primitives"))
        .and_then(Value::as_array_mut)
        .and_then(|p| p.get_mut(binding.primitive))
        .and_then(Value::as_object_mut)
        .ok_or_else(invalid)
}

pub fn prepare_glb(
    document: &AssetDocument,
    expected: DocumentTag,
    limits: ExportLimits,
    import: ImportLimits,
    cancel: &Cancellation,
) -> AssetResult<VerifiedGlb> {
    document.verify_tag(expected, cancel)?;
    let stamp = document.stamp();
    let parsed = source::parse(document.source(), import, cancel)?;
    if parsed.glb.json.len() > limits.json_bytes {
        return Err(budget());
    }
    let view = document.view();
    let changed = bindings(&parsed, &view, limits, cancel)?;
    let bytes = if changed.is_empty() {
        if document.source().len() > limits.output_bytes {
            return Err(budget());
        }
        document.source().clone()
    } else {
        let bin = parsed.glb.bin.as_deref().ok_or_else(invalid)?;
        let (mut meta, bin_len, max) =
            changed_admission(&parsed, &view, &changed, limits, import, cancel)?;
        let mut offset = bin.len();
        let mut appends = Vec::new();
        appends.try_reserve(changed.len()).map_err(|_| budget())?;
        for binding in &changed {
            cancel.check()?;
            let current = MeshId(binding.current);
            if binding.vertices {
                let map = view.source_vertices(current).ok_or_else(invalid)?;
                let positions = &view.primitive(current).positions;
                for gather in gathers(&parsed, &meta, binding)? {
                    cancel.check()?;
                    if binding.normals && gather.key == "NORMAL" {
                        continue;
                    }
                    let size = gather.stride.checked_mul(map.len()).ok_or_else(budget)?;
                    let views = array(&mut meta, "bufferViews")?;
                    let id = views.len();
                    let mut buffer_view =
                        json!({"buffer":0,"byteOffset":offset,"byteLength":size,"target":34962});
                    if gather.stride != gather.element {
                        buffer_view["byteStride"] = json!(gather.stride);
                    }
                    views.push(buffer_view);
                    let mut accessor = gather.accessor.clone();
                    accessor["bufferView"] = json!(id);
                    accessor["count"] = json!(map.len());
                    if gather.key == "POSITION" {
                        let mut min = [f32::INFINITY; 3];
                        let mut max = [f32::NEG_INFINITY; 3];
                        for (i, p) in positions.iter().enumerate() {
                            if i % 4096 == 0 {
                                cancel.check()?;
                            }
                            for axis in 0..3 {
                                min[axis] = min[axis].min(p[axis]);
                                max[axis] = max[axis].max(p[axis]);
                            }
                        }
                        accessor["min"] = json!(min.map(f64::from));
                        accessor["max"] = json!(max.map(f64::from));
                    }
                    let accessors = array(&mut meta, "accessors")?;
                    let accessor_id = accessors.len();
                    accessors.push(accessor);
                    primitive_json(&mut meta, binding)?
                        .get_mut("attributes")
                        .and_then(Value::as_object_mut)
                        .ok_or_else(invalid)?
                        .insert(gather.key.clone(), json!(accessor_id));
                    appends.try_reserve(1).map_err(|_| budget())?;
                    appends.push(Append::Gather(gather, map));
                    offset += size;
                }
            }
            if binding.normals {
                let normals = view
                    .primitive(current)
                    .normals
                    .as_deref()
                    .ok_or_else(invalid)?;
                let size = normals.len().checked_mul(12).ok_or_else(budget)?;
                let views = array(&mut meta, "bufferViews")?;
                let id = views.len();
                views
                    .push(json!({"buffer":0,"byteOffset":offset,"byteLength":size,"target":34962}));
                let accessors = array(&mut meta, "accessors")?;
                let accessor = accessors.len();
                accessors.push(json!({"bufferView":id,"componentType":5126,"count":normals.len(),"type":"VEC3"}));
                primitive_json(&mut meta, binding)?
                    .entry("attributes")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .ok_or_else(invalid)?
                    .insert("NORMAL".into(), json!(accessor));
                appends.try_reserve(1).map_err(|_| budget())?;
                appends.push(Append::Normals(normals));
                offset += size;
            }
            let current_indices = view.indices(current);
            let count = current_indices.len();
            let size = count.checked_mul(4).ok_or_else(budget)?;
            let views = array(&mut meta, "bufferViews")?;
            let view = views.len();
            views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":size,"target":34963}));
            let accessors = array(&mut meta, "accessors")?;
            let accessor = accessors.len();
            accessors.push(
                json!({"bufferView":view,"componentType":5125,"count":count,"type":"SCALAR"}),
            );
            primitive_json(&mut meta, binding)?.insert("indices".into(), json!(accessor));
            appends.try_reserve(1).map_err(|_| budget())?;
            appends.push(Append::Indices(current_indices));
            offset += size;
        }
        let buffer = array(&mut meta, "buffers")?
            .first_mut()
            .and_then(Value::as_object_mut)
            .ok_or_else(invalid)?;
        buffer.insert("byteLength".into(), json!(bin_len));
        let mut output = Limited {
            bytes: Vec::new(),
            max: limits.json_bytes.min(import.json_bytes),
            cancel,
            failure: None,
        };
        if let Err(error) = serde_json::to_writer(&mut output, &meta) {
            return Err(AssetError::new(
                output.failure.unwrap_or(ErrorCode::InvalidFormat),
                format!("Cannot encode GLB metadata: {error}"),
            ));
        }
        let (readback, _) = metadata::parse(&output.bytes, limits.metadata_values, cancel)?;
        if readback != meta {
            return Err(roundtrip::mismatch());
        }
        while !output.bytes.len().is_multiple_of(4) {
            output.write_all(b" ").map_err(|_| {
                AssetError::new(
                    output.failure.unwrap_or(ErrorCode::ResourceLimit),
                    "Cannot pad GLB metadata",
                )
            })?;
        }
        let total = 28usize
            .checked_add(output.bytes.len())
            .and_then(|n| n.checked_add(bin_len))
            .ok_or_else(budget)?;
        if total > max {
            return Err(budget());
        }
        let mut result = Vec::new();
        result.try_reserve_exact(total).map_err(|_| budget())?;
        for word in [
            0x46546c67u32,
            2,
            total as u32,
            output.bytes.len() as u32,
            0x4e4f534a,
        ] {
            result.extend(word.to_le_bytes());
        }
        result.extend_from_slice(&output.bytes);
        result.extend((bin_len as u32).to_le_bytes());
        result.extend(0x004e4942u32.to_le_bytes());
        for chunk in bin.chunks(65536) {
            cancel.check()?;
            result.extend_from_slice(chunk);
        }
        for append in &appends {
            match append {
                Append::Indices(indices) => {
                    for chunk in indices.chunks(4096) {
                        cancel.check()?;
                        for index in chunk {
                            result.extend(index.to_le_bytes());
                        }
                    }
                }
                Append::Normals(normals) => {
                    for chunk in normals.chunks(4096) {
                        cancel.check()?;
                        for normal in chunk {
                            for value in normal {
                                result.extend(value.to_le_bytes());
                            }
                        }
                    }
                }
                Append::Gather(gather, map) => {
                    let padding = gather.stride - gather.element;
                    for (i, &v) in map.iter().enumerate() {
                        if i % 4096 == 0 {
                            cancel.check()?;
                        }
                        let start = (v as usize)
                            .checked_mul(gather.source_stride)
                            .and_then(|n| n.checked_add(gather.source_offset))
                            .ok_or_else(invalid)?;
                        let element = bin.get(start..start + gather.element).ok_or_else(invalid)?;
                        result.extend_from_slice(element);
                        result.extend(std::iter::repeat_n(0u8, padding));
                    }
                }
            }
        }
        if result.len() != total {
            return Err(roundtrip::mismatch());
        }
        Arc::from(result)
    };
    let decoded = crate::import_glb(
        bytes.clone(),
        document.name().into(),
        expected.document,
        import,
        cancel,
    )?;
    roundtrip::compare(document, &decoded, cancel)?;
    let verified = VerifiedGlb { stamp, bytes };
    verified.verify_current(document, cancel)?;
    Ok(verified)
}
