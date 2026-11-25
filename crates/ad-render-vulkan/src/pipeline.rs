use crate::frame::FrameLayout;
use crate::textures::TextureLayout;
use crate::{context::GpuResult, gpu::Gpu};
use ash::vk;
use std::{io::Cursor, sync::Arc};

#[repr(C)]
pub(crate) struct Push {
    pub base_color: [f32; 4],
    pub material: [f32; 4],
    pub view: [f32; 4],
    pub emissive: [f32; 4],
}
pub(crate) struct MeshPipeline {
    pub handle: vk::Pipeline,
    /// Wireframe overlay; null when the GPU lacks line polygon mode.
    pub wire: vk::Pipeline,
    /// glTF BLEND: alpha blending, depth test without depth writes.
    pub blend: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    texture_layout: TextureLayout,
    frame_layout: FrameLayout,
    gpu: Arc<Gpu>,
}
pub(crate) struct Shader {
    pub handle: vk::ShaderModule,
    gpu: Arc<Gpu>,
}
impl Shader {
    pub fn new(gpu: Arc<Gpu>, bytes: &[u8]) -> GpuResult<Self> {
        let code = ash::util::read_spv(&mut Cursor::new(bytes))?;
        // SAFETY: Build-time validated SPIR-V, correctly aligned u32 input.
        let handle = unsafe {
            gpu.device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?
        };
        Ok(Self { handle, gpu })
    }
}
impl Drop for Shader {
    fn drop(&mut self) {
        // SAFETY: Shader modules can be released after pipeline creation returns.
        unsafe {
            self.gpu.device.destroy_shader_module(self.handle, None);
        }
    }
}
impl MeshPipeline {
    pub fn new(gpu: Arc<Gpu>, pass: vk::RenderPass) -> GpuResult<Self> {
        let vert = Shader::new(
            gpu.clone(),
            include_bytes!(concat!(env!("OUT_DIR"), "/mesh.vert.spv")),
        )?;
        let frag = Shader::new(
            gpu.clone(),
            include_bytes!(concat!(env!("OUT_DIR"), "/mesh.frag.spv")),
        )?;
        let mut pipeline = Self {
            handle: vk::Pipeline::null(),
            wire: vk::Pipeline::null(),
            blend: vk::Pipeline::null(),
            layout: vk::PipelineLayout::null(),
            texture_layout: TextureLayout::new(gpu.clone())?,
            frame_layout: FrameLayout::new(gpu.clone())?,
            gpu,
        };
        // SAFETY: All create-info arrays and shader modules live through the
        // creation call. Partial pipeline handles are explicitly freed on error.
        unsafe {
            let ranges = [vk::PushConstantRange {
                stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                offset: 0,
                size: std::mem::size_of::<Push>() as u32,
            }];
            pipeline.layout = pipeline.gpu.device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .push_constant_ranges(&ranges)
                    .set_layouts(&[pipeline.frame_layout.handle, pipeline.texture_layout.handle]),
                None,
            )?;
            let stages = [
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(vert.handle)
                    .name(c"main"),
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(frag.handle)
                    .name(c"main"),
            ];
            let bindings = [
                vk::VertexInputBindingDescription {
                    binding: 0,
                    stride: 12,
                    input_rate: vk::VertexInputRate::VERTEX,
                },
                vk::VertexInputBindingDescription {
                    binding: 1,
                    stride: 8,
                    input_rate: vk::VertexInputRate::VERTEX,
                },
                vk::VertexInputBindingDescription {
                    binding: 2,
                    stride: 16,
                    input_rate: vk::VertexInputRate::VERTEX,
                },
                vk::VertexInputBindingDescription {
                    binding: 3,
                    stride: 12,
                    input_rate: vk::VertexInputRate::VERTEX,
                },
                vk::VertexInputBindingDescription {
                    binding: 4,
                    stride: 8,
                    input_rate: vk::VertexInputRate::VERTEX,
                },
                vk::VertexInputBindingDescription {
                    binding: 5,
                    stride: 8,
                    input_rate: vk::VertexInputRate::VERTEX,
                },
                vk::VertexInputBindingDescription {
                    binding: 6,
                    stride: 16,
                    input_rate: vk::VertexInputRate::VERTEX,
                },
                vk::VertexInputBindingDescription {
                    binding: 7,
                    stride: 8,
                    input_rate: vk::VertexInputRate::VERTEX,
                },
                vk::VertexInputBindingDescription {
                    binding: 8,
                    stride: 8,
                    input_rate: vk::VertexInputRate::VERTEX,
                },
            ];
            let attributes = [
                vk::VertexInputAttributeDescription {
                    location: 0,
                    binding: 0,
                    format: vk::Format::R32G32B32_SFLOAT,
                    offset: 0,
                },
                vk::VertexInputAttributeDescription {
                    location: 1,
                    binding: 1,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: 0,
                },
                vk::VertexInputAttributeDescription {
                    location: 2,
                    binding: 2,
                    format: vk::Format::R32G32B32A32_SFLOAT,
                    offset: 0,
                },
                vk::VertexInputAttributeDescription {
                    location: 3,
                    binding: 3,
                    format: vk::Format::R32G32B32_SFLOAT,
                    offset: 0,
                },
                vk::VertexInputAttributeDescription {
                    location: 4,
                    binding: 4,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: 0,
                },
                vk::VertexInputAttributeDescription {
                    location: 5,
                    binding: 5,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: 0,
                },
                vk::VertexInputAttributeDescription {
                    location: 6,
                    binding: 6,
                    format: vk::Format::R32G32B32A32_SFLOAT,
                    offset: 0,
                },
                vk::VertexInputAttributeDescription {
                    location: 7,
                    binding: 7,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: 0,
                },
                vk::VertexInputAttributeDescription {
                    location: 8,
                    binding: 8,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: 0,
                },
            ];
            let vertex = vk::PipelineVertexInputStateCreateInfo::default()
                .vertex_binding_descriptions(&bindings)
                .vertex_attribute_descriptions(&attributes);
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
                .depth_test_enable(true)
                .depth_write_enable(true)
                // Reversed-Z: nearer fragments have larger depth.
                .depth_compare_op(vk::CompareOp::GREATER);
            // Wireframe overlay: the same triangles as lines over the filled
            // surface; equal depth passes and a slope bias pulls lines forward
            // (towards larger depth with reversed-Z). No depth writes.
            let wire_raster = vk::PipelineRasterizationStateCreateInfo::default()
                .polygon_mode(vk::PolygonMode::LINE)
                .cull_mode(vk::CullModeFlags::NONE)
                .depth_bias_enable(true)
                .depth_bias_slope_factor(1.0)
                .line_width(1.0);
            let wire_depth = vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(false)
                .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL);
            let attachments = [vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)];
            let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
            let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
            let dynamic =
                vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
            let fill = vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vertex)
                .input_assembly_state(&assembly)
                .viewport_state(&viewport)
                .rasterization_state(&raster)
                .multisample_state(&samples)
                .depth_stencil_state(&depth)
                .color_blend_state(&blend)
                .dynamic_state(&dynamic)
                .layout(pipeline.layout)
                .render_pass(pass)
                .subpass(0);
            let wire = fill
                .rasterization_state(&wire_raster)
                .depth_stencil_state(&wire_depth);
            let both = [fill, wire];
            let create = if pipeline.gpu.wireframe {
                &both[..]
            } else {
                &both[..1]
            };
            match pipeline.gpu.device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                create,
                None,
            ) {
                Ok(handles) => {
                    pipeline.handle = handles[0];
                    pipeline.wire = handles.get(1).copied().unwrap_or_default();
                }
                Err((handles, error)) => {
                    for handle in handles {
                        pipeline.gpu.device.destroy_pipeline(handle, None);
                    }
                    return Err(error.into());
                }
            }
            let blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .alpha_blend_op(vk::BlendOp::ADD)
                .color_write_mask(vk::ColorComponentFlags::RGBA)];
            let blend_state =
                vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);
            let blend_depth = vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(false)
                .depth_compare_op(vk::CompareOp::GREATER);
            let blended = fill
                .color_blend_state(&blend_state)
                .depth_stencil_state(&blend_depth);
            match pipeline.gpu.device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[blended],
                None,
            ) {
                Ok(handles) => pipeline.blend = handles[0],
                Err((handles, error)) => {
                    for handle in handles {
                        pipeline.gpu.device.destroy_pipeline(handle, None);
                    }
                    return Err(error.into());
                }
            }
        }
        Ok(pipeline)
    }
}
impl Drop for MeshPipeline {
    fn drop(&mut self) {
        // SAFETY: Swapchain/Renderer waits for all using work before pipeline release.
        unsafe {
            self.gpu.device.destroy_pipeline(self.handle, None);
            self.gpu.device.destroy_pipeline(self.wire, None);
            self.gpu.device.destroy_pipeline(self.blend, None);
            self.gpu.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Push;
    #[test]
    fn push_layout_matches_glsl_and_guaranteed_vulkan_budget() {
        assert_eq!(std::mem::size_of::<Push>(), 64);
        assert_eq!(std::mem::offset_of!(Push, base_color), 0);
        assert_eq!(std::mem::offset_of!(Push, material), 16);
        assert_eq!(std::mem::offset_of!(Push, view), 32);
        assert_eq!(std::mem::offset_of!(Push, emissive), 48);
    }
}
