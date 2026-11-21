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
    /// ADR-0040 environment light: linear zenith (w = strength, 0 = off),
    /// horizon and ground colours.
    pub sky_zenith: [f32; 4],
    pub sky_horizon: [f32; 4],
    pub sky_ground: [f32; 4],
    /// ADR-0040 lamp: unit direction towards it (w unused) and linear colour.
    pub light_direction: [f32; 4],
    pub light_color: [f32; 4],
    /// ADR-0044: world to lamp clip space, and shadow parameters
    /// (strength 0 = off, depth bias, texel size, normal offset).
    pub light_view_projection: [[f32; 4]; 4],
    pub shadow: [f32; 4],
    /// ADR-0047 image based lighting: (strength, 0 = off; last prefiltered
    /// level; panorama yaw in radians; unused) and the irradiance
    /// coefficients (rgb, w unused).
    pub ibl: [f32; 4],
    pub sh: [[f32; 4]; 9],
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
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            // ADR-0044 shadow map and its sampler.
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            // ADR-0047 environment panorama and its sampler.
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];
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
    pub fn new(
        gpu: Arc<Gpu>,
        shadow: &crate::shadow::ShadowMap,
        environment: &crate::sky::Skybox,
    ) -> GpuResult<Self> {
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
            let sizes = [
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::UNIFORM_BUFFER,
                    descriptor_count: 1,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::SAMPLED_IMAGE,
                    descriptor_count: 2,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::SAMPLER,
                    descriptor_count: 2,
                },
            ];
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
            let image = [vk::DescriptorImageInfo {
                sampler: vk::Sampler::null(),
                image_view: shadow.view,
                image_layout: vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
            }];
            let sampler = [vk::DescriptorImageInfo {
                sampler: shadow.sampler,
                ..Default::default()
            }];
            result.gpu.device.update_descriptor_sets(
                &[
                    vk::WriteDescriptorSet::default()
                        .dst_set(result.set)
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                        .buffer_info(&info),
                    vk::WriteDescriptorSet::default()
                        .dst_set(result.set)
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                        .image_info(&image),
                    vk::WriteDescriptorSet::default()
                        .dst_set(result.set)
                        .dst_binding(2)
                        .descriptor_type(vk::DescriptorType::SAMPLER)
                        .image_info(&sampler),
                ],
                &[],
            );
        }
        // SAFETY: the set was just written and is not in use yet.
        unsafe { result.bind_environment(environment) };
        Ok(result)
    }
    /// ADR-0047: points bindings 3 and 4 at an environment panorama.
    ///
    /// # Safety
    /// No submitted work may use the set (the renderer waited for idle).
    pub unsafe fn bind_environment(&self, environment: &crate::sky::Skybox) {
        let image = [vk::DescriptorImageInfo {
            sampler: vk::Sampler::null(),
            image_view: environment.image.view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        let sampler = [vk::DescriptorImageInfo {
            sampler: environment.sampler.handle,
            ..Default::default()
        }];
        // SAFETY: caller guarantees the set is idle; both handles are live.
        unsafe {
            self.gpu.device.update_descriptor_sets(
                &[
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.set)
                        .dst_binding(3)
                        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                        .image_info(&image),
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.set)
                        .dst_binding(4)
                        .descriptor_type(vk::DescriptorType::SAMPLER)
                        .image_info(&sampler),
                ],
                &[],
            );
        }
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
        assert_eq!(std::mem::size_of::<FrameData>(), 416);
        assert_eq!(std::mem::offset_of!(FrameData, ibl), 256);
        assert_eq!(std::mem::offset_of!(FrameData, sh), 272);
        assert_eq!(std::mem::offset_of!(FrameData, light_view_projection), 176);
        assert_eq!(std::mem::offset_of!(FrameData, shadow), 240);
        assert_eq!(std::mem::offset_of!(FrameData, light_direction), 144);
        assert_eq!(std::mem::offset_of!(FrameData, light_color), 160);
        assert_eq!(std::mem::offset_of!(FrameData, sky_zenith), 96);
        assert_eq!(std::mem::offset_of!(FrameData, sky_ground), 128);
        assert_eq!(std::mem::offset_of!(FrameData, eye_exposure), 64);
        assert_eq!(std::mem::offset_of!(FrameData, lighting), 80);
        let mut frame = FrameData::default();
        frame.eye_exposure[3] = 2.;
        assert_eq!(&frame.bytes()[76..80], &2f32.to_ne_bytes());
    }
}
