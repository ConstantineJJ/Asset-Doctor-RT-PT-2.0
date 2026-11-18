//! Rebuild verification data from source/candidate, not planner working records.
use super::{VerificationLimits, budget, mismatch, reserve};
use ad_document::{AssetResult, MeshPrimitive, Texture};
use std::sync::Arc;

fn bits<const N: usize>(v: &[f32; N]) -> [u32; N] {
    v.map(f32::to_bits)
}
fn same_texture(a: &Option<Arc<Texture>>, b: &Option<Arc<Texture>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}
/// Everything except the vertex/index streams must be the source's own.
fn same_material(a: &MeshPrimitive, b: &MeshPrimitive) -> bool {
    a.uv_bindings == b.uv_bindings
        && bits(&a.base_color) == bits(&b.base_color)
        && bits(&a.metallic_roughness) == bits(&b.metallic_roughness)
        && bits(&a.emissive_factor) == bits(&b.emissive_factor)
        && a.normal_scale.to_bits() == b.normal_scale.to_bits()
        && a.occlusion_strength.to_bits() == b.occlusion_strength.to_bits()
        && a.alpha_cutoff.map(f32::to_bits) == b.alpha_cutoff.map(f32::to_bits)
        && same_texture(&a.color_texture, &b.color_texture)
        && same_texture(&a.metallic_roughness_texture, &b.metallic_roughness_texture)
        && same_texture(&a.normal_texture, &b.normal_texture)
        && same_texture(&a.occlusion_texture, &b.occlusion_texture)
        && same_texture(&a.emissive_texture, &b.emissive_texture)
        && a.normals.is_some() == b.normals.is_some()
        && a.tangents.is_some() == b.tangents.is_some()
        && a.colors.is_some() == b.colors.is_some()
        && a.uv_channels.len() == b.uv_channels.len()
        && a.uv_channels
            .iter()
            .zip(&b.uv_channels)
            .all(|(a, b)| a.set == b.set)
}
/// Bitwise equality of every decoded stream at source vertex `v` and
/// candidate vertex `k`.
fn same_vertex(s: &MeshPrimitive, v: usize, c: &MeshPrimitive, k: usize) -> bool {
    fn at<const N: usize>(values: &Option<Vec<[f32; N]>>, i: usize) -> Option<[u32; N]> {
        values.as_ref().and_then(|v| v.get(i)).map(bits)
    }
    s.positions.get(v).map(bits) == c.positions.get(k).map(bits)
        && at(&s.normals, v) == at(&c.normals, k)
        && at(&s.tangents, v) == at(&c.tangents, k)
        && at(&s.colors, v) == at(&c.colors, k)
        && s.uv_channels.iter().zip(&c.uv_channels).all(|(a, b)| {
            a.values.get(v).map(bits).is_some()
                && a.values.get(v).map(bits) == b.values.get(k).map(bits)
        })
}

/// ADR-0033 postcheck. Streams the source once: removed vertices must be
/// exactly the vertices no source index uses; every other vertex equals the
/// next candidate vertex on all decoded streams and source-map entries; every
/// candidate index equals the rank of the source index; no candidate vertex
/// is unused. Material fields and texture identities are unchanged.
pub(crate) fn check_unused(
    source: &MeshPrimitive,
    source_map: Option<&[u32]>,
    removed: &[u32],
    candidate: &MeshPrimitive,
    candidate_map: Option<&[u32]>,
    limits: VerificationLimits,
    cancel: impl FnMut() -> AssetResult<()>,
) -> AssetResult<()> {
    check_remap(
        source,
        source_map,
        removed,
        None,
        candidate,
        candidate_map,
        limits,
        cancel,
    )
}
/// ADR-0034 postcheck for a merge: like [`check_unused`], but every removed
/// vertex must be bitwise equal to its earlier, kept `targets` twin and its
/// indices must point at that twin's rank. Unused vertices are not required
/// to disappear; G012 completeness comes from complete candidate analysis.
pub(crate) fn check_merge(
    source: &MeshPrimitive,
    source_map: Option<&[u32]>,
    removed: &[u32],
    targets: &[u32],
    candidate: &MeshPrimitive,
    candidate_map: Option<&[u32]>,
    limits: VerificationLimits,
    cancel: impl FnMut() -> AssetResult<()>,
) -> AssetResult<()> {
    if targets.len() != removed.len() {
        return Err(mismatch());
    }
    check_remap(
        source,
        source_map,
        removed,
        Some(targets),
        candidate,
        candidate_map,
        limits,
        cancel,
    )
}
#[allow(clippy::too_many_arguments)]
fn check_remap(
    source: &MeshPrimitive,
    source_map: Option<&[u32]>,
    removed: &[u32],
    targets: Option<&[u32]>,
    candidate: &MeshPrimitive,
    candidate_map: Option<&[u32]>,
    limits: VerificationLimits,
    mut cancel: impl FnMut() -> AssetResult<()>,
) -> AssetResult<()> {
    cancel()?;
    let n = source.positions.len();
    let kept = n.checked_sub(removed.len()).ok_or_else(mismatch)?;
    if n > u32::MAX as usize {
        return Err(budget());
    }
    if candidate.positions.len() != kept
        || candidate.indices.len() != source.indices.len()
        || candidate_map.is_some_and(|m| m.len() != kept)
        || source_map.is_some_and(|m| m.len() != n)
        || !same_material(source, candidate)
    {
        return Err(mismatch());
    }
    for (i, &r) in removed.iter().enumerate() {
        if i % 4096 == 0 {
            cancel()?;
        }
        if r as usize >= n || i > 0 && removed[i - 1] >= r {
            return Err(mismatch());
        }
        // A merged vertex must equal an earlier vertex that survives.
        if let Some(t) = targets.map(|t| t[i])
            && (t >= r
                || removed.binary_search(&t).is_ok()
                || !same_vertex(source, t as usize, source, r as usize))
        {
            return Err(mismatch());
        }
    }
    let mut next_removed = 0usize;
    let mut k = 0usize;
    for v in 0..n {
        if v % 4096 == 0 {
            cancel()?;
        }
        if removed.get(next_removed) == Some(&(v as u32)) {
            next_removed += 1;
            continue;
        }
        let expected = source_map.map_or(v as u32, |m| m[v]);
        if !same_vertex(source, v, candidate, k)
            || candidate_map.map_or(k as u32, |m| m[k]) != expected
        {
            return Err(mismatch());
        }
        k += 1;
    }
    if k != kept || next_removed != removed.len() {
        return Err(mismatch());
    }
    // Every source index maps to the rank of its surviving (or twin) vertex.
    let mut used = reserve::<bool>(kept, limits.scratch_bytes)?;
    used.resize(kept, false);
    for (i, (&s, &c)) in source.indices.iter().zip(&candidate.indices).enumerate() {
        if i % 4096 == 0 {
            cancel()?;
        }
        let s = match (removed.binary_search(&s), targets) {
            (Ok(k), Some(targets)) => targets[k],
            (Ok(_), None) => return Err(mismatch()),
            (Err(_), _) => s,
        };
        let below = removed.partition_point(|&r| r < s);
        if c as usize != s as usize - below || c as usize >= kept {
            return Err(mismatch());
        }
        used[c as usize] = true;
    }
    cancel()?;
    // Completeness for G003: the removed set is the whole unused set.
    if targets.is_none() && used.iter().any(|u| !u) {
        return Err(mismatch());
    }
    Ok(())
}

fn key(face: &[u32; 3]) -> [u32; 3] {
    (0..3)
        .map(|start| std::array::from_fn(|i| face[(i + start) % 3]))
        .min()
        .expect("three rotations")
}
pub(crate) fn check(
    before: &[u32],
    removed: &[u32],
    after: &[u32],
    limits: VerificationLimits,
    mut cancel: impl FnMut() -> AssetResult<()>,
) -> AssetResult<()> {
    cancel()?;
    if removed.is_empty() && std::ptr::eq(before, after) {
        return Ok(());
    }
    if !before.len().is_multiple_of(3)
        || !after.len().is_multiple_of(3)
        || removed
            .len()
            .checked_mul(3)
            .and_then(|n| before.len().checked_sub(n))
            != Some(after.len())
    {
        return Err(mismatch());
    }
    let before_count = before.len() / 3;
    if before_count > u32::MAX as usize {
        return Err(budget());
    }
    for (i, &r) in removed.iter().enumerate() {
        if i % 1024 == 0 {
            cancel()?;
        }
        if r as usize >= before_count || i > 0 && removed[i - 1] >= r {
            return Err(mismatch());
        }
    }
    let mut next = 0;
    let mut read = 0;
    for (i, face) in before.as_chunks::<3>().0.iter().enumerate() {
        if i % 1024 == 0 {
            cancel()?;
        }
        if removed.get(next).is_some_and(|&r| r as usize == i) {
            next += 1;
        } else {
            if after.get(read..read + 3) != Some(face.as_slice()) {
                return Err(mismatch());
            }
            read += 3;
        }
    }
    if next != removed.len() || read != after.len() {
        return Err(mismatch());
    }
    // Both arrays are live together, with independent key reconstruction.
    let after_bytes = (after.len() / 3).checked_mul(12).ok_or_else(budget)?;
    let source_budget = limits
        .scratch_bytes
        .checked_sub(after_bytes)
        .ok_or_else(budget)?;
    let mut source = reserve::<([u32; 3], u32)>(before_count, source_budget)?;
    let source_bytes = source.capacity().checked_mul(16).ok_or_else(budget)?;
    let mut candidate = reserve::<[u32; 3]>(
        after.len() / 3,
        limits
            .scratch_bytes
            .checked_sub(source_bytes)
            .ok_or_else(budget)?,
    )?;
    for (i, face) in before.as_chunks::<3>().0.iter().enumerate() {
        if i % 1024 == 0 {
            cancel()?;
        }
        source.push((key(face), i as u32));
    }
    for (i, face) in after.as_chunks::<3>().0.iter().enumerate() {
        if i % 1024 == 0 {
            cancel()?;
        }
        candidate.push(key(face));
    }
    cancel()?;
    source.sort_unstable();
    cancel()?;
    candidate.sort_unstable();
    cancel()?;
    let mut groups = 0;
    let mut previous = None;
    for (i, &(k, ordinal)) in source.iter().enumerate() {
        if i % 1024 == 0 {
            cancel()?;
        }
        if previous == Some(k) {
            continue;
        }
        // Sorting by key then ordinal selects the earliest original survivor.
        if removed.binary_search(&ordinal).is_ok() || candidate.get(groups) != Some(&k) {
            return Err(mismatch());
        }
        groups += 1;
        previous = Some(k);
    }
    // Equality with source unique groups simultaneously proves coverage and
    // no extra duplicate candidate keys; a duplicate shifts the following key.
    if groups != candidate.len() || before_count - groups != removed.len() {
        return Err(mismatch());
    }
    cancel()?;
    Ok(())
}

/// ADR-0032 postcheck in one streaming pass without scratch memory: every
/// source face is either zero-area, listed in `removed` and absent from the
/// candidate, or non-zero and equal to the next candidate face (same order and
/// winding). The candidate must be fully consumed.
pub(crate) fn check_degenerate(
    positions: &[[f32; 3]],
    before: &[u32],
    removed: &[u32],
    after: &[u32],
    mut cancel: impl FnMut() -> AssetResult<()>,
) -> AssetResult<()> {
    cancel()?;
    if !before.len().is_multiple_of(3) || !after.len().is_multiple_of(3) {
        return Err(mismatch());
    }
    let survivors = after.as_chunks::<3>().0;
    let mut next_survivor = 0usize;
    let mut next_removed = 0usize;
    for (ordinal, face) in before.as_chunks::<3>().0.iter().enumerate() {
        if ordinal % 1024 == 0 {
            cancel()?;
        }
        let point = |i: u32| positions.get(i as usize).copied().ok_or_else(mismatch);
        let zero = ad_analysis::zero_area(point(face[0])?, point(face[1])?, point(face[2])?);
        let listed = removed.get(next_removed) == Some(&(ordinal as u32));
        match (zero, listed) {
            (true, true) => next_removed += 1,
            (false, false) => {
                if survivors.get(next_survivor) != Some(face) {
                    return Err(mismatch());
                }
                next_survivor += 1;
            }
            _ => return Err(mismatch()),
        }
    }
    cancel()?;
    if next_removed != removed.len() || next_survivor != survivors.len() {
        return Err(mismatch());
    }
    Ok(())
}
