//! Rebuild verification data from source/candidate, not planner working records.
use super::{VerificationLimits, budget, mismatch, reserve};
use ad_document::AssetResult;

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
