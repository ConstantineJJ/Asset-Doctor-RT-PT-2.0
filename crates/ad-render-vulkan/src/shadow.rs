//! ADR-0044 lamp shadows: one depth map rendered from the lamp with an
//! orthographic matrix the desktop fits to the model and its floor shadow.
//! The model and the floor sample it (3x3 PCF). View only.
use crate::{context::GpuResult, gpu::Gpu, pipeline::Shader, resources::memory_type};
use ash::vk;
use std::sync::Arc;

/// Shadow map side in texels.
pub const SIZE: u32 = 2048;

/// What the shadow pass and the shaders need for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSettings {
    /// World to light clip space (depth 0..1, standard Z).
    pub light_view_projection: [[f32; 4]; 4],
    /// 0 = no darkening, 1 = full shadow.
    pub strength: f32,
    /// Depth bias in light clip units.
    pub bias: f32,
    /// World-space offset along the surface normal before the lookup.
    pub normal_offset: f32,
}
impl ShadowSettings {
    pub fn valid(&self) -> bool {
        self.light_view_projection
            .iter()
            .flatten()
            .chain([&self.strength, &self.bias, &self.normal_offset])
            .all(|v| v.is_finite())
            && (0.0..=1.0).contains(&self.strength)
    }
}

pub(crate) struct ShadowMap {
    pub view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub pass: vk::RenderPass,
    pub framebuffer: vk::Framebuffer,
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    image: vk::Image,
    memory: vk::DeviceMemory,
    gpu: Arc<Gpu>,
}
impl ShadowMap {
    pub fn new(gpu: Arc<Gpu>) -> GpuResult<Self> {
        let mut map = Self {
            view: vk::ImageView::null(),
            sampler: vk::Sampler::null(),
            pass: vk::RenderPass::null(),
            framebuffer: vk::Framebuffer::null(),
            pipeline: vk::Pipeline::null(),
            layout: vk::PipelineLayout::null(),
            image: vk::Image::null(),
            memory: vk::DeviceMemory::null(),
            gpu,
        };
        let vert = Shader::new(
            map.gpu.clone(),
            include_bytes!(concat!(env!("OUT_DIR"), "/shadow.vert.spv")),
        )?;
        // SAFETY: every handle is owned by `map` before the next fallible call
        // and released by Drop; create infos live through each call.
        unsafe {
            let device = &map.gpu.device;
            let format = vk::Format::D32_SFLOAT;
            let props = map
                .gpu
                .surface
                .context
                .instance
                .get_physical_device_format_properties(map.gpu.physical, format);
            if !props.optimal_tiling_features.contains(
                vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE,
            ) {
                return Err("GPU cannot sample a D32 shadow map".into());
            }
            map.image = device.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(format)
                    .extent(vk::Extent3D {
                        width: SIZE,
                        height: SIZE,
                        depth: 1,
                    })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(
                        vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                            | vk::ImageUsageFlags::SAMPLED,
                    )
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )?;
            let req = device.get_image_memory_requirements(map.image);
            map.memory = device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(memory_type(
                        &map.gpu,
                        req.memory_type_bits,
                        vk::MemoryPropertyFlags::DEVICE_LOCAL,
                    )?),
                None,
            )?;
            device.bind_image_memory(map.image, map.memory, 0)?;
            map.view = device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(map.image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::DEPTH)
                            .level_count(1)
                            .layer_count(1),
                    ),
                None,
            )?;
            map.sampler = device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .min_filter(vk::Filter::NEAREST)
                    .mag_filter(vk::Filter::NEAREST)
                    .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                None,
            )?;
            let attachment = [vk::AttachmentDescription::default()
                .format(format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)];
            let depth_ref = vk::AttachmentReference {
                attachment: 0,
                layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            };
            let subpass = [vk::SubpassDescription::default()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .depth_stencil_attachment(&depth_ref)];
            // Previous frame's shader reads finish before this write; this
            // write finishes before the main pass samples it.
            let dependencies = [
                vk::SubpassDependency {
                    src_subpass: vk::SUBPASS_EXTERNAL,
                    dst_subpass: 0,
                    src_stage_mask: vk::PipelineStageFlags::FRAGMENT_SHADER,
                    dst_stage_mask: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                        | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                    src_access_mask: vk::AccessFlags::SHADER_READ,
                    dst_access_mask: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                        | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    dependency_flags: vk::DependencyFlags::empty(),
                },
                vk::SubpassDependency {
                    src_subpass: 0,
                    dst_subpass: vk::SUBPASS_EXTERNAL,
                    src_stage_mask: vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                    dst_stage_mask: vk::PipelineStageFlags::FRAGMENT_SHADER,
                    src_access_mask: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    dst_access_mask: vk::AccessFlags::SHADER_READ,
                    dependency_flags: vk::DependencyFlags::empty(),
                },
            ];
            map.pass = device.create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&attachment)
                    .subpasses(&subpass)
                    .dependencies(&dependencies),
                None,
            )?;
            let views = [map.view];
            map.framebuffer = device.create_framebuffer(
                &vk::FramebufferCreateInfo::default()
                    .render_pass(map.pass)
                    .attachments(&views)
                    .width(SIZE)
                    .height(SIZE)
                    .layers(1),
                None,
            )?;
            let ranges = [vk::PushConstantRange {
                stage_flags: vk::ShaderStageFlags::VERTEX,
                offset: 0,
                size: 64,
            }];
            map.layout = device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().push_constant_ranges(&ranges),
                None,
            )?;
            let stages = [vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vert.handle)
                .name(c"main")];
            let bindings = [vk::VertexInputBindingDescription {
                binding: 0,
                stride: 12,
                input_rate: vk::VertexInputRate::VERTEX,
            }];
            let attributes = [vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: 0,
            }];
            let vertex = vk::PipelineVertexInputStateCreateInfo::default()
                .vertex_binding_descriptions(&bindings)
                .vertex_attribute_descriptions(&attributes);
            let assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
            let viewports = [vk::Viewport {
                x: 0.,
                y: 0.,
                width: SIZE as f32,
                height: SIZE as f32,
                min_depth: 0.,
                max_depth: 1.,
            }];
            let scissors = [vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D {
                    width: SIZE,
                    height: SIZE,
                },
            }];
            let viewport = vk::PipelineViewportStateCreateInfo::default()
                .viewports(&viewports)
                .scissors(&scissors);
            // Two-sided casters; a constant and slope bias against acne.
            let raster = vk::PipelineRasterizationStateCreateInfo::default()
                .polygon_mode(vk::PolygonMode::FILL)
                .cull_mode(vk::CullModeFlags::NONE)
                .depth_bias_enable(true)
                .depth_bias_constant_factor(1.25)
                .depth_bias_slope_factor(1.75)
                .line_width(1.0);
            let samples = vk::PipelineMultisampleStateCreateInfo::default()
                .rasterization_samples(vk::SampleCountFlags::TYPE_1);
            let depth = vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
            let blend = vk::PipelineColorBlendStateCreateInfo::default();
            let create = [vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vertex)
                .input_assembly_state(&assembly)
                .viewport_state(&viewport)
                .rasterization_state(&raster)
                .multisample_state(&samples)
                .depth_stencil_state(&depth)
                .color_blend_state(&blend)
                .layout(map.layout)
                .render_pass(map.pass)
                .subpass(0)];
            match device.create_graphics_pipelines(vk::PipelineCache::null(), &create, None) {
                Ok(handles) => map.pipeline = handles[0],
                Err((handles, error)) => {
                    for handle in handles {
                        device.destroy_pipeline(handle, None);
                    }
                    return Err(error.into());
                }
            }
        }
        map.clear_once()?;
        Ok(map)
    }
    /// One empty pass at creation: the map is cleared to "far" and in its
    /// read-only layout before any shader can sample it.
    fn clear_once(&self) -> GpuResult<()> {
        // SAFETY: private pool, buffer and fence; the wait retires the copy
        // before they are destroyed.
        unsafe {
            let device = &self.gpu.device;
            let pool = device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(self.gpu.family)
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT),
                None,
            )?;
            let result = (|| -> GpuResult<()> {
                let command = device.allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )?[0];
                device.begin_command_buffer(
                    command,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )?;
                self.begin(command);
                device.cmd_end_render_pass(command);
                device.end_command_buffer(command)?;
                let fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
                let commands = [command];
                let submit = [vk::SubmitInfo::default().command_buffers(&commands)];
                let waited = device
                    .queue_submit(self.gpu.queue, &submit, fence)
                    .and_then(|()| device.wait_for_fences(&[fence], true, u64::MAX));
                device.destroy_fence(fence, None);
                waited?;
                Ok(())
            })();
            device.destroy_command_pool(pool, None);
            result
        }
    }
    /// Begins the shadow pass (cleared to depth 1).
    ///
    /// # Safety
    /// `command` is recording and outside any render pass.
    pub unsafe fn begin(&self, command: vk::CommandBuffer) {
        let clear = [vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 1.0,
                stencil: 0,
            },
        }];
        // SAFETY: caller contract; pass and framebuffer are owned here.
        unsafe {
            self.gpu.device.cmd_begin_render_pass(
                command,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(self.pass)
                    .framebuffer(self.framebuffer)
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: vk::Extent2D {
                            width: SIZE,
                            height: SIZE,
                        },
                    })
                    .clear_values(&clear),
                vk::SubpassContents::INLINE,
            );
        }
    }
}
impl Drop for ShadowMap {
    fn drop(&mut self) {
        // SAFETY: the renderer waits for idle before dropping its shadow map.
        unsafe {
            let device = &self.gpu.device;
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_framebuffer(self.framebuffer, None);
            device.destroy_render_pass(self.pass, None);
            device.destroy_sampler(self.sampler, None);
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_refuse_nonfinite_values_and_out_of_range_strength() {
        let identity = [
            [1., 0., 0., 0.],
            [0., 1., 0., 0.],
            [0., 0., 1., 0.],
            [0., 0., 0., 1.],
        ];
        let ok = ShadowSettings {
            light_view_projection: identity,
            strength: 0.7,
            bias: 0.001,
            normal_offset: 0.01,
        };
        assert!(ok.valid());
        assert!(
            !ShadowSettings {
                strength: 1.5,
                ..ok
            }
            .valid()
        );
        assert!(
            !ShadowSettings {
                bias: f32::NAN,
                ..ok
            }
            .valid()
        );
        let mut m = identity;
        m[3][0] = f32::INFINITY;
        assert!(
            !ShadowSettings {
                light_view_projection: m,
                ..ok
            }
            .valid()
        );
    }
}
