//! Object outline (2026-11-21 owner request): the selected and the hovered
//! object are drawn into a small mask before the main pass; in the main pass
//! a full-screen draw marks the pixels just outside the mask — a solid line
//! for the selection, a dashed one for the hover. Colours are either fixed
//! or "dynamic" (the line inverts what is below it).
use crate::{context::GpuResult, gpu::Gpu, pipeline::Shader, resources::memory_type};
use ash::vk;
use std::sync::Arc;

/// What to outline this frame (draw indices of the scene).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutlineSettings {
    pub selected: Vec<usize>,
    pub hovered: Vec<usize>,
    /// None: dynamic contrast (invert); Some: fixed sRGB colour.
    pub color: Option<[u8; 3]>,
}
impl OutlineSettings {
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty() && self.hovered.is_empty()
    }
}

#[repr(C)]
pub(crate) struct MaskPush {
    pub view_projection: [[f32; 4]; 4],
    pub value: [f32; 4],
}
#[repr(C)]
pub(crate) struct OutlinePush {
    pub selected: [f32; 4],
    pub hovered: [f32; 4],
    /// Radius (pixels), dash period (pixels), dynamic flag, unused.
    pub params: [f32; 4],
}
impl OutlinePush {
    /// `decode`: the swapchain encodes sRGB itself, so colours go linear.
    pub fn new(color: Option<[u8; 3]>, decode: bool) -> Self {
        let channel = |v: u8| {
            let c = f32::from(v) / 255.;
            if decode {
                crate::sky::srgb_to_linear(c)
            } else {
                c
            }
        };
        let rgba = color.map_or([1.; 4], |[r, g, b]| {
            [channel(r), channel(g), channel(b), 1.]
        });
        Self {
            selected: rgba,
            hovered: rgba,
            params: [2., 6., if color.is_none() { 1. } else { 0. }, 0.],
        }
    }
}

const FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;

/// Mask image, its pass and pipeline, and the two outline pipelines of the
/// main pass. Sized like the swapchain and rebuilt with it.
pub(crate) struct OutlineTarget {
    pub pass: vk::RenderPass,
    pub framebuffer: vk::Framebuffer,
    pub extent: vk::Extent2D,
    pub mask_layout: vk::PipelineLayout,
    pub mask_pipeline: vk::Pipeline,
    pub set: vk::DescriptorSet,
    pub layout: vk::PipelineLayout,
    /// Fixed colours (alpha blend) and dynamic contrast (invert).
    pub blend_pipeline: vk::Pipeline,
    pub invert_pipeline: vk::Pipeline,
    set_layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    sampler: vk::Sampler,
    view: vk::ImageView,
    image: vk::Image,
    memory: vk::DeviceMemory,
    gpu: Arc<Gpu>,
}

fn pipeline(
    gpu: &Gpu,
    stages: &[vk::PipelineShaderStageCreateInfo<'_>],
    vertex: &vk::PipelineVertexInputStateCreateInfo<'_>,
    blend: vk::PipelineColorBlendAttachmentState,
    layout: vk::PipelineLayout,
    pass: vk::RenderPass,
) -> GpuResult<vk::Pipeline> {
    let assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .line_width(1.0);
    let samples = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let depth = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false);
    let attachments = [blend];
    let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let create = [vk::GraphicsPipelineCreateInfo::default()
        .stages(stages)
        .vertex_input_state(vertex)
        .input_assembly_state(&assembly)
        .viewport_state(&viewport)
        .rasterization_state(&raster)
        .multisample_state(&samples)
        .depth_stencil_state(&depth)
        .color_blend_state(&blend)
        .dynamic_state(&dynamic)
        .layout(layout)
        .render_pass(pass)
        .subpass(0)];
    // SAFETY: create infos live through the call; failures free partial handles.
    unsafe {
        match gpu
            .device
            .create_graphics_pipelines(vk::PipelineCache::null(), &create, None)
        {
            Ok(handles) => Ok(handles[0]),
            Err((handles, error)) => {
                for handle in handles {
                    gpu.device.destroy_pipeline(handle, None);
                }
                Err(error.into())
            }
        }
    }
}

impl OutlineTarget {
    pub fn new(gpu: Arc<Gpu>, extent: vk::Extent2D, main_pass: vk::RenderPass) -> GpuResult<Self> {
        let mut target = Self {
            pass: vk::RenderPass::null(),
            framebuffer: vk::Framebuffer::null(),
            extent,
            mask_layout: vk::PipelineLayout::null(),
            mask_pipeline: vk::Pipeline::null(),
            set: vk::DescriptorSet::null(),
            layout: vk::PipelineLayout::null(),
            blend_pipeline: vk::Pipeline::null(),
            invert_pipeline: vk::Pipeline::null(),
            set_layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            sampler: vk::Sampler::null(),
            view: vk::ImageView::null(),
            image: vk::Image::null(),
            memory: vk::DeviceMemory::null(),
            gpu,
        };
        let mask_vert = Shader::new(
            target.gpu.clone(),
            include_bytes!(concat!(env!("OUT_DIR"), "/mask.vert.spv")),
        )?;
        let mask_frag = Shader::new(
            target.gpu.clone(),
            include_bytes!(concat!(env!("OUT_DIR"), "/mask.frag.spv")),
        )?;
        let full_vert = Shader::new(
            target.gpu.clone(),
            include_bytes!(concat!(env!("OUT_DIR"), "/sky.vert.spv")),
        )?;
        let outline_frag = Shader::new(
            target.gpu.clone(),
            include_bytes!(concat!(env!("OUT_DIR"), "/outline.frag.spv")),
        )?;
        // SAFETY: every handle is owned by `target` before the next fallible
        // call and released by Drop; create infos live through each call.
        unsafe {
            let device = &target.gpu.device;
            target.image = device.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(FORMAT)
                    .extent(vk::Extent3D {
                        width: extent.width.max(1),
                        height: extent.height.max(1),
                        depth: 1,
                    })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )?;
            let req = device.get_image_memory_requirements(target.image);
            target.memory = device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(memory_type(
                        &target.gpu,
                        req.memory_type_bits,
                        vk::MemoryPropertyFlags::DEVICE_LOCAL,
                    )?),
                None,
            )?;
            device.bind_image_memory(target.image, target.memory, 0)?;
            target.view = device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(target.image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(FORMAT)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .level_count(1)
                            .layer_count(1),
                    ),
                None,
            )?;
            target.sampler = device.create_sampler(
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
                .format(FORMAT)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let color_ref = [vk::AttachmentReference {
                attachment: 0,
                layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            }];
            let subpass = [vk::SubpassDescription::default()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(&color_ref)];
            // The previous frame's reads finish before this write; this write
            // finishes before the main pass samples it.
            let dependencies = [
                vk::SubpassDependency {
                    src_subpass: vk::SUBPASS_EXTERNAL,
                    dst_subpass: 0,
                    src_stage_mask: vk::PipelineStageFlags::FRAGMENT_SHADER,
                    dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    src_access_mask: vk::AccessFlags::SHADER_READ,
                    dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                        | vk::AccessFlags::COLOR_ATTACHMENT_READ,
                    dependency_flags: vk::DependencyFlags::empty(),
                },
                vk::SubpassDependency {
                    src_subpass: 0,
                    dst_subpass: vk::SUBPASS_EXTERNAL,
                    src_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    dst_stage_mask: vk::PipelineStageFlags::FRAGMENT_SHADER,
                    src_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    dst_access_mask: vk::AccessFlags::SHADER_READ,
                    dependency_flags: vk::DependencyFlags::empty(),
                },
            ];
            target.pass = device.create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&attachment)
                    .subpasses(&subpass)
                    .dependencies(&dependencies),
                None,
            )?;
            let views = [target.view];
            target.framebuffer = device.create_framebuffer(
                &vk::FramebufferCreateInfo::default()
                    .render_pass(target.pass)
                    .attachments(&views)
                    .width(extent.width.max(1))
                    .height(extent.height.max(1))
                    .layers(1),
                None,
            )?;
            // Mask pipeline: positions, MAX blending of the mask value.
            let ranges = [vk::PushConstantRange {
                stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                offset: 0,
                size: std::mem::size_of::<MaskPush>() as u32,
            }];
            target.mask_layout = device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().push_constant_ranges(&ranges),
                None,
            )?;
            let stages = [
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(mask_vert.handle)
                    .name(c"main"),
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(mask_frag.handle)
                    .name(c"main"),
            ];
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
            let max = vk::PipelineColorBlendAttachmentState::default()
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::ONE)
                .dst_color_blend_factor(vk::BlendFactor::ONE)
                .color_blend_op(vk::BlendOp::MAX)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE)
                .alpha_blend_op(vk::BlendOp::MAX)
                .color_write_mask(vk::ColorComponentFlags::RGBA);
            target.mask_pipeline = pipeline(
                &target.gpu,
                &stages,
                &vertex,
                max,
                target.mask_layout,
                target.pass,
            )?;
            // Outline pipelines in the main pass: mask image + sampler.
            let set_bindings = [
                vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            ];
            target.set_layout = device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&set_bindings),
                None,
            )?;
            let sizes = [
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::SAMPLED_IMAGE,
                    descriptor_count: 1,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::SAMPLER,
                    descriptor_count: 1,
                },
            ];
            target.pool = device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(1)
                    .pool_sizes(&sizes),
                None,
            )?;
            let layouts = [target.set_layout];
            target.set = device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(target.pool)
                    .set_layouts(&layouts),
            )?[0];
            let image = [vk::DescriptorImageInfo {
                sampler: vk::Sampler::null(),
                image_view: target.view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            let sampler = [vk::DescriptorImageInfo {
                sampler: target.sampler,
                ..Default::default()
            }];
            device.update_descriptor_sets(
                &[
                    vk::WriteDescriptorSet::default()
                        .dst_set(target.set)
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                        .image_info(&image),
                    vk::WriteDescriptorSet::default()
                        .dst_set(target.set)
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::SAMPLER)
                        .image_info(&sampler),
                ],
                &[],
            );
            let ranges = [vk::PushConstantRange {
                stage_flags: vk::ShaderStageFlags::FRAGMENT,
                offset: 0,
                size: std::mem::size_of::<OutlinePush>() as u32,
            }];
            target.layout = device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&layouts)
                    .push_constant_ranges(&ranges),
                None,
            )?;
            let stages = [
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(full_vert.handle)
                    .name(c"main"),
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(outline_frag.handle)
                    .name(c"main"),
            ];
            let empty = vk::PipelineVertexInputStateCreateInfo::default();
            let alpha = vk::PipelineColorBlendAttachmentState::default()
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .alpha_blend_op(vk::BlendOp::ADD)
                .color_write_mask(vk::ColorComponentFlags::RGBA);
            target.blend_pipeline = pipeline(
                &target.gpu,
                &stages,
                &empty,
                alpha,
                target.layout,
                main_pass,
            )?;
            let invert = vk::PipelineColorBlendAttachmentState::default()
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::ONE_MINUS_DST_COLOR)
                .dst_color_blend_factor(vk::BlendFactor::ZERO)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ZERO)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE)
                .alpha_blend_op(vk::BlendOp::ADD)
                .color_write_mask(vk::ColorComponentFlags::RGBA);
            target.invert_pipeline = pipeline(
                &target.gpu,
                &stages,
                &empty,
                invert,
                target.layout,
                main_pass,
            )?;
        }
        Ok(target)
    }
    /// Begins the mask pass (cleared to 0) over the whole image.
    ///
    /// # Safety
    /// `command` is recording and outside any render pass.
    pub unsafe fn begin(&self, command: vk::CommandBuffer) {
        let clear = [vk::ClearValue {
            color: vk::ClearColorValue { float32: [0.; 4] },
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
                        extent: self.extent,
                    })
                    .clear_values(&clear),
                vk::SubpassContents::INLINE,
            );
        }
    }
}
impl Drop for OutlineTarget {
    fn drop(&mut self) {
        // SAFETY: the swapchain owner waits for idle before dropping it.
        unsafe {
            let device = &self.gpu.device;
            device.destroy_pipeline(self.blend_pipeline, None);
            device.destroy_pipeline(self.invert_pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.set_layout, None);
            device.destroy_pipeline(self.mask_pipeline, None);
            device.destroy_pipeline_layout(self.mask_layout, None);
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
    fn push_layouts_fit_and_colours_follow_the_mode() {
        assert_eq!(std::mem::size_of::<MaskPush>(), 80);
        assert_eq!(std::mem::size_of::<OutlinePush>(), 48);
        let dynamic = OutlinePush::new(None, false);
        assert_eq!(dynamic.params[2], 1.);
        let red = OutlinePush::new(Some([255, 0, 0]), true);
        assert_eq!((red.params[2], red.selected), (0., [1., 0., 0., 1.]));
        let mid = OutlinePush::new(Some([128, 128, 128]), true).selected[0];
        assert!(mid < 0.3, "decoded to linear: {mid}");
        assert!(OutlineSettings::default().is_empty());
    }
}
