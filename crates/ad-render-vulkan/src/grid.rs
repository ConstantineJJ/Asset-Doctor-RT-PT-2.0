//! ADR-0040: procedural floor grid. View-only; never document state.
use crate::{context::GpuResult, gpu::Gpu, pipeline::Shader};
use ash::vk;
use std::sync::Arc;

/// Procedural floor themes (ADR-0040 increment 2). One is active at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloorTheme {
    StudioGrid,
    DarkMatte,
    LightStudio,
    Checker,
    Tiles,
}
impl FloorTheme {
    pub const ALL: [Self; 5] = [
        Self::StudioGrid,
        Self::DarkMatte,
        Self::LightStudio,
        Self::Checker,
        Self::Tiles,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::StudioGrid => "Сетка студии",
            Self::DarkMatte => "Тёмный матовый",
            Self::LightStudio => "Светлая студия",
            Self::Checker => "Шахматка",
            Self::Tiles => "Плитка",
        }
    }
    /// Theme index read by grid.frag.
    fn shader_id(self) -> f32 {
        match self {
            Self::StudioGrid => 0.,
            Self::DarkMatte => 1.,
            Self::LightStudio => 2.,
            Self::Checker => 3.,
            Self::Tiles => 4.,
        }
    }
}
/// Floor placement in world units and its theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridSettings {
    pub floor_y: f32,
    pub center: [f32; 2],
    pub extent: f32,
    pub spacing: f32,
    pub theme: FloorTheme,
}
/// Largest 1/2/5×10ⁿ step not above `x` (x > 0).
pub fn nice_step(x: f32) -> f32 {
    let x = x.max(f32::MIN_POSITIVE);
    let base = 10f32.powf(x.log10().floor());
    [5., 2., 1.]
        .into_iter()
        .map(|m| m * base)
        .find(|&s| s <= x * 1.0001)
        .unwrap_or(base)
}
/// Grid under the model: floor at its lowest point, step near radius / 4.
pub fn grid_for_bounds(min: [f32; 3], max: [f32; 3]) -> Option<GridSettings> {
    if min.iter().chain(&max).any(|v| !v.is_finite()) || (0..3).any(|i| min[i] > max[i]) {
        return None;
    }
    let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let radius = (size[0] * size[0] + size[1] * size[1] + size[2] * size[2])
        .sqrt()
        .max(1e-4)
        * 0.5;
    Some(GridSettings {
        floor_y: min[1],
        center: [(min[0] + max[0]) * 0.5, (min[2] + max[2]) * 0.5],
        extent: radius * 4.,
        spacing: nice_step(radius / 4.),
        theme: FloorTheme::StudioGrid,
    })
}

#[repr(C)]
pub(crate) struct GridPush {
    pub view_projection: [[f32; 4]; 4],
    /// center x, floor y, center z, extent
    pub plane: [f32; 4],
    /// spacing, major every N, unused, opacity
    pub style: [f32; 4],
}
impl GridPush {
    pub fn new(view_projection: [[f32; 4]; 4], grid: GridSettings) -> Self {
        Self {
            view_projection,
            plane: [grid.center[0], grid.floor_y, grid.center[1], grid.extent],
            style: [grid.spacing, 5., grid.theme.shader_id(), 1.],
        }
    }
}

pub(crate) struct GridPipeline {
    pub handle: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    gpu: Arc<Gpu>,
}
impl GridPipeline {
    pub fn new(gpu: Arc<Gpu>, pass: vk::RenderPass) -> GpuResult<Self> {
        let vert = Shader::new(
            gpu.clone(),
            include_bytes!(concat!(env!("OUT_DIR"), "/grid.vert.spv")),
        )?;
        let frag = Shader::new(
            gpu.clone(),
            include_bytes!(concat!(env!("OUT_DIR"), "/grid.frag.spv")),
        )?;
        let mut pipeline = Self {
            handle: vk::Pipeline::null(),
            layout: vk::PipelineLayout::null(),
            gpu,
        };
        // SAFETY: create-info arrays and shader modules live through creation;
        // partial handles are freed on error and by Drop.
        unsafe {
            let ranges = [vk::PushConstantRange {
                stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                offset: 0,
                size: std::mem::size_of::<GridPush>() as u32,
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
            let vertex = vk::PipelineVertexInputStateCreateInfo::default();
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
            // Tested against the model (reversed-Z) but never written: the
            // model always occludes the grid, the grid never hides the model.
            let depth = vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(false)
                .depth_compare_op(vk::CompareOp::GREATER);
            let attachments = [vk::PipelineColorBlendAttachmentState::default()
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ZERO)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE)
                .alpha_blend_op(vk::BlendOp::ADD)
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
impl Drop for GridPipeline {
    fn drop(&mut self) {
        // SAFETY: the swapchain/renderer waits for using work before release;
        // null handles are ignored by Vulkan.
        unsafe {
            self.gpu.device.destroy_pipeline(self.handle, None);
            self.gpu.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grid_push_fits_the_guaranteed_push_budget_and_matches_glsl() {
        assert_eq!(std::mem::size_of::<GridPush>(), 96);
        assert_eq!(std::mem::offset_of!(GridPush, plane), 64);
        assert_eq!(std::mem::offset_of!(GridPush, style), 80);
    }
    #[test]
    fn floor_themes_are_distinct_and_reach_the_shader() {
        let labels: std::collections::BTreeSet<_> =
            FloorTheme::ALL.iter().map(|t| t.label()).collect();
        assert_eq!(labels.len(), FloorTheme::ALL.len(), "unique labels");
        let ids: std::collections::BTreeSet<_> = FloorTheme::ALL
            .iter()
            .map(|t| t.shader_id() as u32)
            .collect();
        assert_eq!(ids.len(), FloorTheme::ALL.len(), "distinct shader ids");
        let mut g = grid_for_bounds([0.; 3], [1.; 3]).unwrap();
        assert_eq!(g.theme, FloorTheme::StudioGrid);
        g.theme = FloorTheme::Tiles;
        assert_eq!(GridPush::new([[0.; 4]; 4], g).style[2], 4.);
    }
    #[test]
    fn nice_steps_and_grid_follow_model_size_and_lowest_point() {
        assert_eq!(nice_step(0.3), 0.2);
        assert_eq!(nice_step(7.0), 5.0);
        assert_eq!(nice_step(1.0), 1.0);
        assert_eq!(nice_step(0.012), 0.01);
        let g = grid_for_bounds([-1., -0.5, -1.], [1., 3.5, 1.]).unwrap();
        assert_eq!(g.floor_y, -0.5);
        assert_eq!(g.center, [0., 0.]);
        assert!(g.extent > 4. && g.spacing == 0.5);
        assert!(grid_for_bounds([f32::NAN, 0., 0.], [1., 1., 1.]).is_none());
        assert!(grid_for_bounds([1., 0., 0.], [0., 1., 1.]).is_none());
        // Huge and tiny models keep finite, proportional grids.
        let tiny = grid_for_bounds([0.; 3], [1e-3; 3]).unwrap();
        assert!(tiny.spacing > 0. && tiny.spacing < 1e-3);
    }
}
