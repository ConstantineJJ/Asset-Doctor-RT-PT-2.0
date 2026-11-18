//! Read-only document views: the current data, or one primitive's proposed
//! indices — or, for a vertex compaction, its whole proposed primitive — at
//! the revision a successful command would produce (ADR-0031, ADR-0033).
use crate::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentTag, ErrorCode, MeshId,
    MeshPrimitive, PreparedRemoval,
};
use std::sync::Arc;

/// Proposed data for one primitive: new indices over the current vertex
/// streams, or a complete replacement primitive with its source-vertex map.
#[derive(Clone, Copy)]
pub(crate) enum CandidateData<'a> {
    Indices(&'a [u32]),
    Primitive(&'a MeshPrimitive, Option<&'a [u32]>),
}
impl<'a> CandidateData<'a> {
    fn indices(self) -> &'a [u32] {
        match self {
            Self::Indices(v) => v,
            Self::Primitive(p, _) => &p.indices,
        }
    }
}

/// Proposed change for one source primitive. Constructed only from guarded
/// command data; still validated again by `AssetDocument::candidate_view`.
#[derive(Clone, Copy)]
pub struct CandidateIndices<'a> {
    base: DocumentTag,
    next: DocumentTag,
    owner: &'a Arc<()>,
    mesh: MeshId,
    data: CandidateData<'a>,
}
impl<'a> CandidateIndices<'a> {
    pub(crate) fn new(
        document: &'a AssetDocument,
        mesh: MeshId,
        data: CandidateData<'a>,
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
            data,
        })
    }
    pub fn mesh(&self) -> MeshId {
        self.mesh
    }
    /// Tag the document will carry if this exact change is published.
    pub fn tag(&self) -> DocumentTag {
        self.next
    }
    /// True when publication replaces the vertex streams (ADR-0033).
    pub fn changes_vertices(&self) -> bool {
        matches!(self.data, CandidateData::Primitive(..))
    }
}

/// Immutable view used by snapshots and analysis. A candidate view never
/// mutates the document and carries the future tag, so derived data can be
/// prepared before publication and becomes current only after it.
#[derive(Clone, Copy)]
pub struct DocumentView<'a> {
    document: &'a AssetDocument,
    tag: DocumentTag,
    candidate: Option<(MeshId, CandidateData<'a>)>,
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
    /// Every source primitive as seen through this view, once each.
    pub fn primitives(&self) -> Primitives<'a> {
        Primitives { view: *self }
    }
    /// One source primitive as seen through this view. For an index-only
    /// candidate the vertex streams are current; read indices via `indices`.
    pub fn primitive(&self, mesh: MeshId) -> &'a MeshPrimitive {
        match self.candidate {
            Some((id, CandidateData::Primitive(p, _))) if id == mesh => p,
            _ => &self.document.parts.meshes[mesh.0],
        }
    }
    /// Indices of a source primitive as seen through this view.
    pub fn indices(&self, mesh: MeshId) -> &'a [u32] {
        match self.candidate {
            Some((id, data)) if id == mesh => data.indices(),
            _ => &self.document.parts.meshes[mesh.0].indices,
        }
    }
    /// Map from this view's vertices to original source vertices; `None` is
    /// the identity (ADR-0033).
    pub fn source_vertices(&self, mesh: MeshId) -> Option<&'a [u32]> {
        match self.candidate {
            Some((id, CandidateData::Primitive(_, map))) if id == mesh => map,
            _ => self.document.source_vertices(mesh),
        }
    }
}

/// Source primitives through a view; a candidate primitive replaces its slot.
#[derive(Clone, Copy)]
pub struct Primitives<'a> {
    view: DocumentView<'a>,
}
impl<'a> Primitives<'a> {
    pub fn len(&self) -> usize {
        self.view.document.parts.meshes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, index: usize) -> Option<&'a MeshPrimitive> {
        (index < self.len()).then(|| self.view.primitive(MeshId(index)))
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &'a MeshPrimitive> + 'a {
        let view = self.view;
        (0..self.len()).map(move |i| view.primitive(MeshId(i)))
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
    /// Validates origin, current revision and every candidate index (and, for
    /// a replacement primitive, its stream lengths) before any derived read.
    /// Refusal leaves no partial state; nothing is mutated.
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
        let current = self
            .parts
            .meshes
            .get(candidate.mesh.0)
            .ok_or_else(invalid)?;
        let vertices = match candidate.data {
            CandidateData::Indices(_) => current.positions.len(),
            CandidateData::Primitive(p, map) => {
                let n = p.positions.len();
                let channels_match = p.uv_channels.len() == current.uv_channels.len()
                    && p.uv_channels
                        .iter()
                        .zip(&current.uv_channels)
                        .all(|(a, b)| a.set == b.set && a.values.len() == n);
                if n == 0
                    || !channels_match
                    || p.normals.as_ref().is_some_and(|v| v.len() != n)
                    || p.tangents.as_ref().is_some_and(|v| v.len() != n)
                    || p.colors.as_ref().is_some_and(|v| v.len() != n)
                    || map.is_some_and(|m| m.len() != n)
                {
                    return Err(invalid());
                }
                n
            }
        };
        let indices = candidate.data.indices();
        if indices.is_empty() || !indices.len().is_multiple_of(3) {
            return Err(invalid());
        }
        for (i, &index) in indices.iter().enumerate() {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            if index as usize >= vertices {
                return Err(invalid());
            }
        }
        cancel.check()?;
        Ok(DocumentView {
            document: self,
            tag: candidate.next,
            candidate: Some((candidate.mesh, candidate.data)),
        })
    }
}

impl PreparedRemoval {
    /// Future data of a non-empty prepared command. A no-op never changes
    /// revision, so it has no candidate to preview and is refused.
    pub fn candidate<'a>(
        &'a self,
        document: &'a AssetDocument,
        cancel: &Cancellation,
    ) -> AssetResult<CandidateIndices<'a>> {
        self.indices_after(document, cancel)?;
        let Some(data) = self.data() else {
            return Err(AssetError::new(
                ErrorCode::Unsupported,
                "A no-op command has no candidate revision",
            ));
        };
        CandidateIndices::new(document, self.preview().mesh, data)
    }
}
