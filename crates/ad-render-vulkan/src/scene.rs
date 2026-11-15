use crate::{context::GpuResult, gpu::Gpu, resources::Buffer};
use ad_document::{DocumentId, DrawRange, GeometrySnapshot, ImportLimits, geometry_bytes};
use ash::vk;
use std::sync::Arc;

pub(crate) struct GpuScene {
    pub vertices: Buffer,
    pub indices: Buffer,
    pub draws: Vec<DrawRange>,
    pub document_id: DocumentId,
    pub revision: u64,
}
impl GpuScene {
    pub fn new(gpu: Arc<Gpu>, snapshot: &GeometrySnapshot) -> GpuResult<Self> {
        if snapshot.positions.is_empty()
            || snapshot.indices.is_empty()
            || snapshot.draws.is_empty()
            || snapshot.draws.len() > ImportLimits::default().draws
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
        let (vertices, indices) = unsafe {
            (
                std::slice::from_raw_parts(
                    snapshot.positions.as_ptr().cast::<u8>(),
                    snapshot.positions.len() * 12,
                ),
                std::slice::from_raw_parts(
                    snapshot.indices.as_ptr().cast::<u8>(),
                    snapshot.indices.len() * 4,
                ),
            )
        };
        let vertices = Buffer::upload(gpu.clone(), vertices, vk::BufferUsageFlags::VERTEX_BUFFER)?;
        let indices = Buffer::upload(gpu, indices, vk::BufferUsageFlags::INDEX_BUFFER)?;
        let draws = snapshot
            .draws
            .iter()
            .map(|d| DrawRange {
                index_start: d.index_start,
                index_count: d.index_count,
                base_color: d.base_color,
                color_texture: d.color_texture.clone(),
                alpha_cutoff: d.alpha_cutoff,
            })
            .collect();
        Ok(Self {
            vertices,
            indices,
            draws,
            document_id: snapshot.document_id,
            revision: snapshot.revision,
        })
    }
}
