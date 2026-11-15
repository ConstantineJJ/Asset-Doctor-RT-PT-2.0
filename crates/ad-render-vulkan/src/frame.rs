//! Single in-flight frame data; never asset/source ownership.
use crate::{context::GpuResult, gpu::Gpu, resources::Buffer};
use ash::vk;
use std::sync::Arc;

#[repr(C)]
#[derive(Default)]
pub(crate) struct FrameData {
    pub view_projection: [[f32; 4]; 4],
    pub eye_exposure: [f32; 4],
    pub lighting: [f32; 4],
}
impl FrameData {
    fn bytes(&self) -> &[u8] {
        // SAFETY: repr(C) arrays of initialized floats; tested offsets/no padding.
        unsafe {
            std::slice::from_raw_parts(
                (self as *const Self).cast::<u8>(),
                std::mem::size_of::<Self>(),
            )
        }
    }
}
pub(crate) struct FrameLayout {
    pub handle: vk::DescriptorSetLayout,
    gpu: Arc<Gpu>,
}
impl FrameLayout {
    pub fn new(gpu: Arc<Gpu>) -> GpuResult<Self> {
        let bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_count(1)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];
        // SAFETY: Fixed valid layout, borrowed create info lives through call.
        let handle = unsafe {
            gpu.device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )?
        };
        Ok(Self { handle, gpu })
    }
}
impl Drop for FrameLayout {
    fn drop(&mut self) {
        // SAFETY: Pool/pipeline owners retire dependent use before layout release.
        unsafe {
            self.gpu
                .device
                .destroy_descriptor_set_layout(self.handle, None);
        }
    }
}
pub(crate) struct FrameBinding {
    pub set: vk::DescriptorSet,
    pool: vk::DescriptorPool,
    layout: FrameLayout,
    buffer: Buffer,
    gpu: Arc<Gpu>,
}
impl FrameBinding {
    pub fn new(gpu: Arc<Gpu>) -> GpuResult<Self> {
        let buffer = Buffer::upload(
            gpu.clone(),
            FrameData::default().bytes(),
            vk::BufferUsageFlags::UNIFORM_BUFFER,
        )?;
        let mut result = Self {
            set: vk::DescriptorSet::null(),
            pool: vk::DescriptorPool::null(),
            layout: FrameLayout::new(gpu.clone())?,
            buffer,
            gpu,
        };
        // SAFETY: All successful allocations owned before next fallible call.
        unsafe {
            let sizes = [vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: 1,
            }];
            result.pool = result.gpu.device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(1)
                    .pool_sizes(&sizes),
                None,
            )?;
            let layouts = [result.layout.handle];
            result.set = result.gpu.device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(result.pool)
                    .set_layouts(&layouts),
            )?[0];
            let info = [vk::DescriptorBufferInfo {
                buffer: result.buffer.handle,
                offset: 0,
                range: std::mem::size_of::<FrameData>() as u64,
            }];
            result.gpu.device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(result.set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&info)],
                &[],
            );
        }
        Ok(result)
    }
    /// # Safety
    /// The renderer must have retired the single in-flight submission first.
    pub unsafe fn update(&self, data: &FrameData) -> GpuResult<()> {
        // SAFETY: Same no-device-access requirement passed to the buffer owner.
        unsafe { self.buffer.overwrite(data.bytes()) }
    }
}
impl Drop for FrameBinding {
    fn drop(&mut self) {
        // SAFETY: Renderer retired work; pool invalidates set before fields drop.
        unsafe {
            self.gpu.device.destroy_descriptor_pool(self.pool, None);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn std140_frame_offsets_and_data_bytes_match_glsl() {
        assert_eq!(std::mem::size_of::<FrameData>(), 96);
        assert_eq!(std::mem::offset_of!(FrameData, eye_exposure), 64);
        assert_eq!(std::mem::offset_of!(FrameData, lighting), 80);
        let mut frame = FrameData::default();
        frame.eye_exposure[3] = 2.;
        assert_eq!(&frame.bytes()[76..80], &2f32.to_ne_bytes());
    }
}
