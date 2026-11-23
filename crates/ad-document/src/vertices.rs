//! ADR-0033: removing unused vertices of one primitive. The command publishes a
//! complete replacement primitive (streams compacted, indices renumbered,
//! material fields shared) together with its source-vertex map.
use crate::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentTag, ImportLimits, MeshId,
    MeshPrimitive, PreparedRemoval, RemovalPreview, UvChannel,
    indices::{Alternate, budget},
};
use std::mem::size_of;

fn invalid() -> AssetError {
    AssetError::new(
        crate::ErrorCode::InvalidGeometry,
        "Vertex removals must be sorted, unique, in range and unused by every triangle",
    )
}

/// Retained heap bytes of one primitive's vertex/index streams and source map
/// (capacity based), plus the boxed primitive header kept by history.
pub(crate) fn primitive_bytes(mesh: &MeshPrimitive, map: Option<&Vec<u32>>) -> AssetResult<usize> {
    let parts = [
        mesh.positions.capacity().checked_mul(12),
        mesh.normals
            .as_ref()
            .map_or(Some(0), |v| v.capacity().checked_mul(12)),
        mesh.tangents
            .as_ref()
            .map_or(Some(0), |v| v.capacity().checked_mul(16)),
        mesh.colors
            .as_ref()
            .map_or(Some(0), |v| v.capacity().checked_mul(16)),
        mesh.indices.capacity().checked_mul(4),
        map.map_or(Some(0), |v| v.capacity().checked_mul(4)),
        mesh.uv_channels
            .capacity()
            .checked_mul(size_of::<UvChannel>()),
        Some(size_of::<MeshPrimitive>()),
    ];
    let mut total = 0usize;
    for part in parts {
        total = part.and_then(|p| total.checked_add(p)).ok_or_else(budget)?;
    }
    for channel in &mesh.uv_channels {
        total = channel
            .values
            .capacity()
            .checked_mul(8)
            .and_then(|p| total.checked_add(p))
            .ok_or_else(budget)?;
    }
    Ok(total)
}

/// Keeps the values whose rank is not `u32::MAX`, in order.
fn gather<T: Copy>(
    values: &[T],
    rank: &[u32],
    kept: usize,
    cancel: &Cancellation,
) -> AssetResult<Vec<T>> {
    let mut result = Vec::new();
    result.try_reserve_exact(kept).map_err(|_| budget())?;
    for (i, (value, &r)) in values.iter().zip(rank).enumerate() {
        if i % 4096 == 0 {
            cancel.check()?;
        }
        if r != u32::MAX {
            result.push(*value);
        }
    }
    Ok(result)
}

/// Document-level guard for merges: every decoded stream bitwise equal.
fn same_streams(mesh: &MeshPrimitive, a: u32, b: u32) -> bool {
    fn bits<const N: usize>(v: &[[f32; N]], i: u32) -> Option<[u32; N]> {
        v.get(i as usize).map(|x| x.map(f32::to_bits))
    }
    fn optional<const N: usize>(v: &Option<Vec<[f32; N]>>, a: u32, b: u32) -> bool {
        v.as_deref().is_none_or(|v| bits(v, a) == bits(v, b))
    }
    bits(&mesh.positions, a).is_some()
        && bits(&mesh.positions, a) == bits(&mesh.positions, b)
        && optional(&mesh.normals, a, b)
        && optional(&mesh.tangents, a, b)
        && optional(&mesh.colors, a, b)
        && mesh
            .uv_channels
            .iter()
            .all(|c| bits(&c.values, a) == bits(&c.values, b))
}

fn gather_optional<T: Copy>(
    values: &Option<Vec<T>>,
    rank: &[u32],
    kept: usize,
    cancel: &Cancellation,
) -> AssetResult<Option<Vec<T>>> {
    values
        .as_deref()
        .map(|v| gather(v, rank, kept, cancel))
        .transpose()
}

impl AssetDocument {
    /// Map from current vertices of a primitive to its original source
    /// vertices; `None` is the identity (ADR-0033).
    pub fn source_vertices(&self, mesh: MeshId) -> Option<&[u32]> {
        self.vertex_maps.get(mesh.0)?.as_deref()
    }
    /// Lower bound for a history reservation of a whole-primitive command.
    pub fn primitive_capacity_bytes(&self, mesh: MeshId) -> AssetResult<usize> {
        let primitive = self.parts.meshes.get(mesh.0).ok_or_else(invalid)?;
        primitive_bytes(primitive, self.vertex_maps[mesh.0].as_ref())
    }
    /// Prepares removal of the listed vertices (sorted current ordinals, each
    /// unused by this primitive's indices). Never mutates the document; an
    /// empty list is a verified no-op. Application history must admit before
    /// committing.
    pub fn prepare_vertex_removal(
        &self,
        expected: DocumentTag,
        mesh: MeshId,
        removed: &[u32],
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        self.prepare_vertex_remap(expected, mesh, removed, None, limits, cancel)
    }
    /// ADR-0034: merges each `[removed, kept]` pair (sorted by removed vertex)
    /// by pointing every index of the removed vertex at its earlier, kept twin,
    /// then compacting. Refused unless the twin is earlier, itself kept and
    /// bitwise equal on every decoded stream. Never mutates the document.
    pub fn prepare_vertex_merge(
        &self,
        expected: DocumentTag,
        mesh: MeshId,
        pairs: &[[u32; 2]],
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        let mut removed = Vec::new();
        let mut targets = Vec::new();
        removed
            .try_reserve_exact(pairs.len())
            .and_then(|_| targets.try_reserve_exact(pairs.len()))
            .map_err(|_| budget())?;
        for pair in pairs {
            removed.push(pair[0]);
            targets.push(pair[1]);
        }
        self.prepare_vertex_remap(expected, mesh, &removed, Some(&targets), limits, cancel)
    }
    fn prepare_vertex_remap(
        &self,
        expected: DocumentTag,
        mesh: MeshId,
        removed: &[u32],
        targets: Option<&[u32]>,
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        self.require_editable()?;
        self.verify_tag(expected, cancel)?;
        let current = self.parts.meshes.get(mesh.0).ok_or_else(invalid)?;
        let count = current.positions.len();
        let triangles = current.indices.len() / 3;
        let mut previous = None;
        for (i, &v) in removed.iter().enumerate() {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            if v as usize >= count || previous.is_some_and(|p| p >= v) {
                return Err(invalid());
            }
            previous = Some(v);
        }
        if let Some(targets) = targets {
            for (i, (&r, &t)) in removed.iter().zip(targets).enumerate() {
                if i % 4096 == 0 {
                    cancel.check()?;
                }
                if t >= r || removed.binary_search(&t).is_ok() || !same_streams(current, t, r) {
                    return Err(invalid());
                }
            }
        }
        let preview = RemovalPreview {
            tag: expected,
            mesh,
            before_triangles: triangles,
            after_triangles: triangles,
            before_vertices: count,
            after_vertices: count - removed.len(),
        };
        if removed.is_empty() {
            cancel.check()?;
            return Ok(PreparedRemoval::new(preview, self.origin.clone(), None, 0));
        }
        let kept = count - removed.len();
        let mut rank = Vec::new();
        rank.try_reserve_exact(count).map_err(|_| budget())?;
        let mut next = removed.iter().peekable();
        let mut ordinal = 0u32;
        for v in 0..count {
            if v % 4096 == 0 {
                cancel.check()?;
            }
            if next.peek().is_some_and(|&&r| r as usize == v) {
                next.next();
                rank.push(u32::MAX);
            } else {
                rank.push(ordinal);
                ordinal += 1;
            }
        }
        let mut indices = Vec::new();
        indices
            .try_reserve_exact(current.indices.len())
            .map_err(|_| budget())?;
        for (i, &index) in current.indices.iter().enumerate() {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            let r = match (rank[index as usize], targets) {
                (u32::MAX, Some(targets)) => removed
                    .binary_search(&index)
                    .map(|k| rank[targets[k] as usize])
                    .map_err(|_| invalid())?,
                (u32::MAX, None) => return Err(invalid()),
                (r, _) => r,
            };
            indices.push(r);
        }
        self.check_index_budget(
            mesh,
            (kept, current.uv_channels.len(), indices.len()),
            limits,
            cancel,
        )?;
        let mut uv_channels = Vec::new();
        uv_channels
            .try_reserve_exact(current.uv_channels.len())
            .map_err(|_| budget())?;
        for channel in &current.uv_channels {
            uv_channels.push(UvChannel {
                set: channel.set,
                values: gather(&channel.values, &rank, kept, cancel)?,
            });
        }
        let old_map = self.vertex_maps[mesh.0].as_deref();
        let mut map = Vec::new();
        map.try_reserve_exact(kept).map_err(|_| budget())?;
        for (v, &r) in rank.iter().enumerate() {
            if v % 4096 == 0 {
                cancel.check()?;
            }
            if r != u32::MAX {
                map.push(old_map.map_or(v as u32, |m| m[v]));
            }
        }
        let primitive = MeshPrimitive {
            uv_channels,
            uv_bindings: current.uv_bindings,
            positions: gather(&current.positions, &rank, kept, cancel)?,
            normals: gather_optional(&current.normals, &rank, kept, cancel)?,
            tangents: gather_optional(&current.tangents, &rank, kept, cancel)?,
            normal_texture: current.normal_texture.clone(),
            normal_scale: current.normal_scale,
            occlusion_texture: current.occlusion_texture.clone(),
            occlusion_strength: current.occlusion_strength,
            emissive_factor: current.emissive_factor,
            emissive_texture: current.emissive_texture.clone(),
            metallic_roughness: current.metallic_roughness,
            indices,
            base_color: current.base_color,
            colors: gather_optional(&current.colors, &rank, kept, cancel)?,
            color_texture: current.color_texture.clone(),
            metallic_roughness_texture: current.metallic_roughness_texture.clone(),
            alpha_cutoff: current.alpha_cutoff,
        };
        let retained =
            primitive_bytes(&primitive, Some(&map))?.max(self.primitive_capacity_bytes(mesh)?);
        cancel.check()?;
        Ok(PreparedRemoval::new(
            preview,
            self.origin.clone(),
            Some(Alternate::Primitive(
                Box::new(primitive),
                Some(map),
                self.normals_changed[mesh.0],
            )),
            retained,
        ))
    }
}

impl AssetDocument {
    /// True when the primitive's NORMAL values were rewritten (ADR-0036/0037).
    pub fn normals_changed(&self, mesh: MeshId) -> bool {
        self.normals_changed.get(mesh.0).copied().unwrap_or(false)
    }
    /// ADR-0037: replaces the NORMAL stream of one primitive. Accepted only
    /// without tangents, with one finite unit-length normal per vertex and, when
    /// `keep_authored`, every current non-zero normal unchanged bit for bit.
    /// Never mutates the document; identical normals are a verified no-op.
    pub fn prepare_normals(
        &self,
        expected: DocumentTag,
        mesh: MeshId,
        normals: Vec<[f32; 3]>,
        keep_authored: bool,
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        self.require_editable()?;
        self.verify_tag(expected, cancel)?;
        let current = self.parts.meshes.get(mesh.0).ok_or_else(invalid)?;
        if current.tangents.is_some() {
            return Err(AssetError::new(
                crate::ErrorCode::Unsupported,
                "Normals of a primitive with tangents are not recomputed: the tangent basis would become stale",
            ));
        }
        let bad = || {
            AssetError::new(
                crate::ErrorCode::InvalidGeometry,
                "Replacement normals must be finite unit vectors, one per vertex, keeping authored normals",
            )
        };
        if normals.len() != current.positions.len() {
            return Err(bad());
        }
        for (i, n) in normals.iter().enumerate() {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            let length = n
                .iter()
                .map(|&v| f64::from(v) * f64::from(v))
                .sum::<f64>()
                .sqrt();
            let authored = current
                .normals
                .as_ref()
                .map(|v| v[i])
                .filter(|v| *v != [0.; 3]);
            if n.iter().any(|v| !v.is_finite())
                || (length - 1.).abs() > 1e-5
                || (keep_authored
                    && authored.is_some_and(|a| a.map(f32::to_bits) != n.map(f32::to_bits)))
            {
                return Err(bad());
            }
        }
        let triangles = current.indices.len() / 3;
        let preview = RemovalPreview {
            tag: expected,
            mesh,
            before_triangles: triangles,
            after_triangles: triangles,
            before_vertices: current.positions.len(),
            after_vertices: current.positions.len(),
        };
        let same = current.normals.as_ref().is_some_and(|c| {
            c.iter()
                .zip(&normals)
                .all(|(a, b)| a.map(f32::to_bits) == b.map(f32::to_bits))
        });
        if same {
            cancel.check()?;
            return Ok(PreparedRemoval::new(preview, self.origin.clone(), None, 0));
        }
        self.check_index_budget(
            mesh,
            (
                current.positions.len(),
                current.uv_channels.len(),
                current.indices.len(),
            ),
            limits,
            cancel,
        )?;
        let copy = |v: &[[f32; 2]]| -> AssetResult<Vec<[f32; 2]>> {
            let mut out = Vec::new();
            out.try_reserve_exact(v.len()).map_err(|_| budget())?;
            out.extend_from_slice(v);
            Ok(out)
        };
        let mut uv_channels = Vec::new();
        uv_channels
            .try_reserve_exact(current.uv_channels.len())
            .map_err(|_| budget())?;
        for channel in &current.uv_channels {
            uv_channels.push(UvChannel {
                set: channel.set,
                values: copy(&channel.values)?,
            });
        }
        let mut positions = Vec::new();
        positions
            .try_reserve_exact(current.positions.len())
            .map_err(|_| budget())?;
        positions.extend_from_slice(&current.positions);
        let mut indices = Vec::new();
        indices
            .try_reserve_exact(current.indices.len())
            .map_err(|_| budget())?;
        indices.extend_from_slice(&current.indices);
        let colors = match &current.colors {
            Some(c) => {
                let mut out = Vec::new();
                out.try_reserve_exact(c.len()).map_err(|_| budget())?;
                out.extend_from_slice(c);
                Some(out)
            }
            None => None,
        };
        let map = match self.vertex_maps[mesh.0].as_ref() {
            Some(m) => {
                let mut out = Vec::new();
                out.try_reserve_exact(m.len()).map_err(|_| budget())?;
                out.extend_from_slice(m);
                Some(out)
            }
            None => None,
        };
        let primitive = MeshPrimitive {
            uv_channels,
            uv_bindings: current.uv_bindings,
            positions,
            normals: Some(normals),
            tangents: None,
            normal_texture: current.normal_texture.clone(),
            normal_scale: current.normal_scale,
            occlusion_texture: current.occlusion_texture.clone(),
            occlusion_strength: current.occlusion_strength,
            emissive_factor: current.emissive_factor,
            emissive_texture: current.emissive_texture.clone(),
            metallic_roughness: current.metallic_roughness,
            indices,
            base_color: current.base_color,
            colors,
            color_texture: current.color_texture.clone(),
            metallic_roughness_texture: current.metallic_roughness_texture.clone(),
            alpha_cutoff: current.alpha_cutoff,
        };
        let retained =
            primitive_bytes(&primitive, map.as_ref())?.max(self.primitive_capacity_bytes(mesh)?);
        cancel.check()?;
        Ok(PreparedRemoval::new(
            preview,
            self.origin.clone(),
            Some(Alternate::Primitive(Box::new(primitive), map, true)),
            retained,
        ))
    }
}
