//! Read-only document views: the current indices, or one primitive's proposed
//! indices at the revision a successful command would produce (ADR-0031).
use crate::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentTag, ErrorCode, MeshId,
    MeshPrimitive, PreparedRemoval,
};
use std::sync::Arc;

/// Proposed indices for one source primitive. Constructed only from guarded
/// command data; still validated again by `AssetDocument::candidate_view`.
#[derive(Clone, Copy)]
pub struct CandidateIndices<'a> {
    base: DocumentTag,
    next: DocumentTag,
    owner: &'a Arc<()>,
    mesh: MeshId,
    indices: &'a [u32],
}
impl<'a> CandidateIndices<'a> {
    pub(crate) fn new(
        document: &'a AssetDocument,
        mesh: MeshId,
        indices: &'a [u32],
    ) -> AssetResult<Self> {
        if mesh.0 >= document.parts.meshes.len() {
            return Err(invalid());
        }
        let base = document.tag();
        let revision = base.revision.checked_add(1).ok_or_else(|| {
            AssetError::new(ErrorCode::ResourceLimit, "Document revision limit reached")
        })?;
        Ok(Self {
            base,
            next: DocumentTag {
                document: base.document,
                revision,
            },
            owner: &document.origin,
            mesh,
            indices,
        })
    }
    pub fn mesh(&self) -> MeshId {
        self.mesh
    }
    /// Tag the document will carry if this exact change is published.
    pub fn tag(&self) -> DocumentTag {
        self.next
    }
}

/// Immutable view used by snapshots and analysis. A candidate view never
/// mutates the document and carries the future tag, so derived data can be
/// prepared before publication and becomes current only after it.
#[derive(Clone, Copy)]
pub struct DocumentView<'a> {
    document: &'a AssetDocument,
    tag: DocumentTag,
    candidate: Option<(MeshId, &'a [u32])>,
}
impl<'a> DocumentView<'a> {
    pub fn document(&self) -> &'a AssetDocument {
        self.document
    }
    pub fn tag(&self) -> DocumentTag {
        self.tag
    }
    pub fn is_candidate(&self) -> bool {
        self.candidate.is_some()
    }
    pub fn primitives(&self) -> &'a [MeshPrimitive] {
        &self.document.parts.meshes
    }
    /// Indices of a source primitive as seen through this view.
    pub fn indices(&self, mesh: MeshId) -> &'a [u32] {
        match self.candidate {
            Some((id, indices)) if id == mesh => indices,
            _ => &self.document.parts.meshes[mesh.0].indices,
        }
    }
}

fn invalid() -> AssetError {
    AssetError::new(
        ErrorCode::InvalidGeometry,
        "Candidate indices must be complete in-range triangles of an existing primitive",
    )
}

impl AssetDocument {
    pub fn view(&self) -> DocumentView<'_> {
        DocumentView {
            document: self,
            tag: self.tag(),
            candidate: None,
        }
    }
    /// Validates origin, current revision and every candidate index before any
    /// derived read. Refusal leaves no partial state; nothing is mutated.
    pub fn candidate_view<'a>(
        &'a self,
        candidate: CandidateIndices<'a>,
        cancel: &Cancellation,
    ) -> AssetResult<DocumentView<'a>> {
        self.verify_tag(candidate.base, cancel)?;
        if !Arc::ptr_eq(candidate.owner, &self.origin) {
            return Err(AssetError::new(
                ErrorCode::Conflict,
                "Candidate belongs to another document",
            ));
        }
        let mesh = self
            .parts
            .meshes
            .get(candidate.mesh.0)
            .ok_or_else(invalid)?;
        if candidate.indices.is_empty() || !candidate.indices.len().is_multiple_of(3) {
            return Err(invalid());
        }
        for (i, &index) in candidate.indices.iter().enumerate() {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            if index as usize >= mesh.positions.len() {
                return Err(invalid());
            }
        }
        cancel.check()?;
        Ok(DocumentView {
            document: self,
            tag: candidate.next,
            candidate: Some((candidate.mesh, candidate.indices)),
        })
    }
}

impl PreparedRemoval {
    /// Future indices of a non-empty prepared removal. A no-op never changes
    /// revision, so it has no candidate to preview and is refused.
    pub fn candidate<'a>(
        &'a self,
        document: &'a AssetDocument,
        cancel: &Cancellation,
    ) -> AssetResult<CandidateIndices<'a>> {
        let indices = self.indices_after(document, cancel)?;
        if !self.changes_indices() {
            return Err(AssetError::new(
                ErrorCode::Unsupported,
                "A no-op command has no candidate revision",
            ));
        }
        CandidateIndices::new(document, self.preview().mesh, indices)
    }
}
