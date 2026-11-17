use crate::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentId, ErrorCode, ImportLimits,
    MeshId, geometry_bytes, source_uv_bytes,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocumentTag {
    pub document: DocumentId,
    pub revision: u64,
}
/// Private origin plus public version: safe against reused public IDs.
#[derive(Clone)]
pub struct DocumentStamp {
    tag: DocumentTag,
    owner: Arc<()>,
}
impl DocumentStamp {
    pub fn tag(&self) -> DocumentTag {
        self.tag
    }
    pub fn is_current(&self, document: &AssetDocument) -> bool {
        self.tag == document.tag() && Arc::ptr_eq(&self.owner, &document.origin)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemovalPreview {
    pub tag: DocumentTag,
    pub mesh: MeshId,
    pub before_triangles: usize,
    pub after_triangles: usize,
}
/// Opaque, single-use proposal. It never owns a mutable document reference.
pub struct PreparedRemoval {
    preview: RemovalPreview,
    owner: Arc<()>,
    replacement: Option<Vec<u32>>,
    retained_bytes: usize,
}
impl PreparedRemoval {
    /// Immutable candidate for independent checks; never publishes an edit.
    /// Both borrows keep the proposal and current document alive for the slice.
    pub fn indices_after<'a>(
        &'a self,
        document: &'a AssetDocument,
        cancel: &Cancellation,
    ) -> AssetResult<&'a [u32]> {
        document.verify_tag(self.preview.tag, cancel)?;
        if !Arc::ptr_eq(&self.owner, &document.origin) {
            return Err(conflict());
        }
        Ok(self
            .replacement
            .as_deref()
            .unwrap_or(&document.parts.meshes[self.preview.mesh.0].indices))
    }
    pub fn preview(&self) -> RemovalPreview {
        self.preview
    }
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
    pub(crate) fn changes_indices(&self) -> bool {
        self.replacement.is_some()
    }
}
/// An applied change owns the alternate index buffer, never copied attributes.
pub struct IndexExchange {
    owner: Arc<()>,
    mesh: MeshId,
    alternate: Vec<u32>,
    retained_bytes: usize,
}
impl IndexExchange {
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
    /// Read-only indices this exchange would publish next (Undo or Redo
    /// target), tagged with the revision that publication would produce.
    pub fn candidate<'a>(
        &'a self,
        document: &'a AssetDocument,
    ) -> AssetResult<crate::CandidateIndices<'a>> {
        if !Arc::ptr_eq(&self.owner, &document.origin) {
            return Err(conflict());
        }
        crate::CandidateIndices::new(document, self.mesh, &self.alternate)
    }
}
pub struct AppliedRemoval {
    pub tag: DocumentTag,
    pub change: Option<IndexExchange>,
}
fn budget() -> AssetError {
    AssetError::new(
        ErrorCode::ResourceLimit,
        "Index command exceeds resource or revision limits",
    )
}
fn conflict() -> AssetError {
    AssetError::new(
        ErrorCode::Conflict,
        "Command document or revision is no longer current",
    )
}
fn invalid() -> AssetError {
    AssetError::new(
        ErrorCode::InvalidGeometry,
        "Triangle removals must be sorted, unique and in range",
    )
}

fn copy_retained(
    indices: &[u32],
    removed: &[u32],
    mut check: impl FnMut() -> AssetResult<()>,
) -> AssetResult<Vec<u32>> {
    let len = indices
        .len()
        .checked_sub(removed.len().checked_mul(3).ok_or_else(budget)?)
        .ok_or_else(invalid)?;
    let mut result = Vec::new();
    result.try_reserve_exact(len).map_err(|_| budget())?;
    let mut next = removed.iter().peekable();
    for (index, face) in indices.as_chunks::<3>().0.iter().enumerate() {
        if index % 1024 == 0 {
            check()?;
        }
        if next.peek().is_some_and(|&&v| v as usize == index) {
            next.next();
        } else {
            result.extend_from_slice(face);
        }
    }
    check()?;
    Ok(result)
}

impl AssetDocument {
    pub fn stamp(&self) -> DocumentStamp {
        DocumentStamp {
            tag: self.tag(),
            owner: self.origin.clone(),
        }
    }
    pub fn tag(&self) -> DocumentTag {
        DocumentTag {
            document: self.id(),
            revision: self.revision(),
        }
    }
    pub fn verify_tag(&self, expected: DocumentTag, cancel: &Cancellation) -> AssetResult<()> {
        cancel.check()?;
        if self.tag() != expected {
            return Err(conflict());
        }
        Ok(())
    }
    /// Current allocation size is a lower bound for the history reservation.
    pub fn index_capacity_bytes(&self, mesh: MeshId) -> AssetResult<usize> {
        self.parts
            .meshes
            .get(mesh.0)
            .ok_or_else(invalid)?
            .indices
            .capacity()
            .checked_mul(4)
            .ok_or_else(budget)
    }
    pub fn prepare_triangle_removal(
        &self,
        expected: DocumentTag,
        mesh: MeshId,
        removed: &[u32],
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        self.verify_tag(expected, cancel)?;
        let current = self.parts.meshes.get(mesh.0).ok_or_else(invalid)?;
        let count = current.indices.len() / 3;
        let mut previous = None;
        for (i, &index) in removed.iter().enumerate() {
            if i % 1024 == 0 {
                cancel.check()?;
            }
            if index as usize >= count || previous.is_some_and(|p| p >= index) {
                return Err(invalid());
            }
            previous = Some(index);
        }
        if !removed.is_empty() && removed.len() == count {
            return Err(AssetError::new(
                ErrorCode::Unsupported,
                "Removing all triangles of a primitive is not supported yet",
            ));
        }
        let after = count - removed.len();
        let replacement = if removed.is_empty() {
            None
        } else {
            self.check_index_budget(mesh, after * 3, limits, cancel)?;
            Some(copy_retained(&current.indices, removed, || cancel.check())?)
        };
        let retained_bytes = match &replacement {
            Some(v) => current
                .indices
                .capacity()
                .max(v.capacity())
                .checked_mul(4)
                .ok_or_else(budget)?,
            None => 0,
        };
        cancel.check()?;
        Ok(PreparedRemoval {
            preview: RemovalPreview {
                tag: expected,
                mesh,
                before_triangles: count,
                after_triangles: after,
            },
            owner: self.origin.clone(),
            replacement,
            retained_bytes,
        })
    }
    fn check_index_budget(
        &self,
        mesh: MeshId,
        replacement_len: usize,
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<()> {
        let mut total = 0usize;
        for (index, m) in self.parts.meshes.iter().enumerate() {
            cancel.check()?;
            let indices = if index == mesh.0 {
                replacement_len
            } else {
                m.indices.len()
            };
            let bytes = geometry_bytes(m.positions.len(), indices)?;
            let uv = source_uv_bytes(m.positions.len(), m.uv_channels.len())?;
            total = total
                .checked_add(bytes)
                .and_then(|v| v.checked_add(uv))
                .ok_or_else(budget)?;
            if total > limits.decoded_bytes {
                return Err(budget());
            }
        }
        Ok(())
    }
    /// Low-level command commit. Application history must admit before calling.
    /// A no-op plan is verified and returns no change; otherwise stage + publish.
    pub fn apply_triangle_removal(
        &mut self,
        plan: PreparedRemoval,
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<AppliedRemoval> {
        if plan.replacement.is_none() {
            self.verify_tag(plan.preview.tag, cancel)?;
            if !Arc::ptr_eq(&self.origin, &plan.owner) {
                return Err(conflict());
            }
            return Ok(AppliedRemoval {
                tag: self.tag(),
                change: None,
            });
        }
        let change = self.stage_triangle_removal(plan, limits, cancel)?.publish();
        Ok(AppliedRemoval {
            tag: self.tag(),
            change: Some(change),
        })
    }
    /// Runs every check of a non-empty removal without mutation (ADR-0031).
    /// Dropping the stage leaves the document untouched; `publish` cannot fail.
    pub fn stage_triangle_removal(
        &mut self,
        plan: PreparedRemoval,
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<StagedRemoval<'_>> {
        self.verify_tag(plan.preview.tag, cancel)?;
        if !Arc::ptr_eq(&self.origin, &plan.owner) {
            return Err(conflict());
        }
        let Some(replacement) = plan.replacement else {
            return Err(AssetError::new(
                ErrorCode::Unsupported,
                "A no-op command has nothing to publish",
            ));
        };
        self.check_index_budget(plan.preview.mesh, replacement.len(), limits, cancel)?;
        let next = self.revision.checked_add(1).ok_or_else(budget)?;
        cancel.check()?;
        Ok(StagedRemoval {
            document: self,
            change: IndexExchange {
                owner: plan.owner,
                mesh: plan.preview.mesh,
                alternate: replacement,
                retained_bytes: plan.retained_bytes,
            },
            next,
        })
    }
    /// Exchange an applied alternate buffer for Undo/Redo. No geometry allocation.
    /// The caller must select the correct next history entry; the tag does not
    /// enforce replay order. ad-commands::Session owns that ordered cursor.
    pub fn exchange_indices(
        &mut self,
        expected: DocumentTag,
        change: &mut IndexExchange,
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<DocumentTag> {
        Ok(self
            .stage_exchange(expected, change, limits, cancel)?
            .publish())
    }
    /// Runs every check of an Undo/Redo exchange without mutation (ADR-0031).
    pub fn stage_exchange<'a>(
        &'a mut self,
        expected: DocumentTag,
        change: &'a mut IndexExchange,
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<StagedExchange<'a>> {
        self.verify_tag(expected, cancel)?;
        if !Arc::ptr_eq(&self.origin, &change.owner) {
            return Err(conflict());
        }
        let mesh = self.parts.meshes.get(change.mesh.0).ok_or_else(invalid)?;
        let bytes = mesh
            .indices
            .capacity()
            .max(change.alternate.capacity())
            .checked_mul(4)
            .ok_or_else(budget)?;
        if bytes > change.retained_bytes {
            return Err(budget());
        }
        self.check_index_budget(change.mesh, change.alternate.len(), limits, cancel)?;
        let next = self.revision.checked_add(1).ok_or_else(budget)?;
        cancel.check()?;
        Ok(StagedExchange {
            document: self,
            change,
            next,
        })
    }
}

/// A fully checked removal. Holding it borrows the document mutably, so no
/// other change can intervene; `publish` only swaps buffers and sets revision.
pub struct StagedRemoval<'a> {
    document: &'a mut AssetDocument,
    change: IndexExchange,
    next: u64,
}
impl StagedRemoval<'_> {
    pub fn tag_after(&self) -> DocumentTag {
        DocumentTag {
            document: self.document.id(),
            revision: self.next,
        }
    }
    /// Infallible: no allocation, no checks. Returns the Undo payload.
    pub fn publish(mut self) -> IndexExchange {
        std::mem::swap(
            &mut self.document.parts.meshes[self.change.mesh.0].indices,
            &mut self.change.alternate,
        );
        self.document.revision = self.next;
        self.change
    }
}
/// A fully checked Undo/Redo exchange; see [`StagedRemoval`].
pub struct StagedExchange<'a> {
    document: &'a mut AssetDocument,
    change: &'a mut IndexExchange,
    next: u64,
}
impl StagedExchange<'_> {
    pub fn tag_after(&self) -> DocumentTag {
        DocumentTag {
            document: self.document.id(),
            revision: self.next,
        }
    }
    /// Infallible: swaps the alternate buffer and sets the checked revision.
    pub fn publish(self) -> DocumentTag {
        std::mem::swap(
            &mut self.document.parts.meshes[self.change.mesh.0].indices,
            &mut self.change.alternate,
        );
        self.document.revision = self.next;
        self.document.tag()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancel_during_candidate_copy_returns_no_partial_index_buffer() {
        let cancel = Cancellation::default();
        let mut probes = 0;
        let result = copy_retained(
            &std::iter::repeat_n([0, 1, 2], 2050)
                .flatten()
                .collect::<Vec<_>>(),
            &[0],
            || {
                probes += 1;
                if probes == 2 {
                    cancel.cancel();
                }
                cancel.check()
            },
        );
        assert_eq!(probes, 2);
        assert!(matches!(result,Err(e) if e.code==ErrorCode::Cancelled));
    }
}
