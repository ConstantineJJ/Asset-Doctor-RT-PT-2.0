use crate::textures::TextureBindings;
use crate::{context::GpuResult, gpu::Gpu, resources::Buffer};
use ad_document::{DocumentId, GeometrySnapshot, ImportLimits, geometry_bytes};
use ash::vk;
use std::sync::Arc;

pub(crate) struct GpuDraw {
    pub index_start: u32,
    pub index_count: u32,
    pub base_color: [f32; 4],
    pub metallic_roughness: [f32; 2],
    pub alpha_cutoff: Option<f32>,
    /// glTF BLEND: drawn after opaque geometry and the floor, back to front.
    pub blend: bool,
    /// Centre of the draw's vertices (world), for back-to-front order.
    pub centre: [f32; 3],
    pub texture_slot: usize,
    pub occlusion_strength: f32,
    pub has_occlusion_map: bool,
    pub normal_scale: f32,
    pub has_normal_map: bool,
    pub emissive_factor: [f32; 3],
    pub has_emission: bool,
}

pub(crate) struct GpuScene {
    pub vertices: Buffer,
    pub normals: Buffer,
    pub tangents: Buffer,
    pub normal_uvs: Buffer,
    pub occlusion_uvs: Buffer,
    pub emissive_uvs: Buffer,
    pub indices: Buffer,
    pub uvs: Buffer,
    pub metallic_roughness_uvs: Buffer,
    pub colors: Buffer,
    pub textures: TextureBindings,
    pub draws: Vec<GpuDraw>,
    pub document_id: DocumentId,
    pub revision: u64,
    pub has_mr_maps: bool,
    pub has_normal_maps: bool,
    pub has_occlusion_maps: bool,
    pub has_emission: bool,
}
impl GpuScene {
    /// Accounted bytes of the geometry buffers (textures are logged separately).
    pub fn geometry_bytes(&self) -> usize {
        [
            &self.vertices,
            &self.normals,
            &self.tangents,
            &self.normal_uvs,
            &self.occlusion_uvs,
            &self.emissive_uvs,
            &self.indices,
            &self.uvs,
            &self.metallic_roughness_uvs,
            &self.colors,
        ]
        .iter()
        .map(|buffer| buffer.size())
        .sum()
    }
    pub fn new(gpu: Arc<Gpu>, snapshot: &GeometrySnapshot) -> GpuResult<Self> {
        if snapshot.positions.is_empty()
            || snapshot.indices.is_empty()
            || snapshot.draws.is_empty()
            || snapshot.draws.len() > ImportLimits::default().draws
            || snapshot.uvs.len() != snapshot.positions.len()
            || snapshot.metallic_roughness_uvs.len() != snapshot.positions.len()
            || snapshot
                .metallic_roughness_uvs
                .iter()
                .flatten()
                .any(|v| !v.is_finite())
            || snapshot.colors.len() != snapshot.positions.len()
            || snapshot.normals.len() != snapshot.positions.len()
            || snapshot.tangents.len() != snapshot.positions.len()
            || snapshot.normal_uvs.len() != snapshot.positions.len()
            || snapshot.occlusion_uvs.len() != snapshot.positions.len()
            || snapshot.emissive_uvs.len() != snapshot.positions.len()
            || snapshot
                .emissive_uvs
                .iter()
                .flatten()
                .any(|v| !v.is_finite())
            || snapshot
                .occlusion_uvs
                .iter()
                .flatten()
                .any(|v| !v.is_finite())
            || snapshot.normal_uvs.iter().flatten().any(|v| !v.is_finite())
            || snapshot.tangents.iter().any(|t| {
                t.iter().any(|v| !v.is_finite()) || (t[3] != 0. && t[3] != -1. && t[3] != 1.)
            })
            || snapshot.normals.iter().flatten().any(|v| !v.is_finite())
            || snapshot.uvs.iter().flatten().any(|v| !v.is_finite())
            || snapshot
                .colors
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            || geometry_bytes(snapshot.positions.len(), snapshot.indices.len())?
                > ImportLimits::default().snapshot_bytes
            || snapshot
                .indices
                .iter()
                .any(|&index| index as usize >= snapshot.positions.len())
            || snapshot
                .positions
                .iter()
                .flatten()
                .any(|value| !value.is_finite())
            || snapshot.draws.iter().any(|draw| {
                draw.index_count == 0
                    || draw
                        .emissive_factor
                        .iter()
                        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                    || !draw.normal_scale.is_finite()
                    || !draw.occlusion_strength.is_finite()
                    || !(0.0..=1.0).contains(&draw.occlusion_strength)
                    || draw
                        .metallic_roughness
                        .iter()
                        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                    || draw.alpha_cutoff.is_some_and(|v| !v.is_finite() || v < 0.)
                    || !draw.index_count.is_multiple_of(3)
                    || draw
                        .index_start
                        .checked_add(draw.index_count)
                        .is_none_or(|end| end as usize > snapshot.indices.len())
                    || draw
                        .base_color
                        .iter()
                        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
            })
        {
            return Err(
                "Invalid or over-budget geometry snapshot; current GPU scene preserved".into(),
            );
        }
        // SAFETY: Arrays contain only initialized f32/u32 values, with no padding.
        // Slices borrow the immutable snapshot through the synchronous copy.
        let (
            vertices,
            indices,
            uvs,
            colors,
            normals,
            metallic_roughness_uvs,
            tangents,
            normal_uvs,
            occlusion_uvs,
            emissive_uvs,
        ) = unsafe {
            (
                std::slice::from_raw_parts(
                    snapshot.positions.as_ptr().cast::<u8>(),
                    snapshot.positions.len() * 12,
                ),
                std::slice::from_raw_parts(
                    snapshot.indices.as_ptr().cast::<u8>(),
                    snapshot.indices.len() * 4,
                ),
                std::slice::from_raw_parts(
                    snapshot.uvs.as_ptr().cast::<u8>(),
                    snapshot.uvs.len() * 8,
                ),
                std::slice::from_raw_parts(
                    snapshot.colors.as_ptr().cast::<u8>(),
                    snapshot.colors.len() * 16,
                ),
                std::slice::from_raw_parts(
                    snapshot.normals.as_ptr().cast::<u8>(),
                    snapshot.normals.len() * 12,
                ),
                std::slice::from_raw_parts(
                    snapshot.metallic_roughness_uvs.as_ptr().cast::<u8>(),
                    snapshot.metallic_roughness_uvs.len() * 8,
                ),
                std::slice::from_raw_parts(
                    snapshot.tangents.as_ptr().cast::<u8>(),
                    snapshot.tangents.len() * 16,
                ),
                std::slice::from_raw_parts(
                    snapshot.normal_uvs.as_ptr().cast::<u8>(),
                    snapshot.normal_uvs.len() * 8,
                ),
                std::slice::from_raw_parts(
                    snapshot.occlusion_uvs.as_ptr().cast::<u8>(),
                    snapshot.occlusion_uvs.len() * 8,
                ),
                std::slice::from_raw_parts(
                    snapshot.emissive_uvs.as_ptr().cast::<u8>(),
                    snapshot.emissive_uvs.len() * 8,
                ),
            )
        };
        let vertices = Buffer::upload(gpu.clone(), vertices, vk::BufferUsageFlags::VERTEX_BUFFER)?;
        let indices = Buffer::upload(gpu.clone(), indices, vk::BufferUsageFlags::INDEX_BUFFER)?;
        let uvs = Buffer::upload(gpu.clone(), uvs, vk::BufferUsageFlags::VERTEX_BUFFER)?;
        let colors = Buffer::upload(gpu.clone(), colors, vk::BufferUsageFlags::VERTEX_BUFFER)?;
        let normals = Buffer::upload(gpu.clone(), normals, vk::BufferUsageFlags::VERTEX_BUFFER)?;
        let tangents = Buffer::upload(gpu.clone(), tangents, vk::BufferUsageFlags::VERTEX_BUFFER)?;
        let normal_uvs =
            Buffer::upload(gpu.clone(), normal_uvs, vk::BufferUsageFlags::VERTEX_BUFFER)?;
        let occlusion_uvs = Buffer::upload(
            gpu.clone(),
            occlusion_uvs,
            vk::BufferUsageFlags::VERTEX_BUFFER,
        )?;
        let metallic_roughness_uvs = Buffer::upload(
            gpu.clone(),
            metallic_roughness_uvs,
            vk::BufferUsageFlags::VERTEX_BUFFER,
        )?;
        let emissive_uvs = Buffer::upload(
            gpu.clone(),
            emissive_uvs,
            vk::BufferUsageFlags::VERTEX_BUFFER,
        )?;
        let textures = TextureBindings::new(gpu, &snapshot.draws)?;
        let draws = snapshot
            .draws
            .iter()
            .enumerate()
            .map(|(i, d)| GpuDraw {
                index_start: d.index_start,
                index_count: d.index_count,
                base_color: d.base_color,
                metallic_roughness: d.metallic_roughness,
                alpha_cutoff: d.alpha_cutoff,
                blend: d.blend,
                centre: draw_centre(snapshot, d),
                texture_slot: textures.draw_slots[i],
                normal_scale: d.normal_scale,
                has_normal_map: d.normal_texture.is_some(),
                has_occlusion_map: d.occlusion_texture.is_some(),
                occlusion_strength: d.occlusion_strength,
                emissive_factor: d.emissive_factor,
                has_emission: d.emissive_factor.iter().any(|&v| v > 0.),
            })
            .collect();
        Ok(Self {
            vertices,
            normals,
            tangents,
            normal_uvs,
            occlusion_uvs,
            indices,
            emissive_uvs,
            uvs,
            metallic_roughness_uvs,
            colors,
            textures,
            draws,
            document_id: snapshot.document_id,
            revision: snapshot.revision,
            has_mr_maps: snapshot
                .draws
                .iter()
                .any(|d| d.metallic_roughness_texture.is_some()),
            has_normal_maps: snapshot.draws.iter().any(|d| d.normal_texture.is_some()),
            has_occlusion_maps: snapshot.draws.iter().any(|d| d.occlusion_texture.is_some()),
            has_emission: snapshot
                .draws
                .iter()
                .any(|d| d.emissive_factor.iter().any(|&v| v > 0.)),
        })
    }
}

/// Mean of a draw's referenced vertex positions (world).
fn draw_centre(
    snapshot: &ad_document::GeometrySnapshot,
    draw: &ad_document::DrawRange,
) -> [f32; 3] {
    let start = draw.index_start as usize;
    let end = start + draw.index_count as usize;
    let (mut sum, mut n) = ([0f64; 3], 0usize);
    for &i in snapshot.indices.get(start..end).unwrap_or(&[]) {
        if let Some(p) = snapshot.positions.get(i as usize) {
            for k in 0..3 {
                sum[k] += f64::from(p[k]);
            }
            n += 1;
        }
    }
    if n == 0 {
        return [0.; 3];
    }
    sum.map(|v| (v / n as f64) as f32)
}
