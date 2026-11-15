//! Renderer-independent immutable asset data and revision-tagged snapshots.
use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidFormat,
    Unsupported,
    ResourceLimit,
    Cancelled,
    InvalidGeometry,
    InvalidScene,
    Io,
}

#[derive(Debug, Clone)]
pub struct AssetError {
    pub code: ErrorCode,
    pub message: String,
}
impl AssetError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for AssetError {}
pub type AssetResult<T> = Result<T, AssetError>;

#[derive(Default, Clone)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn check(&self) -> AssetResult<()> {
        if self.0.load(Ordering::Relaxed) {
            Err(AssetError::new(ErrorCode::Cancelled, "Import cancelled"))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ImportLimits {
    pub source_bytes: usize,
    pub json_bytes: usize,
    pub decoded_bytes: usize,
    pub snapshot_bytes: usize,
    pub nodes: usize,
    pub draws: usize,
}
impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            source_bytes: 192 << 20,
            json_bytes: 16 << 20,
            decoded_bytes: 512 << 20,
            snapshot_bytes: 512 << 20,
            nodes: 50_000,
            draws: 100_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocumentId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeId(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshId(pub usize);

pub struct MeshPrimitive {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub base_color: [f32; 4],
}
pub struct Node {
    pub name: String,
    pub local: [[f32; 4]; 4],
    pub children: Vec<NodeId>,
    pub meshes: Arc<[MeshId]>,
}
pub struct DocumentParts {
    pub id: DocumentId,
    pub name: String,
    pub source: Arc<[u8]>,
    pub meshes: Vec<MeshPrimitive>,
    pub nodes: Vec<Node>,
    pub roots: Vec<NodeId>,
    pub notes: Vec<String>,
}
pub struct AssetDocument {
    parts: DocumentParts,
}

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}
pub struct DrawRange {
    pub index_start: u32,
    pub index_count: u32,
    pub base_color: [f32; 4],
}
pub struct GeometrySnapshot {
    pub document_id: DocumentId,
    pub revision: u64,
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub draws: Vec<DrawRange>,
    pub bounds: Bounds,
}

impl AssetDocument {
    pub fn new(
        parts: DocumentParts,
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<Self> {
        cancel.check()?;
        if parts.source.len() > limits.source_bytes || parts.nodes.len() > limits.nodes {
            return Err(AssetError::new(
                ErrorCode::ResourceLimit,
                "Asset exceeds source or node budget",
            ));
        }
        let mut bytes = 0usize;
        for mesh in &parts.meshes {
            cancel.check()?;
            bytes = bytes
                .checked_add(geometry_bytes(mesh.positions.len(), mesh.indices.len())?)
                .ok_or_else(|| {
                    AssetError::new(ErrorCode::ResourceLimit, "Decoded geometry size overflow")
                })?;
            if bytes > limits.decoded_bytes {
                return Err(AssetError::new(
                    ErrorCode::ResourceLimit,
                    "Decoded geometry exceeds import budget",
                ));
            }
            if mesh.indices.is_empty()
                || !mesh.indices.len().is_multiple_of(3)
                || mesh
                    .indices
                    .iter()
                    .any(|&i| i as usize >= mesh.positions.len())
                || mesh
                    .positions
                    .iter()
                    .flatten()
                    .any(|value| !value.is_finite())
                || mesh
                    .base_color
                    .iter()
                    .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
            {
                return Err(AssetError::new(
                    ErrorCode::InvalidGeometry,
                    "Invalid triangle indices, positions or material color",
                ));
            }
        }
        if parts.roots.is_empty() || parts.roots.iter().any(|root| root.0 >= parts.nodes.len()) {
            return Err(AssetError::new(
                ErrorCode::InvalidScene,
                "Asset has no valid scene roots",
            ));
        }
        // Source instances share primitive IDs. Validate each retained slice once
        // rather than multiplying primitive checks by the number of instances.
        let mut checked_mesh_lists = std::collections::HashSet::new();
        for node in &parts.nodes {
            cancel.check()?;
            if checked_mesh_lists.insert((node.meshes.as_ptr(), node.meshes.len())) {
                for (i, id) in node.meshes.iter().enumerate() {
                    if i % 4096 == 0 {
                        cancel.check()?;
                    }
                    if id.0 >= parts.meshes.len() {
                        return Err(AssetError::new(
                            ErrorCode::InvalidScene,
                            "Invalid node mesh reference",
                        ));
                    }
                }
            }
            let matrix = glam::Mat4::from_cols_array_2d(&node.local);
            if !matrix.is_finite()
                || node.local[0][3] != 0.0
                || node.local[1][3] != 0.0
                || node.local[2][3] != 0.0
                || node.local[3][3] != 1.0
                || node.children.iter().any(|id| id.0 >= parts.nodes.len())
            {
                return Err(AssetError::new(
                    ErrorCode::InvalidScene,
                    "Invalid scene transform or node/mesh reference",
                ));
            }
        }
        Ok(Self { parts })
    }
    pub fn id(&self) -> DocumentId {
        self.parts.id
    }
    pub fn name(&self) -> &str {
        &self.parts.name
    }
    pub fn source(&self) -> &Arc<[u8]> {
        &self.parts.source
    }
    pub fn nodes(&self) -> &[Node] {
        &self.parts.nodes
    }
    pub fn roots(&self) -> &[NodeId] {
        &self.parts.roots
    }
    pub fn notes(&self) -> &[String] {
        &self.parts.notes
    }
    pub fn snapshot(
        &self,
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<GeometrySnapshot> {
        use glam::{Mat4, Vec3};
        cancel.check()?;
        let mut visited = vec![false; self.parts.nodes.len()];
        let mut pending: Vec<_> = self
            .parts
            .roots
            .iter()
            .rev()
            .map(|id| (*id, Mat4::IDENTITY))
            .collect();
        let mut instances = Vec::new();
        let (mut vertices, mut indices) = (0usize, 0usize);
        while let Some((id, parent)) = pending.pop() {
            cancel.check()?;
            if visited[id.0] {
                return Err(AssetError::new(
                    ErrorCode::InvalidScene,
                    "Scene contains a cycle or node with multiple parents",
                ));
            }
            visited[id.0] = true;
            let node = &self.parts.nodes[id.0];
            let world = parent * Mat4::from_cols_array_2d(&node.local);
            if !world.is_finite() {
                return Err(AssetError::new(
                    ErrorCode::InvalidScene,
                    "World transform overflow",
                ));
            }
            for mesh_id in node.meshes.iter() {
                if instances.len() >= limits.draws {
                    return Err(AssetError::new(
                        ErrorCode::ResourceLimit,
                        "Scene exceeds draw/instance budget",
                    ));
                }
                let mesh = &self.parts.meshes[mesh_id.0];
                vertices = vertices.checked_add(mesh.positions.len()).ok_or_else(|| {
                    AssetError::new(ErrorCode::ResourceLimit, "Snapshot vertex count overflow")
                })?;
                indices = indices.checked_add(mesh.indices.len()).ok_or_else(|| {
                    AssetError::new(ErrorCode::ResourceLimit, "Snapshot index count overflow")
                })?;
                if geometry_bytes(vertices, indices)? > limits.snapshot_bytes
                    || vertices > u32::MAX as usize
                    || indices > u32::MAX as usize
                {
                    return Err(AssetError::new(
                        ErrorCode::ResourceLimit,
                        "Instanced scene exceeds snapshot/GPU geometry budget",
                    ));
                }
                instances.push((*mesh_id, world));
            }
            pending.extend(node.children.iter().rev().map(|id| (*id, world)));
        }
        if indices == 0 {
            return Err(AssetError::new(
                ErrorCode::Unsupported,
                "Scene has no visible static triangle geometry",
            ));
        }
        let mut result = GeometrySnapshot {
            document_id: self.id(),
            revision: 0,
            positions: Vec::new(),
            indices: Vec::new(),
            draws: Vec::new(),
            bounds: Bounds {
                min: [f32::INFINITY; 3],
                max: [f32::NEG_INFINITY; 3],
            },
        };
        result.positions.try_reserve_exact(vertices).map_err(|_| {
            AssetError::new(ErrorCode::ResourceLimit, "Cannot allocate scene vertices")
        })?;
        result.indices.try_reserve_exact(indices).map_err(|_| {
            AssetError::new(ErrorCode::ResourceLimit, "Cannot allocate scene indices")
        })?;
        result
            .draws
            .try_reserve_exact(instances.len())
            .map_err(|_| {
                AssetError::new(ErrorCode::ResourceLimit, "Cannot allocate scene draws")
            })?;
        for (mesh_id, world) in instances {
            let mesh = &self.parts.meshes[mesh_id.0];
            let offset = result.positions.len() as u32;
            for (i, point) in mesh.positions.iter().enumerate() {
                if i % 4096 == 0 {
                    cancel.check()?;
                }
                let position = world.transform_point3(Vec3::from_array(*point));
                if !position.is_finite() || position.abs().max_element() > 1.0e12 {
                    return Err(AssetError::new(
                        ErrorCode::InvalidGeometry,
                        "Scene coordinates exceed the supported finite range",
                    ));
                }
                result.positions.push(position.to_array());
            }
            result.draws.push(DrawRange {
                index_start: result.indices.len() as u32,
                index_count: mesh.indices.len() as u32,
                base_color: mesh.base_color,
            });
            for (i, &index) in mesh.indices.iter().enumerate() {
                if i % 4096 == 0 {
                    cancel.check()?;
                }
                let position = result.positions[(offset + index) as usize];
                for (axis, value) in position.into_iter().enumerate() {
                    result.bounds.min[axis] = result.bounds.min[axis].min(value);
                    result.bounds.max[axis] = result.bounds.max[axis].max(value);
                }
                result.indices.push(offset + index);
            }
        }
        cancel.check()?;
        Ok(result)
    }
}

pub fn geometry_bytes(vertices: usize, indices: usize) -> AssetResult<usize> {
    vertices
        .checked_mul(12)
        .and_then(|value| {
            indices
                .checked_mul(4)
                .and_then(|bytes| value.checked_add(bytes))
        })
        .ok_or_else(|| AssetError::new(ErrorCode::ResourceLimit, "Geometry byte count overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Mat4;
    fn parts() -> DocumentParts {
        DocumentParts {
            id: DocumentId(7),
            name: "synthetic".into(),
            source: Arc::from([1, 2, 3, 4]),
            meshes: vec![MeshPrimitive {
                positions: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [99999., 0., 0.]],
                indices: vec![0, 1, 2],
                base_color: [1.; 4],
            }],
            nodes: vec![
                Node {
                    name: "parent".into(),
                    local: Mat4::from_translation(glam::vec3(5., 0., 0.)).to_cols_array_2d(),
                    children: vec![NodeId(1)],
                    meshes: Arc::from([]),
                },
                Node {
                    name: "mesh".into(),
                    local: Mat4::from_scale(glam::vec3(-2., 2., 2.)).to_cols_array_2d(),
                    children: vec![],
                    meshes: Arc::from([MeshId(0)]),
                },
            ],
            roots: vec![NodeId(0)],
            notes: vec![],
        }
    }
    #[test]
    fn snapshot_composes_authored_transforms_without_changing_source_or_fitting_to_unused_vertex() {
        let doc =
            AssetDocument::new(parts(), ImportLimits::default(), &Cancellation::default()).unwrap();
        let snapshot = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        assert_eq!(snapshot.positions[1], [3., 0., 0.]);
        assert_eq!(snapshot.bounds.min, [3., 0., 0.]);
        assert_eq!(snapshot.bounds.max, [5., 2., 0.]);
        assert_eq!(&**doc.source(), &[1, 2, 3, 4]);
        assert_eq!(
            doc.nodes()[0].local,
            Mat4::from_translation(glam::vec3(5., 0., 0.)).to_cols_array_2d()
        );
        assert_eq!(snapshot.document_id, DocumentId(7));
        assert_eq!(snapshot.revision, 0);
    }
    #[test]
    fn invalid_indices_are_refused_before_any_snapshot() {
        let mut data = parts();
        data.meshes[0].indices[2] = 999;
        assert!(matches!(
            AssetDocument::new(data, ImportLimits::default(), &Cancellation::default()),
            Err(AssetError {
                code: ErrorCode::InvalidGeometry,
                ..
            })
        ));
    }
    #[test]
    fn cancelled_document_admission_is_explicit() {
        let cancel = Cancellation::default();
        cancel.cancel();
        assert!(matches!(
            AssetDocument::new(parts(), ImportLimits::default(), &cancel),
            Err(AssetError {
                code: ErrorCode::Cancelled,
                ..
            })
        ));
    }
    #[test]
    fn snapshot_budget_refusal_does_not_mutate_document() {
        let doc =
            AssetDocument::new(parts(), ImportLimits::default(), &Cancellation::default()).unwrap();
        let limits = ImportLimits {
            snapshot_bytes: 1,
            ..Default::default()
        };
        assert!(matches!(
            doc.snapshot(limits, &Cancellation::default()),
            Err(AssetError {
                code: ErrorCode::ResourceLimit,
                ..
            })
        ));
        assert_eq!(
            doc.snapshot(ImportLimits::default(), &Cancellation::default())
                .unwrap()
                .indices,
            vec![0, 1, 2]
        );
    }
    #[test]
    fn cycle_and_repeated_parent_are_errors_not_recursive_overflow() {
        let mut data = parts();
        data.nodes[1].children.push(NodeId(0));
        let result = AssetDocument::new(data, ImportLimits::default(), &Cancellation::default())
            .and_then(|doc| doc.snapshot(ImportLimits::default(), &Cancellation::default()));
        assert!(matches!(
            result,
            Err(AssetError {
                code: ErrorCode::InvalidScene,
                ..
            })
        ));
    }
}
