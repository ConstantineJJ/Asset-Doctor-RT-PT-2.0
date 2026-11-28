//! Poses of a read-only (rigged or animated) scene.
//!
//! - CPU path: a posed snapshot overwrites the scene's position, normal and
//!   tangent buffers after an idle wait.
//! - GPU path (ADR-0066): the scene keeps a skinning layout (source vertices
//!   with matrix slots and weights); a pose is only its matrices. A compute
//!   pass at the start of the next frame writes the posed vertices into the
//!   same buffers, so every pipeline (mesh, shadow, outline) draws them.
use super::Renderer;
use crate::{context::GpuResult, gpu::Gpu, pipeline::Shader, resources::Buffer, scene::GpuScene};
use ad_document::{DocumentId, GeometrySnapshot, SkinLayout};
use ash::vk;
use std::sync::Arc;

pub(crate) struct GpuSkin {
    source: Buffer,
    matrices: Buffer,
    pending: Option<Vec<u8>>,
    count: u32,
    set_layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    gpu: Arc<Gpu>,
}

fn as_bytes<T>(values: &[T]) -> &[u8] {
    // SAFETY: Only used with plain f32/u32 arrays and repr(C) vertex records
    // without padding (SkinVertex is 20 four-byte fields).
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), std::mem::size_of_val(values)) }
}

impl GpuSkin {
    fn new(gpu: Arc<Gpu>, layout: &SkinLayout, scene: &GpuScene) -> GpuResult<Self> {
        let count = layout.vertices.len();
        if count == 0
            || layout.matrices == 0
            || scene.vertices.size() != count * 12
            || scene.normals.size() != count * 12
            || scene.tangents.size() != count * 16
            || count > u32::MAX as usize
        {
            return Err("Skinning layout does not match the scene".into());
        }
        let source_bytes = as_bytes(&layout.vertices);
        // SAFETY: Read-only property query on a live physical device.
        let limits = unsafe {
            gpu.surface
                .context
                .instance
                .get_physical_device_properties(gpu.physical)
                .limits
        };
        let range = u64::from(limits.max_storage_buffer_range);
        if source_bytes.len() as u64 > range
            || (layout.matrices * 64) as u64 > range
            || (count * 16) as u64 > range
        {
            return Err("Scene exceeds the GPU storage buffer range for skinning".into());
        }
        let source = Buffer::upload(
            gpu.clone(),
            source_bytes,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?;
        let identity: Vec<[[f32; 4]; 4]> = vec![glam_identity(); layout.matrices];
        let matrices = Buffer::upload(
            gpu.clone(),
            as_bytes(&identity),
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?;
        let mut skin = Self {
            source,
            matrices,
            pending: None,
            count: count as u32,
            set_layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            gpu,
        };
        let shader = Shader::new(
            skin.gpu.clone(),
            include_bytes!(concat!(env!("OUT_DIR"), "/skin.comp.spv")),
        )?;
        // SAFETY: Self owns every handle as soon as it exists, so a later
        // failure releases it in Drop; create infos outlive each call.
        unsafe {
            let device = &skin.gpu.device;
            let bindings: Vec<_> = (0..5)
                .map(|binding| {
                    vk::DescriptorSetLayoutBinding::default()
                        .binding(binding)
                        .descriptor_count(1)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .stage_flags(vk::ShaderStageFlags::COMPUTE)
                })
                .collect();
            skin.set_layout = device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )?;
            let sizes = [vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 5,
            }];
            skin.pool = device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(1)
                    .pool_sizes(&sizes),
                None,
            )?;
            let layouts = [skin.set_layout];
            skin.set = device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(skin.pool)
                    .set_layouts(&layouts),
            )?[0];
            let infos = [
                skin.source.handle,
                skin.matrices.handle,
                scene.vertices.handle,
                scene.normals.handle,
                scene.tangents.handle,
            ]
            .map(|buffer| {
                [vk::DescriptorBufferInfo {
                    buffer,
                    offset: 0,
                    range: vk::WHOLE_SIZE,
                }]
            });
            let writes: Vec<_> = infos
                .iter()
                .enumerate()
                .map(|(binding, info)| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(skin.set)
                        .dst_binding(binding as u32)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(info)
                })
                .collect();
            device.update_descriptor_sets(&writes, &[]);
            let ranges = [vk::PushConstantRange::default()
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .offset(0)
                .size(4)];
            skin.layout = device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&layouts)
                    .push_constant_ranges(&ranges),
                None,
            )?;
            let stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::COMPUTE)
                .module(shader.handle)
                .name(c"main");
            skin.pipeline = device
                .create_compute_pipelines(
                    vk::PipelineCache::null(),
                    &[vk::ComputePipelineCreateInfo::default()
                        .stage(stage)
                        .layout(skin.layout)],
                    None,
                )
                .map_err(|(_, error)| error)?[0];
        }
        Ok(skin)
    }

    /// Records the skinning pass when a new pose waits: the matrices are
    /// written first (the previous frame has retired), then the dispatch,
    /// fenced by barriers against the previous frame's vertex reads and the
    /// coming draws.
    ///
    /// # Safety
    /// `command` is recording and no submitted work uses the matrices.
    pub(super) unsafe fn record(&mut self, command: vk::CommandBuffer) -> GpuResult<bool> {
        let Some(bytes) = self.pending.take() else {
            return Ok(false);
        };
        // SAFETY: caller guarantees the single in-flight frame has retired.
        unsafe {
            self.matrices.overwrite(&bytes)?;
            let device = &self.gpu.device;
            let before = [vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::VERTEX_ATTRIBUTE_READ)
                .dst_access_mask(vk::AccessFlags::SHADER_WRITE)];
            device.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::VERTEX_INPUT,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &before,
                &[],
                &[],
            );
            device.cmd_bind_pipeline(command, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            device.cmd_bind_descriptor_sets(
                command,
                vk::PipelineBindPoint::COMPUTE,
                self.layout,
                0,
                &[self.set],
                &[],
            );
            device.cmd_push_constants(
                command,
                self.layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                &self.count.to_le_bytes(),
            );
            device.cmd_dispatch(command, self.count.div_ceil(64), 1, 1);
            let after = [vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::VERTEX_ATTRIBUTE_READ)];
            device.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::VERTEX_INPUT,
                vk::DependencyFlags::empty(),
                &after,
                &[],
                &[],
            );
        }
        Ok(true)
    }
}

fn glam_identity() -> [[f32; 4]; 4] {
    [
        [1., 0., 0., 0.],
        [0., 1., 0., 0.],
        [0., 0., 1., 0.],
        [0., 0., 0., 1.],
    ]
}

impl Drop for GpuSkin {
    fn drop(&mut self) {
        // SAFETY: The scene owning this skin is dropped only after the
        // renderer retired its submissions; null handles are ignored.
        unsafe {
            let device = &self.gpu.device;
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.set_layout, None);
        }
    }
}

impl Renderer {
    /// Publish a CPU pose for the current read-only scene: positions,
    /// normals and tangents are overwritten in place after an idle wait.
    /// Indices, draws and materials are shared with the base snapshot.
    pub fn set_scene_pose(&mut self, snapshot: &GeometrySnapshot) -> GpuResult<()> {
        let scene = self.scene.as_ref().ok_or("No scene for animation")?;
        let positions = as_bytes(&snapshot.positions);
        let normals = as_bytes(&snapshot.normals);
        let tangents = as_bytes(&snapshot.tangents);
        if snapshot.document_id != scene.document_id
            || snapshot.revision != scene.revision
            || positions.len() != scene.vertices.size()
            || normals.len() != scene.normals.size()
            || tangents.len() != scene.tangents.size()
        {
            return Err("Animation pose does not match the current scene".into());
        }
        self.wait_idle()?;
        let scene = self.scene.as_mut().expect("Scene checked before upload");
        // SAFETY: wait_idle retired every submission that reads these buffers.
        unsafe {
            scene.vertices.overwrite(positions)?;
            scene.normals.overwrite(normals)?;
            scene.tangents.overwrite(tangents)?;
        }
        if let Some(skin) = scene.skin.as_mut() {
            skin.pending = None;
        }
        Ok(())
    }

    /// ADR-0066: whether this GPU can skin (a compute-capable queue).
    pub fn gpu_skinning(&self) -> bool {
        self.gpu.compute
    }

    /// Attaches a skinning layout to the current scene (once per scene).
    pub fn set_scene_skin(&mut self, layout: &SkinLayout) -> GpuResult<()> {
        if !self.gpu.compute {
            return Err("This GPU queue has no compute support".into());
        }
        let scene = self.scene.as_ref().ok_or("No scene for skinning")?;
        if scene.document_id != layout.document_id || scene.revision != layout.revision {
            return Err("Skinning layout belongs to another scene".into());
        }
        if scene.skin.is_some() {
            return Ok(());
        }
        let skin = GpuSkin::new(self.gpu.clone(), layout, scene)?;
        eprintln!(
            "gpu_skinning_ready vertices={} matrices={}",
            layout.vertices.len(),
            layout.matrices
        );
        self.scene.as_mut().expect("scene checked").skin = Some(skin);
        Ok(())
    }

    /// A GPU pose: its matrices, skinned at the start of the next frame.
    pub fn set_skin_pose(
        &mut self,
        document: DocumentId,
        revision: u64,
        matrices: &[[[f32; 4]; 4]],
    ) -> GpuResult<()> {
        let scene = self.scene.as_mut().ok_or("No scene for skinning")?;
        if scene.document_id != document || scene.revision != revision {
            return Err("Skinning pose belongs to another scene".into());
        }
        let skin = scene
            .skin
            .as_mut()
            .ok_or("The scene has no skinning layout")?;
        let bytes = as_bytes(matrices);
        if bytes.len() != skin.matrices.size() {
            return Err("Skinning pose has the wrong number of matrices".into());
        }
        skin.pending = Some(bytes.to_vec());
        Ok(())
    }

    /// A GPU pose is set but not yet skinned (the next frame does it).
    pub fn skin_pending(&self) -> bool {
        self.scene
            .as_ref()
            .and_then(|scene| scene.skin.as_ref())
            .is_some_and(|skin| skin.pending.is_some())
    }

    /// Smoke check: the scene's current positions, read back after an idle
    /// wait (what the last frame drew).
    pub fn scene_positions(&mut self) -> GpuResult<Vec<[f32; 3]>> {
        self.wait_idle()?;
        let scene = self.scene.as_ref().ok_or("No scene")?;
        // SAFETY: wait_idle retired every access to the buffer.
        let bytes = unsafe { scene.vertices.read()? };
        Ok(bytes
            .as_chunks::<12>()
            .0
            .iter()
            .map(|c| {
                let f = |i: usize| f32::from_le_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]);
                [f(0), f(4), f(8)]
            })
            .collect())
    }
}
