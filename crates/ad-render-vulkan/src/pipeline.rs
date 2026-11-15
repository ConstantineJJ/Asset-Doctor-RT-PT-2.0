use crate::{context::GpuResult, gpu::Gpu};
use ash::vk;
use std::{io::Cursor, sync::Arc};

#[repr(C)]
pub(crate) struct Push {
    pub view_projection: [[f32; 4]; 4],
    pub base_color: [f32; 4],
    pub encode_srgb: u32,
    pub padding: [u32; 3],
}
pub(crate) struct MeshPipeline {
    pub handle: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    gpu: Arc<Gpu>,
}
struct Shader {
    handle: vk::ShaderModule,
    gpu: Arc<Gpu>,
}
impl Shader {
    fn new(gpu: Arc<Gpu>, bytes: &[u8]) -> GpuResult<Self> {
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
            layout: vk::PipelineLayout::null(),
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
                &vk::PipelineLayoutCreateInfo::default().push_constant_ranges(&ranges),
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
                .depth_compare_op(vk::CompareOp::LESS);
            let attachments = [vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)];
            let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
            let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
            let dynamic =
                vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
            let create = [vk::GraphicsPipelineCreateInfo::default()
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
                .subpass(0)];
            match pipeline.gpu.device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                &create,
                None,
            ) {
                Ok(handles) => pipeline.handle = handles[0],
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
            self.gpu.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}
