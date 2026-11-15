//! Renderer-independent immutable asset data and revision-tagged snapshots.
mod color;
mod normals;
pub use color::*;
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
    pub texture_bytes: usize,
    pub texture_side: u32,
    pub textures: usize,
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
            texture_bytes: 256 << 20,
            texture_side: 8192,
            textures: 256,
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
    pub normals: Option<Vec<[f32; 3]>>,
    pub tangents: Option<Vec<[f32; 4]>>,
    pub normal_texture: Option<Arc<Texture>>,
    pub normal_uvs: Option<Vec<[f32; 2]>>,
    pub normal_scale: f32,
    pub occlusion_texture: Option<Arc<Texture>>,
    pub occlusion_uvs: Option<Vec<[f32; 2]>>,
    pub occlusion_strength: f32,
    pub emissive_factor: [f32; 3],
    pub emissive_texture: Option<Arc<Texture>>,
    pub emissive_uvs: Option<Vec<[f32; 2]>>,
    pub metallic_roughness: [f32; 2],
    pub indices: Vec<u32>,
    pub base_color: [f32; 4],
    pub uvs: Option<Vec<[f32; 2]>>,
    pub colors: Option<Vec<[f32; 4]>>,
    pub color_texture: Option<Arc<Texture>>,
    pub metallic_roughness_texture: Option<Arc<Texture>>,
    pub metallic_roughness_uvs: Option<Vec<[f32; 2]>>,
    pub alpha_cutoff: Option<f32>,
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
    texture_stats: TextureStats,
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
    pub metallic_roughness: [f32; 2],
    pub color_texture: Option<Arc<Texture>>,
    pub metallic_roughness_texture: Option<Arc<Texture>>,
    pub normal_texture: Option<Arc<Texture>>,
    pub normal_scale: f32,
    pub occlusion_texture: Option<Arc<Texture>>,
    pub occlusion_strength: f32,
    pub emissive_factor: [f32; 3],
    pub emissive_texture: Option<Arc<Texture>>,
    pub alpha_cutoff: Option<f32>,
}
pub struct GeometrySnapshot {
    pub document_id: DocumentId,
    pub revision: u64,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,
    pub normal_uvs: Vec<[f32; 2]>,
    pub occlusion_uvs: Vec<[f32; 2]>,
    pub emissive_uvs: Vec<[f32; 2]>,
    pub fallback_tangent_vertices: usize,
    pub fallback_normal_vertices: usize,
    pub uvs: Vec<[f32; 2]>,
    pub metallic_roughness_uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
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
        let mut textures = TextureAdmission::default();
        for mesh in &parts.meshes {
            cancel.check()?;
            if mesh
                .emissive_factor
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                || mesh.emissive_uvs.as_ref().is_some_and(|uv| {
                    uv.len() != mesh.positions.len() || uv.iter().flatten().any(|v| !v.is_finite())
                })
                || (mesh.emissive_texture.is_some() && mesh.emissive_uvs.is_none())
            {
                return Err(AssetError::new(
                    ErrorCode::InvalidGeometry,
                    "Invalid emissive factor/UV attributes",
                ));
            }
            if let Some(texture) = &mesh.emissive_texture {
                textures.admit(texture, TextureEncoding::Srgb, limits)?;
            }
            if !mesh.occlusion_strength.is_finite()
                || !(0.0..=1.0).contains(&mesh.occlusion_strength)
                || mesh.occlusion_uvs.as_ref().is_some_and(|uv| {
                    uv.len() != mesh.positions.len() || uv.iter().flatten().any(|v| !v.is_finite())
                })
                || (mesh.occlusion_texture.is_some() && mesh.occlusion_uvs.is_none())
            {
                return Err(AssetError::new(
                    ErrorCode::InvalidGeometry,
                    "Invalid occlusion strength/UV attributes",
                ));
            }
            if let Some(texture) = &mesh.occlusion_texture {
                textures.admit(texture, TextureEncoding::Linear, limits)?;
            }
            if !mesh.normal_scale.is_finite()
                || mesh.tangents.as_ref().is_some_and(|t| {
                    t.len() != mesh.positions.len()
                        || t.iter().any(|v| {
                            v.iter().any(|x| !x.is_finite()) || (v[3] != -1. && v[3] != 1.)
                        })
                })
                || mesh.normal_uvs.as_ref().is_some_and(|uv| {
                    uv.len() != mesh.positions.len() || uv.iter().flatten().any(|v| !v.is_finite())
                })
                || (mesh.normal_texture.is_some() && mesh.normal_uvs.is_none())
            {
                return Err(AssetError::new(
                    ErrorCode::InvalidGeometry,
                    "Invalid tangent/normal-texture attributes",
                ));
            }
            if mesh.normals.as_ref().is_some_and(|normals| {
                normals.len() != mesh.positions.len()
                    || normals.iter().flatten().any(|v| !v.is_finite())
            }) || mesh
                .metallic_roughness
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            {
                return Err(AssetError::new(
                    ErrorCode::InvalidGeometry,
                    "Invalid normals or metallic/roughness factors",
                ));
            }
            if let Some(texture) = &mesh.color_texture {
                textures.admit(texture, TextureEncoding::Srgb, limits)?;
            }
            if let Some(texture) = &mesh.metallic_roughness_texture {
                textures.admit(texture, TextureEncoding::Linear, limits)?;
            }
            if let Some(texture) = &mesh.normal_texture {
                textures.admit(texture, TextureEncoding::Linear, limits)?;
            }
            if mesh.metallic_roughness_uvs.as_ref().is_some_and(|uv| {
                uv.len() != mesh.positions.len() || uv.iter().flatten().any(|v| !v.is_finite())
            }) || (mesh.metallic_roughness_texture.is_some()
                && mesh.metallic_roughness_uvs.is_none())
            {
                return Err(AssetError::new(
                    ErrorCode::InvalidGeometry,
                    "Invalid metallic/roughness UV attributes",
                ));
            }
            if mesh.uvs.as_ref().is_some_and(|uv| {
                uv.len() != mesh.positions.len() || uv.iter().flatten().any(|v| !v.is_finite())
            }) || mesh.colors.as_ref().is_some_and(|colors| {
                colors.len() != mesh.positions.len()
                    || colors
                        .iter()
                        .flatten()
                        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            }) || (mesh.color_texture.is_some() && mesh.uvs.is_none())
                || mesh.alpha_cutoff.is_some_and(|v| !v.is_finite() || v < 0.)
            {
                return Err(AssetError::new(
                    ErrorCode::InvalidGeometry,
                    "Invalid UV/color/material attributes",
                ));
            }
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
        Ok(Self {
            parts,
            texture_stats: textures.stats,
        })
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
    pub fn texture_stats(&self) -> TextureStats {
        self.texture_stats
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
            normals: Vec::new(),
            tangents: Vec::new(),
            normal_uvs: Vec::new(),
            occlusion_uvs: Vec::new(),
            emissive_uvs: Vec::new(),
            fallback_tangent_vertices: 0,
            fallback_normal_vertices: 0,
            uvs: Vec::new(),
            metallic_roughness_uvs: Vec::new(),
            colors: Vec::new(),
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
        result.normals.try_reserve_exact(vertices).map_err(|_| {
            AssetError::new(ErrorCode::ResourceLimit, "Cannot allocate scene normals")
        })?;
        result.tangents.try_reserve_exact(vertices).map_err(|_| {
            AssetError::new(ErrorCode::ResourceLimit, "Cannot allocate scene tangents")
        })?;
        result.normal_uvs.try_reserve_exact(vertices).map_err(|_| {
            AssetError::new(ErrorCode::ResourceLimit, "Cannot allocate normal-map UVs")
        })?;
        result
            .occlusion_uvs
            .try_reserve_exact(vertices)
            .map_err(|_| {
                AssetError::new(ErrorCode::ResourceLimit, "Cannot allocate occlusion UVs")
            })?;
        result
            .uvs
            .try_reserve_exact(vertices)
            .map_err(|_| AssetError::new(ErrorCode::ResourceLimit, "Cannot allocate scene UVs"))?;
        result.colors.try_reserve_exact(vertices).map_err(|_| {
            AssetError::new(ErrorCode::ResourceLimit, "Cannot allocate scene colors")
        })?;
        result
            .metallic_roughness_uvs
            .try_reserve_exact(vertices)
            .map_err(|_| {
                AssetError::new(
                    ErrorCode::ResourceLimit,
                    "Cannot allocate metallic/roughness UVs",
                )
            })?;
        result
            .emissive_uvs
            .try_reserve_exact(vertices)
            .map_err(|_| {
                AssetError::new(ErrorCode::ResourceLimit, "Cannot allocate emission UVs")
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
            let normal_matrix = normals::normal_matrix(world);
            let tangent_matrix = normals::tangent_matrix(world);
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
                let normal = mesh.normals.as_ref().map_or([0.; 3], |values| {
                    normals::transform_normal(normal_matrix, values[i])
                });
                result.fallback_normal_vertices += usize::from(normal == [0.; 3]);
                result.normals.push(normal);
                let tangent = mesh.tangents.as_ref().map_or([0.; 4], |values| {
                    normals::transform_tangent(tangent_matrix, normal, values[i])
                });
                result.fallback_tangent_vertices +=
                    usize::from(mesh.normal_texture.is_some() && tangent == [0.; 4]);
                result.tangents.push(tangent);
                result
                    .normal_uvs
                    .push(mesh.normal_uvs.as_ref().map_or([0.; 2], |uv| uv[i]));
                result
                    .occlusion_uvs
                    .push(mesh.occlusion_uvs.as_ref().map_or([0.; 2], |uv| uv[i]));
                result
                    .emissive_uvs
                    .push(mesh.emissive_uvs.as_ref().map_or([0.; 2], |uv| uv[i]));
                result
                    .uvs
                    .push(mesh.uvs.as_ref().map_or([0.; 2], |uv| uv[i]));
                result.metallic_roughness_uvs.push(
                    mesh.metallic_roughness_uvs
                        .as_ref()
                        .map_or([0.; 2], |uv| uv[i]),
                );
                result
                    .colors
                    .push(mesh.colors.as_ref().map_or([1.; 4], |colors| colors[i]));
            }
            result.draws.push(DrawRange {
                index_start: result.indices.len() as u32,
                index_count: mesh.indices.len() as u32,
                base_color: mesh.base_color,
                metallic_roughness: mesh.metallic_roughness,
                color_texture: mesh.color_texture.clone(),
                metallic_roughness_texture: mesh.metallic_roughness_texture.clone(),
                normal_texture: mesh.normal_texture.clone(),
                normal_scale: mesh.normal_scale,
                occlusion_texture: mesh.occlusion_texture.clone(),
                occlusion_strength: mesh.occlusion_strength,
                emissive_factor: mesh.emissive_factor,
                emissive_texture: mesh.emissive_texture.clone(),
                alpha_cutoff: mesh.alpha_cutoff,
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
        .checked_mul(96)
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
                normals: None,
                tangents: None,
                normal_texture: None,
                normal_uvs: None,
                normal_scale: 1.,
                occlusion_texture: None,
                occlusion_uvs: None,
                occlusion_strength: 1.,
                emissive_factor: [0.; 3],
                emissive_texture: None,
                emissive_uvs: None,
                metallic_roughness: [1.; 2],
                indices: vec![0, 1, 2],
                base_color: [1.; 4],
                uvs: None,
                colors: None,
                color_texture: None,
                metallic_roughness_texture: None,
                metallic_roughness_uvs: None,
                alpha_cutoff: None,
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
    fn snapshot_derives_normals_without_repairing_source_and_counts_fallbacks() {
        let mut data = parts();
        data.meshes[0].normals = Some(vec![[1., 1., 1.]; 4]);
        data.meshes[0].metallic_roughness = [0.25, 0.];
        data.nodes[1].local = Mat4::from_scale(glam::vec3(-2., 1., 0.5)).to_cols_array_2d();
        let doc =
            AssetDocument::new(data, ImportLimits::default(), &Cancellation::default()).unwrap();
        let snapshot = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        assert_eq!(doc.parts.meshes[0].normals.as_ref().unwrap()[0], [1.; 3]);
        assert_eq!(snapshot.draws[0].metallic_roughness, [0.25, 0.]);
        assert!(
            (glam::Vec3::from_array(snapshot.normals[0]) - glam::vec3(-0.5, 1., 2.).normalize())
                .length()
                < 1e-6
        );
        assert_eq!(snapshot.fallback_normal_vertices, 0);
        let mut data = parts();
        data.meshes[0].normals = Some(vec![[0., 1., 0.]; 4]);
        data.nodes[1].local = Mat4::from_scale(glam::vec3(1., 0., 1.)).to_cols_array_2d();
        let doc =
            AssetDocument::new(data, ImportLimits::default(), &Cancellation::default()).unwrap();
        let snapshot = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        assert_eq!(snapshot.normals, [[0.; 3]; 4]);
        assert_eq!(snapshot.fallback_normal_vertices, 4);
        assert_eq!(geometry_bytes(4, 3).unwrap(), 396);
        for bad in [f32::NAN, -0.1, 1.1] {
            let mut data = parts();
            data.meshes[0].metallic_roughness[0] = bad;
            assert!(
                AssetDocument::new(data, ImportLimits::default(), &Cancellation::default())
                    .is_err()
            );
        }
    }
    #[test]
    fn normal_map_snapshot_keeps_source_tangent_and_scale_and_refuses_bad_streams() {
        let mut data = parts();
        data.meshes[0].normals = Some(vec![[0., 0., 1.]; 4]);
        data.meshes[0].tangents = Some(vec![[1., 0., 0., 1.]; 4]);
        data.meshes[0].normal_uvs = Some(vec![[0.; 2]; 4]);
        data.meshes[0].normal_scale = -2.;
        data.meshes[0].normal_texture = Some(Arc::new(Texture {
            image: Arc::new(TextureImage {
                width: 1,
                height: 1,
                rgba: Arc::from([128, 128, 255, 0]),
            }),
            sampler: TextureSampler::default(),
        }));
        let doc =
            AssetDocument::new(data, ImportLimits::default(), &Cancellation::default()).unwrap();
        let snapshot = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        assert_eq!(
            doc.parts.meshes[0].tangents.as_ref().unwrap()[0],
            [1., 0., 0., 1.]
        );
        assert_eq!(snapshot.tangents[0], [-1., 0., 0., -1.]);
        assert_eq!(snapshot.draws[0].normal_scale, -2.);
        assert_eq!(snapshot.fallback_tangent_vertices, 0);
        assert!(
            doc.snapshot(
                ImportLimits {
                    snapshot_bytes: 331,
                    ..Default::default()
                },
                &Cancellation::default()
            )
            .is_err()
        );
        for bad_w in [0., 2., f32::NAN] {
            let mut data = parts();
            data.meshes[0].tangents = Some(vec![[1., 0., 0., bad_w]; 4]);
            assert!(
                AssetDocument::new(data, ImportLimits::default(), &Cancellation::default())
                    .is_err()
            );
        }
        let mut data = parts();
        data.meshes[0].normal_scale = f32::INFINITY;
        assert!(
            AssetDocument::new(data, ImportLimits::default(), &Cancellation::default()).is_err()
        );
        let mut data = parts();
        data.meshes[0].normal_uvs = Some(vec![[0.; 2]; 3]);
        assert!(
            AssetDocument::new(data, ImportLimits::default(), &Cancellation::default()).is_err()
        );
    }
    #[test]
    fn occlusion_strength_is_finite_bounded_and_uvs_match_geometry() {
        for strength in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            let mut data = parts();
            data.meshes[0].occlusion_strength = strength;
            assert!(
                AssetDocument::new(data, ImportLimits::default(), &Cancellation::default())
                    .is_err()
            );
        }
        let mut data = parts();
        data.meshes[0].occlusion_uvs = Some(vec![[f32::NAN, 0.]; 4]);
        assert!(
            AssetDocument::new(data, ImportLimits::default(), &Cancellation::default()).is_err()
        );
        for strength in [0., 0.5, 1.] {
            let mut data = parts();
            data.meshes[0].occlusion_strength = strength;
            let doc = AssetDocument::new(data, ImportLimits::default(), &Cancellation::default())
                .unwrap();
            assert_eq!(
                doc.snapshot(ImportLimits::default(), &Cancellation::default())
                    .unwrap()
                    .draws[0]
                    .occlusion_strength,
                strength
            );
        }
    }
    #[test]
    fn emission_factor_and_uv_admission_reject_invalid_values_without_source_edit() {
        for bad in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            let mut data = parts();
            data.meshes[0].emissive_factor = [bad, 0., 0.];
            assert!(
                AssetDocument::new(data, ImportLimits::default(), &Cancellation::default())
                    .is_err()
            );
        }
        let mut data = parts();
        data.meshes[0].emissive_uvs = Some(vec![[0.; 2]; 3]);
        assert!(
            AssetDocument::new(data, ImportLimits::default(), &Cancellation::default()).is_err()
        );
        let mut data = parts();
        data.meshes[0].emissive_factor = [0.2, 0.5, 1.];
        let doc =
            AssetDocument::new(data, ImportLimits::default(), &Cancellation::default()).unwrap();
        assert_eq!(
            doc.snapshot(ImportLimits::default(), &Cancellation::default())
                .unwrap()
                .draws[0]
                .emissive_factor,
            [0.2, 0.5, 1.]
        );
        assert_eq!(&**doc.source(), &[1, 2, 3, 4]);
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
