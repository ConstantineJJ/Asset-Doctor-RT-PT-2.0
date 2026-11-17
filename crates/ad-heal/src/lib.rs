//! Bounded read-only repair plans; no renderer, IO, format or report ownership.
mod verify;
use ad_commands::Session;
use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentStamp, DocumentTag, ErrorCode,
    MeshId, PreparedRemoval,
};

#[derive(Clone, Copy)]
pub struct PlanLimits {
    pub triangles: usize,
    pub scratch_bytes: usize,
    pub result_bytes: usize,
}
impl Default for PlanLimits {
    fn default() -> Self {
        Self {
            triangles: 4_000_000,
            scratch_bytes: 64 << 20,
            result_bytes: 16 << 20,
        }
    }
}
#[derive(Clone, Copy)]
pub struct VerificationLimits {
    pub scratch_bytes: usize,
}
impl Default for VerificationLimits {
    fn default() -> Self {
        Self {
            scratch_bytes: 128 << 20,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DuplicatePreview {
    pub tag: DocumentTag,
    pub mesh: MeshId,
    pub before_triangles: usize,
    pub after_triangles: usize,
    /// All source nodes referencing this primitive, including inactive nodes.
    /// This is not a count of currently displayed instances.
    pub source_node_references: usize,
}
pub struct DuplicatePlan {
    stamp: DocumentStamp,
    preview: DuplicatePreview,
    removed: Vec<u32>,
}
impl DuplicatePlan {
    pub fn preview(&self) -> DuplicatePreview {
        self.preview
    }
    pub fn removed_triangles(&self) -> &[u32] {
        &self.removed
    }
    /// Retained Vec capacity only; excludes the fixed plan header.
    pub fn retained_bytes(&self) -> usize {
        self.removed.capacity() * std::mem::size_of::<u32>()
    }
    pub fn is_current(&self, document: &AssetDocument) -> bool {
        self.stamp.is_current(document)
    }
    /// Verifies the complete index transformation before returning a generic
    /// command. This is not material/export eligibility or desktop publication.
    pub fn prepare(
        &self,
        session: &Session,
        limits: VerificationLimits,
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        cancel.check()?;
        let doc = session.document();
        if !self.is_current(doc) {
            return Err(AssetError::new(
                ErrorCode::Conflict,
                "Duplicate plan is stale or belongs to another document",
            ));
        }
        let command =
            session.prepare_removal(self.preview.tag, self.preview.mesh, &self.removed, cancel)?;
        let candidate = command.indices_after(doc, cancel)?;
        verify::check(
            &doc.primitives()[self.preview.mesh.0].indices,
            &self.removed,
            candidate,
            limits,
            || cancel.check(),
        )?;
        Ok(command)
    }
}
fn budget() -> AssetError {
    AssetError::new(
        ErrorCode::ResourceLimit,
        "Duplicate-removal planning or verification limit exceeded",
    )
}
fn mismatch() -> AssetError {
    AssetError::new(
        ErrorCode::VerificationFailed,
        "Duplicate-removal candidate failed independent verification",
    )
}
fn reserve<T>(count: usize, bytes: usize) -> AssetResult<Vec<T>> {
    if count
        .checked_mul(std::mem::size_of::<T>())
        .is_none_or(|n| n > bytes)
    {
        return Err(budget());
    }
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| budget())?;
    if values
        .capacity()
        .checked_mul(std::mem::size_of::<T>())
        .is_none_or(|n| n > bytes)
    {
        return Err(budget());
    }
    Ok(values)
}

/// Complete read-only scan of one source primitive. Diagnostic samples are not
/// accepted as input. A successful empty plan means this scope has no duplicates.
pub fn plan_duplicates(
    document: &AssetDocument,
    expected: DocumentTag,
    mesh: MeshId,
    limits: PlanLimits,
    cancel: &Cancellation,
) -> AssetResult<DuplicatePlan> {
    document.verify_tag(expected, cancel)?;
    let primitive = document.primitives().get(mesh.0).ok_or_else(|| {
        AssetError::new(
            ErrorCode::InvalidGeometry,
            "Unknown duplicate-removal source primitive",
        )
    })?;
    let removed = scan(&primitive.indices, limits, || cancel.check())?;
    let references = source_references(document, mesh, limits.scratch_bytes, || cancel.check())?;
    cancel.check()?;
    Ok(DuplicatePlan {
        stamp: document.stamp(),
        preview: DuplicatePreview {
            tag: expected,
            mesh,
            before_triangles: primitive.indices.len() / 3,
            after_triangles: primitive.indices.len() / 3 - removed.len(),
            source_node_references: references,
        },
        removed,
    })
}

fn source_references(
    document: &AssetDocument,
    mesh: MeshId,
    bytes: usize,
    mut check: impl FnMut() -> AssetResult<()>,
) -> AssetResult<usize> {
    // Planner records have already retired. Group shared source primitive lists
    // by slice identity so thousands of instances do not rescan a large list.
    let mut lists = reserve::<&[MeshId]>(document.nodes().len(), bytes)?;
    for (i, node) in document.nodes().iter().enumerate() {
        if i % 1024 == 0 {
            check()?;
        }
        lists.push(&node.meshes);
    }
    let identity = |list: &&[MeshId]| (list.as_ptr() as usize, list.len());
    check()?;
    lists.sort_unstable_by_key(identity);
    check()?;
    let mut references = 0;
    let mut start = 0;
    while start < lists.len() {
        check()?;
        let mut end = start + 1;
        while end < lists.len() && identity(&lists[end]) == identity(&lists[start]) {
            if end % 1024 == 0 {
                check()?;
            }
            end += 1;
        }
        for (i, &id) in lists[start].iter().enumerate() {
            if i % 1024 == 0 {
                check()?;
            }
            if id == mesh {
                references += end - start;
                break;
            }
        }
        start = end;
    }
    check()?;
    Ok(references)
}

fn scan(
    indices: &[u32],
    limits: PlanLimits,
    mut check: impl FnMut() -> AssetResult<()>,
) -> AssetResult<Vec<u32>> {
    check()?;
    let count = indices.len() / 3;
    if count > limits.triangles || count > u32::MAX as usize {
        return Err(budget());
    }
    if !indices.len().is_multiple_of(3) {
        return Err(mismatch());
    }
    let mut records = reserve::<([u32; 3], u32)>(count, limits.scratch_bytes)?;
    for (ordinal, face) in indices.as_chunks::<3>().0.iter().enumerate() {
        if ordinal % 1024 == 0 {
            check()?;
        }
        let [a, b, c] = *face;
        records.push(([a, b, c].min([b, c, a]).min([c, a, b]), ordinal as u32));
    }
    check()?;
    records.sort_unstable();
    check()?;
    let mut duplicate_count = 0;
    for (i, pair) in records.windows(2).enumerate() {
        if i % 1024 == 0 {
            check()?;
        }
        if pair[0].0 == pair[1].0 {
            duplicate_count += 1;
        }
    }
    let mut removed = reserve::<u32>(duplicate_count, limits.result_bytes)?;
    for (i, pair) in records.windows(2).enumerate() {
        if i % 1024 == 0 {
            check()?;
        }
        if pair[0].0 == pair[1].0 {
            removed.push(pair[1].1);
        }
    }
    drop(records);
    check()?;
    removed.sort_unstable();
    check()?;
    Ok(removed)
}
#[cfg(test)]
mod tests;
