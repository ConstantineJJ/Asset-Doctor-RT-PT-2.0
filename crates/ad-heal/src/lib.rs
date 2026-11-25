//! Bounded read-only repair plans; no renderer, IO, format or report ownership.
mod normals;
mod verify;
use ad_commands::Session;
use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentStamp, DocumentTag, ErrorCode,
    MeshId, PreparedRemoval,
};
pub use normals::plan_normals;

#[derive(Clone, Copy)]
pub struct PlanLimits {
    pub triangles: usize,
    pub scratch_bytes: usize,
    pub result_bytes: usize,
}
impl Default for PlanLimits {
    fn default() -> Self {
        Self {
            triangles: 16_000_000,
            scratch_bytes: 512 << 20,
            result_bytes: 128 << 20,
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
            scratch_bytes: 1 << 30,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DuplicatePreview {
    pub tag: DocumentTag,
    pub mesh: MeshId,
    pub before_triangles: usize,
    pub after_triangles: usize,
    pub before_vertices: usize,
    pub after_vertices: usize,
    /// Normals a G004/G005 repair rewrites (ADR-0037); 0 for other kinds.
    pub changed_normals: usize,
    /// All source nodes referencing this primitive, including inactive nodes.
    /// This is not a count of currently displayed instances.
    pub source_node_references: usize,
}
/// Repairs of one source primitive: whole-triangle removals, or removal of
/// vertices no triangle uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairKind {
    /// G002 / REPAIR-06 (ADR-0030).
    DuplicateTriangles,
    /// G001 / REPAIR-01 (ADR-0032).
    DegenerateTriangles,
    /// G003 / REPAIR-02 (ADR-0033): vertex streams are compacted.
    UnusedVertices,
    /// G012 / REPAIR-04 (ADR-0034): exact duplicate vertices are merged.
    DuplicateVertices,
    /// G004 / REPAIR-03 (ADR-0037): create normals for a primitive without them.
    MissingNormals,
    /// G005 / REPAIR-03 (ADR-0037): replace zero normals, keep authored ones.
    ZeroNormals,
}
/// Complete removal plan for one source primitive and one repair kind.
/// `removed` holds triangle ordinals, or vertex ordinals for
/// [`RepairKind::UnusedVertices`].
pub struct DuplicatePlan {
    stamp: DocumentStamp,
    preview: DuplicatePreview,
    removed: Vec<u32>,
    /// For [`RepairKind::DuplicateVertices`]: the earlier twin of each removed
    /// vertex; empty otherwise.
    targets: Vec<u32>,
    /// For normals repairs: the complete replacement NORMAL stream.
    normals: Vec<[f32; 3]>,
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
    /// Triangle ordinals removed; empty for a vertex repair.
    pub fn removed_triangles(&self) -> &[u32] {
        match self.kind {
            RepairKind::DuplicateTriangles | RepairKind::DegenerateTriangles => &self.removed,
            _ => &[],
        }
    }
    /// A verified complete scan that changes nothing.
    pub fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.preview.changed_normals == 0
    }
    /// Vertex ordinals removed; empty for a triangle repair.
    pub fn removed_vertices(&self) -> &[u32] {
        match self.kind {
            RepairKind::UnusedVertices | RepairKind::DuplicateVertices => &self.removed,
            _ => &[],
        }
    }
    /// Earlier twin of each removed vertex for a merge; empty otherwise.
    pub fn merge_targets(&self) -> &[u32] {
        &self.targets
    }
    /// Retained Vec capacity only; excludes the fixed plan header.
    pub fn retained_bytes(&self) -> usize {
        (self.removed.capacity() + self.targets.capacity()) * std::mem::size_of::<u32>()
            + self.normals.capacity() * std::mem::size_of::<[f32; 3]>()
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
        let (tag, mesh) = (self.preview.tag, self.preview.mesh);
        let source = &doc.primitives()[mesh.0];
        if matches!(
            self.kind,
            RepairKind::MissingNormals | RepairKind::ZeroNormals
        ) {
            let keep = self.kind == RepairKind::ZeroNormals;
            let mut normals = reserve::<[f32; 3]>(self.normals.len(), usize::MAX)?;
            normals.extend_from_slice(&self.normals);
            let command = session.prepare_normals(tag, mesh, normals, keep, cancel)?;
            let after = command.primitive_after(doc, cancel)?;
            verify::check_normals(
                source,
                after.normals.as_deref().ok_or_else(mismatch)?,
                keep,
                || cancel.check(),
            )?;
            return Ok(command);
        }
        if self.kind == RepairKind::DuplicateVertices {
            let mut pairs = reserve::<[u32; 2]>(self.removed.len(), usize::MAX)?;
            pairs.extend(
                self.removed
                    .iter()
                    .zip(&self.targets)
                    .map(|(&r, &t)| [r, t]),
            );
            let command = session.prepare_vertex_merge(tag, mesh, &pairs, cancel)?;
            verify::check_merge(
                source,
                doc.source_vertices(mesh),
                (&self.removed, &self.targets),
                command.primitive_after(doc, cancel)?,
                command.source_vertices_after(doc),
                limits,
                || cancel.check(),
            )?;
            return Ok(command);
        }
        if self.kind == RepairKind::UnusedVertices {
            let command = session.prepare_vertex_removal(tag, mesh, &self.removed, cancel)?;
            verify::check_unused(
                source,
                doc.source_vertices(mesh),
                &self.removed,
                command.primitive_after(doc, cancel)?,
                command.source_vertices_after(doc),
                limits,
                || cancel.check(),
            )?;
            return Ok(command);
        }
        let command = session.prepare_removal(tag, mesh, &self.removed, cancel)?;
        let candidate = command.indices_after(doc, cancel)?;
        match self.kind {
            RepairKind::UnusedVertices
            | RepairKind::DuplicateVertices
            | RepairKind::MissingNormals
            | RepairKind::ZeroNormals => unreachable!("handled above"),
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
            before_vertices: primitive.positions.len(),
            after_vertices: primitive.positions.len(),
            changed_normals: 0,
            source_node_references: references,
        },
        removed,
        targets: Vec::new(),
        normals: Vec::new(),
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
            before_vertices: primitive.positions.len(),
            after_vertices: primitive.positions.len(),
            changed_normals: 0,
            source_node_references: references,
        },
        removed,
        targets: Vec::new(),
        normals: Vec::new(),
        kind: RepairKind::DegenerateTriangles,
    })
}

/// Complete read-only G003 scan of one source primitive (ADR-0033): every
/// vertex that no triangle of this primitive references, in ascending order.
/// Vertices used only by a sibling primitive sharing the source accessor are
/// unused here. An empty plan is a no-op.
pub fn plan_unused(
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
            "Unknown unused-vertex source primitive",
        )
    })?;
    let count = primitive.positions.len();
    if count > u32::MAX as usize || primitive.indices.len() / 3 > limits.triangles {
        return Err(budget());
    }
    let mut used = reserve::<bool>(count, limits.scratch_bytes)?;
    used.resize(count, false);
    for (i, &index) in primitive.indices.iter().enumerate() {
        if i % 4096 == 0 {
            cancel.check()?;
        }
        used[index as usize] = true;
    }
    let unused = used.iter().filter(|u| !**u).count();
    let mut removed = reserve::<u32>(unused, limits.result_bytes)?;
    for (v, &u) in used.iter().enumerate() {
        if v % 4096 == 0 {
            cancel.check()?;
        }
        if !u {
            removed.push(v as u32);
        }
    }
    drop(used);
    let references = source_references(document, mesh, limits.scratch_bytes, || cancel.check())?;
    cancel.check()?;
    let triangles = primitive.indices.len() / 3;
    Ok(DuplicatePlan {
        stamp: document.stamp(),
        preview: DuplicatePreview {
            tag: expected,
            mesh,
            before_triangles: triangles,
            after_triangles: triangles,
            before_vertices: count,
            after_vertices: count - removed.len(),
            changed_normals: 0,
            source_node_references: references,
        },
        removed,
        targets: Vec::new(),
        normals: Vec::new(),
        kind: RepairKind::UnusedVertices,
    })
}

/// Complete read-only G012 scan of one source primitive (ADR-0034): every
/// vertex bitwise equal to an earlier vertex on all decoded streams, paired
/// with the earliest such vertex. Uses the diagnostic's own order, so the plan
/// equals the G012 set. An empty plan is a no-op.
pub fn plan_vertex_merge(
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
            "Unknown vertex-merge source primitive",
        )
    })?;
    let count = primitive.positions.len();
    if count > u32::MAX as usize || primitive.indices.len() / 3 > limits.triangles {
        return Err(budget());
    }
    let mut order = reserve::<u32>(count, limits.scratch_bytes)?;
    order.extend(0..count as u32);
    cancel.check()?;
    order.sort_unstable_by(|&a, &b| ad_analysis::vertex_order(primitive, a, b).then(a.cmp(&b)));
    cancel.check()?;
    let mut pairs = 0usize;
    for (i, w) in order.windows(2).enumerate() {
        if i % 4096 == 0 {
            cancel.check()?;
        }
        pairs += usize::from(ad_analysis::same_vertex(primitive, w[0], w[1]));
    }
    let mut merged = reserve::<[u32; 2]>(pairs, limits.result_bytes)?;
    let mut first = order.first().copied().unwrap_or(0);
    for (i, w) in order.windows(2).enumerate() {
        if i % 4096 == 0 {
            cancel.check()?;
        }
        if ad_analysis::same_vertex(primitive, w[0], w[1]) {
            merged.push([w[1], first]);
        } else {
            first = w[1];
        }
    }
    drop(order);
    merged.sort_unstable();
    let mut removed = reserve::<u32>(merged.len(), limits.result_bytes)?;
    let mut targets = reserve::<u32>(merged.len(), limits.result_bytes)?;
    for [r, t] in merged {
        removed.push(r);
        targets.push(t);
    }
    let references = source_references(document, mesh, limits.scratch_bytes, || cancel.check())?;
    cancel.check()?;
    let triangles = primitive.indices.len() / 3;
    Ok(DuplicatePlan {
        stamp: document.stamp(),
        preview: DuplicatePreview {
            tag: expected,
            mesh,
            before_triangles: triangles,
            after_triangles: triangles,
            before_vertices: count,
            after_vertices: count - removed.len(),
            changed_normals: 0,
            source_node_references: references,
        },
        removed,
        targets,
        normals: Vec::new(),
        kind: RepairKind::DuplicateVertices,
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
mod merge_tests;
#[cfg(test)]
mod normals_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod unused_tests;
