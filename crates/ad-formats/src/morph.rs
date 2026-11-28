//! ADR-0065: reading morph targets (POSITION and NORMAL deltas, F32 VEC3)
//! and the mesh's default weights and target names.
use ad_document::{AssetError, AssetResult, Cancellation, ErrorCode, MorphPrimitive, MorphTarget};
use gltf::accessor::{DataType, Dimensions};
use std::sync::Arc;

fn invalid(message: &str) -> AssetError {
    AssetError::new(ErrorCode::InvalidFormat, message)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn read_targets(
    mesh: &gltf::Mesh<'_>,
    primitive: &gltf::Primitive<'_>,
    bin: &[u8],
    count: usize,
    decoded: &mut usize,
    remaining: usize,
    cancel: &Cancellation,
) -> AssetResult<Option<Arc<MorphPrimitive>>> {
    let targets = primitive.morph_targets().len();
    if targets == 0 {
        return Ok(None);
    }
    let budget = || {
        AssetError::new(
            ErrorCode::ResourceLimit,
            "Import exceeds the configured resource budget",
        )
    };
    for target in primitive.morph_targets() {
        for accessor in [target.positions(), target.normals()].into_iter().flatten() {
            if accessor.data_type() != DataType::F32
                || accessor.dimensions() != Dimensions::Vec3
                || accessor.normalized()
                || accessor.count() != count
            {
                return Err(AssetError::new(
                    ErrorCode::Unsupported,
                    "Морф-цели должны быть F32 VEC3 на каждую вершину (сжатые цели пока не открываются)",
                ));
            }
        }
    }
    *decoded = count
        .checked_mul(2 * size_of::<[f32; 3]>())
        .and_then(|per| per.checked_mul(targets))
        .and_then(|bytes| decoded.checked_add(bytes))
        .filter(|&total| total <= remaining)
        .ok_or_else(budget)?;
    let reader = primitive.reader(|_| Some(bin));
    let mut out = Vec::new();
    out.try_reserve_exact(targets).map_err(|_| budget())?;
    for (positions, normals, _tangents) in reader.read_morph_targets() {
        cancel.check()?;
        let read = |iter: gltf::accessor::Iter<'_, [f32; 3]>| -> AssetResult<Vec<[f32; 3]>> {
            let mut values = Vec::new();
            values.try_reserve_exact(count).map_err(|_| budget())?;
            for (i, v) in iter.enumerate() {
                if i % 4096 == 0 {
                    cancel.check()?;
                }
                values.push(v);
            }
            if values.len() != count {
                return Err(invalid("A morph target does not cover every vertex"));
            }
            Ok(values)
        };
        let positions = match positions {
            Some(iter) => read(iter)?,
            None => vec![[0.; 3]; count],
        };
        let normals = normals.map(read).transpose()?;
        out.push(MorphTarget { positions, normals });
    }
    let weights: Arc<[f32]> = match mesh.weights() {
        Some(w) if w.len() == targets => Arc::from(w),
        Some(_) => return Err(invalid("Mesh weights do not match its morph targets")),
        None => Arc::from(vec![0.; targets]),
    };
    let names: Arc<[String]> = mesh
        .extras()
        .as_ref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw.get()).ok())
        .and_then(|extras| {
            let names = extras.get("targetNames")?.as_array()?;
            (names.len() == targets).then(|| {
                names
                    .iter()
                    .map(|n| {
                        n.as_str()
                            .unwrap_or("")
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(120)
                            .collect()
                    })
                    .collect()
            })
        })
        .unwrap_or_else(|| Arc::from([]));
    Ok(Some(Arc::new(MorphPrimitive {
        targets: out,
        weights,
        names,
    })))
}
