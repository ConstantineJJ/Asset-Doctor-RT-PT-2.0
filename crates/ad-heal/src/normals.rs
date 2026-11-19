//! ADR-0037: area-weighted normals for G004/G005 (REPAIR-03).
use super::{
    DuplicatePlan, DuplicatePreview, PlanLimits, RepairKind, budget, reserve, source_references,
};
use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentTag, ErrorCode, MeshId,
};

/// Per-vertex sums of face cross products (area weighted), by vertex index.
pub(crate) fn face_sums(
    positions: &[[f32; 3]],
    indices: &[u32],
    scratch_bytes: usize,
    mut cancel: impl FnMut() -> AssetResult<()>,
) -> AssetResult<Vec<[f64; 3]>> {
    let mut sums = reserve::<[f64; 3]>(positions.len(), scratch_bytes)?;
    sums.resize(positions.len(), [0.; 3]);
    for (i, face) in indices.as_chunks::<3>().0.iter().enumerate() {
        if i % 4096 == 0 {
            cancel()?;
        }
        let p = |k: usize| positions[face[k] as usize].map(f64::from);
        let (a, b, c) = (p(0), p(1), p(2));
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        for &vertex in face {
            let s = &mut sums[vertex as usize];
            for axis in 0..3 {
                s[axis] += n[axis];
            }
        }
    }
    cancel()?;
    Ok(sums)
}
/// Unit f32 normal of a sum, or None when the sum is zero.
pub(crate) fn unit(sum: [f64; 3]) -> Option<[f32; 3]> {
    let length = (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]).sqrt();
    (length > 0. && length.is_finite()).then(|| sum.map(|v| (v / length) as f32))
}

/// Complete read-only G004/G005 plan for one source primitive (ADR-0037).
/// G004 creates every normal; G005 replaces only zero normals. Refused when
/// the primitive has tangents or a target vertex has no non-degenerate face.
pub fn plan_normals(
    document: &AssetDocument,
    expected: DocumentTag,
    mesh: MeshId,
    kind: RepairKind,
    limits: PlanLimits,
    cancel: &Cancellation,
) -> AssetResult<DuplicatePlan> {
    document.verify_tag(expected, cancel)?;
    let unsupported = |m: String| AssetError::new(ErrorCode::Unsupported, m);
    let primitive = document.primitives().get(mesh.0).ok_or_else(|| {
        AssetError::new(
            ErrorCode::InvalidGeometry,
            "Unknown normals source primitive",
        )
    })?;
    match (kind, &primitive.normals) {
        (RepairKind::MissingNormals, None) | (RepairKind::ZeroNormals, Some(_)) => {}
        (RepairKind::MissingNormals, Some(_)) => {
            return Err(unsupported(
                "The primitive already has normals (G004 does not apply)".into(),
            ));
        }
        (RepairKind::ZeroNormals, None) => {
            return Err(unsupported(
                "The primitive has no normals (use the G004 repair)".into(),
            ));
        }
        _ => return Err(unsupported("Not a normals repair".into())),
    }
    if primitive.tangents.is_some() {
        return Err(unsupported(
            "Normals are not recomputed for a primitive with tangents: the tangent basis would become stale".into(),
        ));
    }
    if primitive.indices.len() / 3 > limits.triangles {
        return Err(budget());
    }
    let sums = face_sums(
        &primitive.positions,
        &primitive.indices,
        limits.scratch_bytes,
        || cancel.check(),
    )?;
    let mut used = reserve::<bool>(primitive.positions.len(), limits.scratch_bytes)?;
    used.resize(primitive.positions.len(), false);
    for &i in &primitive.indices {
        used[i as usize] = true;
    }
    let mut normals = reserve::<[f32; 3]>(primitive.positions.len(), limits.result_bytes)?;
    let mut changed = 0usize;
    for (v, sum) in sums.iter().enumerate() {
        if v % 4096 == 0 {
            cancel.check()?;
        }
        let current = primitive.normals.as_ref().map(|n| n[v]);
        if current.is_some_and(|n| n != [0.; 3]) {
            normals.push(current.expect("checked"));
            continue;
        }
        let Some(normal) = unit(*sum) else {
            return Err(unsupported(format!(
                "Vertex {v} has no non-degenerate triangle, so its normal cannot be computed; {}",
                if used[v] {
                    "remove zero-area triangles (G001) first"
                } else {
                    "remove unused vertices (G003) first"
                }
            )));
        };
        normals.push(normal);
        changed += 1;
    }
    drop(sums);
    let references = source_references(document, mesh, limits.scratch_bytes, || cancel.check())?;
    cancel.check()?;
    let triangles = primitive.indices.len() / 3;
    let count = primitive.positions.len();
    Ok(DuplicatePlan {
        stamp: document.stamp(),
        preview: DuplicatePreview {
            tag: expected,
            mesh,
            before_triangles: triangles,
            after_triangles: triangles,
            before_vertices: count,
            after_vertices: count,
            changed_normals: changed,
            source_node_references: references,
        },
        removed: Vec::new(),
        targets: Vec::new(),
        normals,
        kind,
    })
}
