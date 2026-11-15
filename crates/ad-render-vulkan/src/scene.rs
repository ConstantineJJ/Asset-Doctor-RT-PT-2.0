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
    pub texture_slot: usize,
}

pub(crate) struct GpuScene {
    pub vertices: Buffer,
    pub normals: Buffer,
    pub indices: Buffer,
    pub uvs: Buffer,
    pub colors: Buffer,
    pub textures: TextureBindings,
    pub draws: Vec<GpuDraw>,
    pub document_id: DocumentId,
    pub revision: u64,
}
impl GpuScene {
    pub fn new(gpu: Arc<Gpu>, snapshot: &GeometrySnapshot) -> GpuResult<Self> {
        if snapshot.positions.is_empty()
            || snapshot.indices.is_empty()
            || snapshot.draws.is_empty()
            || snapshot.draws.len() > ImportLimits::default().draws
            || snapshot.uvs.len() != snapshot.positions.len()
            || snapshot.colors.len() != snapshot.positions.len()
            || snapshot.normals.len() != snapshot.positions.len()
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
        let (vertices, indices, uvs, colors, normals) = unsafe {
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
            )
        };
        let vertices = Buffer::upload(gpu.clone(), vertices, vk::BufferUsageFlags::VERTEX_BUFFER)?;
        let indices = Buffer::upload(gpu.clone(), indices, vk::BufferUsageFlags::INDEX_BUFFER)?;
        let uvs = Buffer::upload(gpu.clone(), uvs, vk::BufferUsageFlags::VERTEX_BUFFER)?;
        let colors = Buffer::upload(gpu.clone(), colors, vk::BufferUsageFlags::VERTEX_BUFFER)?;
        let normals = Buffer::upload(gpu.clone(), normals, vk::BufferUsageFlags::VERTEX_BUFFER)?;
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
                texture_slot: textures.draw_slots[i],
            })
            .collect();
        Ok(Self {
            vertices,
            normals,
            indices,
            uvs,
            colors,
            textures,
            draws,
            document_id: snapshot.document_id,
            revision: snapshot.revision,
        })
    }
}
