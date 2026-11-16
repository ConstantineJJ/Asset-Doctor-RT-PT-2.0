use crate::{metadata, roundtrip, source};
use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentStamp, DocumentTag, ErrorCode,
    ImportLimits,
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
}

fn bindings(
    parsed: &source::Parsed<'_>,
    document: &AssetDocument,
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
            let current = document
                .primitives()
                .get(ordinal)
                .ok_or_else(roundtrip::mismatch)?;
            let position = primitive
                .get(&gltf::Semantic::Positions)
                .ok_or_else(invalid)?;
            if position.count() != current.positions.len() || position.count() > u32::MAX as usize {
                return Err(roundtrip::mismatch());
            }
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
            let wanted = current.indices.as_chunks::<3>().0;
            let mut kept = 0;
            let mut same = count == current.indices.len();
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
                same &= wanted.get(face) == Some(&triangle);
                if wanted.get(kept) == Some(&triangle) {
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
                });
            }
            ordinal += 1;
        }
    }
    if ordinal != document.primitives().len() {
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
    let changed = bindings(&parsed, document, limits, cancel)?;
    let bytes = if changed.is_empty() {
        if document.source().len() > limits.output_bytes {
            return Err(budget());
        }
        document.source().clone()
    } else {
        let bin = parsed.glb.bin.as_deref().ok_or_else(invalid)?;
        strict_chunks(document.source(), parsed.glb.json.len(), bin.len())?;
        let (mut meta, values) = metadata::parse(&parsed.glb.json, limits.metadata_values, cancel)?;
        metadata::core_only(&meta, cancel)?;
        if changed
            .len()
            .checked_mul(11)
            .and_then(|n| n.checked_add(values))
            .is_none_or(|n| n > limits.metadata_values)
        {
            return Err(budget());
        }
        let mut bin_len = bin.len();
        for binding in &changed {
            bin_len = bin_len
                .checked_add(
                    document.primitives()[binding.current]
                        .indices
                        .len()
                        .checked_mul(4)
                        .ok_or_else(budget)?,
                )
                .ok_or_else(budget)?;
        }
        let max = limits
            .output_bytes
            .min(import.source_bytes)
            .min(u32::MAX as usize);
        if bin_len.checked_add(28).is_none_or(|n| n > max) {
            return Err(budget());
        }
        let mut offset = bin.len();
        for binding in &changed {
            cancel.check()?;
            let count = document.primitives()[binding.current].indices.len();
            let size = count.checked_mul(4).ok_or_else(budget)?;
            let views = array(&mut meta, "bufferViews")?;
            let view = views.len();
            views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":size,"target":34963}));
            let accessors = array(&mut meta, "accessors")?;
            let accessor = accessors.len();
            accessors.push(
                json!({"bufferView":view,"componentType":5125,"count":count,"type":"SCALAR"}),
            );
            let primitive = array(&mut meta, "meshes")?
                .get_mut(binding.mesh)
                .and_then(|m| m.get_mut("primitives"))
                .and_then(Value::as_array_mut)
                .and_then(|p| p.get_mut(binding.primitive))
                .and_then(Value::as_object_mut)
                .ok_or_else(invalid)?;
            primitive.insert("indices".into(), json!(accessor));
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
        for binding in &changed {
            for chunk in document.primitives()[binding.current].indices.chunks(4096) {
                cancel.check()?;
                for index in chunk {
                    result.extend(index.to_le_bytes());
                }
            }
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
