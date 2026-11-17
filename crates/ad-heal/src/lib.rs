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
/// Index-only repairs that remove whole triangles of one source primitive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairKind {
    /// G002 / REPAIR-06 (ADR-0030).
    DuplicateTriangles,
    /// G001 / REPAIR-01 (ADR-0032).
    DegenerateTriangles,
}
/// Complete removal plan for one source primitive and one repair kind.
pub struct DuplicatePlan {
    stamp: DocumentStamp,
    preview: DuplicatePreview,
    removed: Vec<u32>,
    kind: RepairKind,
}
/// Kind-neutral name for the same plan type.
pub type RemovalPlan = DuplicatePlan;
impl DuplicatePlan {
    pub fn preview(&self) -> DuplicatePreview {
        self.preview
    }
    pub fn kind(&self) -> RepairKind {
        self.kind
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
                "Repair plan is stale or belongs to another document",
            ));
        }
        let command =
            session.prepare_removal(self.preview.tag, self.preview.mesh, &self.removed, cancel)?;
        let candidate = command.indices_after(doc, cancel)?;
        let source = &doc.primitives()[self.preview.mesh.0];
        match self.kind {
            RepairKind::DuplicateTriangles => {
                verify::check(&source.indices, &self.removed, candidate, limits, || {
                    cancel.check()
                })?
            }
            RepairKind::DegenerateTriangles => verify::check_degenerate(
                &source.positions,
                &source.indices,
                &self.removed,
                candidate,
                || cancel.check(),
            )?,
        }
        Ok(command)
    }
}
fn budget() -> AssetError {
    AssetError::new(
        ErrorCode::ResourceLimit,
        "Repair planning or verification limit exceeded",
    )
}
fn mismatch() -> AssetError {
    AssetError::new(
        ErrorCode::VerificationFailed,
        "Repair candidate failed independent verification",
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
        kind: RepairKind::DuplicateTriangles,
    })
}

/// Complete read-only G001 scan of one source primitive (ADR-0032): exactly
/// the faces the diagnostic's exact zero-area predicate reports, including
/// repeated-index faces. A fully degenerate primitive is refused because
/// empty primitives are not supported. An empty plan is a no-op.
pub fn plan_degenerate(
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
            "Unknown degenerate-removal source primitive",
        )
    })?;
    let faces = primitive.indices.as_chunks::<3>().0;
    if faces.len() > limits.triangles || faces.len() > u32::MAX as usize {
        return Err(budget());
    }
    let positions = &primitive.positions;
    let degenerate = |face: &[u32; 3]| {
        ad_analysis::zero_area(
            positions[face[0] as usize],
            positions[face[1] as usize],
            positions[face[2] as usize],
        )
    };
    let mut count = 0usize;
    for (i, face) in faces.iter().enumerate() {
        if i % 1024 == 0 {
            cancel.check()?;
        }
        count += usize::from(degenerate(face));
    }
    if count > 0 && count == faces.len() {
        return Err(AssetError::new(
            ErrorCode::Unsupported,
            "Every triangle of this primitive is degenerate; removing all of them would leave an empty primitive, which is not supported",
        ));
    }
    let mut removed = reserve::<u32>(count, limits.result_bytes)?;
    for (i, face) in faces.iter().enumerate() {
        if i % 1024 == 0 {
            cancel.check()?;
        }
        if degenerate(face) {
            removed.push(i as u32);
        }
    }
    let references = source_references(document, mesh, limits.scratch_bytes, || cancel.check())?;
    cancel.check()?;
    Ok(DuplicatePlan {
        stamp: document.stamp(),
        preview: DuplicatePreview {
            tag: expected,
            mesh,
            before_triangles: faces.len(),
            after_triangles: faces.len() - removed.len(),
            source_node_references: references,
        },
        removed,
        kind: RepairKind::DegenerateTriangles,
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
mod degenerate_tests;
#[cfg(test)]
mod tests;
