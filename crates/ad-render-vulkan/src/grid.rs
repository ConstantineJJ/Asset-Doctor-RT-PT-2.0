//! ADR-0040: procedural floor grid. View-only; never document state.
use crate::{context::GpuResult, gpu::Gpu, pipeline::Shader};
use ash::vk;
use std::sync::Arc;

/// Procedural floor themes (ADR-0040). One is active at a time; every theme
/// is a shader pattern plus two colours, so no textures are involved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloorTheme {
    StudioGrid,
    DarkMatte,
    LightStudio,
    Checker,
    Tiles,
    Grass,
    DryGrass,
    Countryside,
    Sand,
    Asphalt,
    Cobblestone,
    RedBrick,
    Moon,
    Mars,
    Lake,
    Snow,
    Clay,
    Moss,
    BlueStudio,
    PurpleStudio,
}
/// Shader pattern of a theme (grid.frag `style.z`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    Grid = 0,
    Matte = 1,
    Light = 2,
    Checker = 3,
    Tiles = 4,
    Noise = 5,
    Water = 6,
    Bricks = 7,
    Stones = 8,
    Fields = 9,
}
impl FloorTheme {
    pub const ALL: [Self; 20] = [
        Self::StudioGrid,
        Self::DarkMatte,
        Self::LightStudio,
        Self::Checker,
        Self::Tiles,
        Self::Grass,
        Self::DryGrass,
        Self::Countryside,
        Self::Sand,
        Self::Asphalt,
        Self::Cobblestone,
        Self::RedBrick,
        Self::Moon,
        Self::Mars,
        Self::Lake,
        Self::Snow,
        Self::Clay,
        Self::Moss,
        Self::BlueStudio,
        Self::PurpleStudio,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::StudioGrid => "Сетка студии",
            Self::DarkMatte => "Тёмный матовый",
            Self::LightStudio => "Светлая студия",
            Self::Checker => "Шахматка",
            Self::Tiles => "Плитка",
            Self::Grass => "Трава",
            Self::DryGrass => "Увядшая трава",
            Self::Countryside => "Сельская местность",
            Self::Sand => "Песок",
            Self::Asphalt => "Асфальт",
            Self::Cobblestone => "Брусчатка",
            Self::RedBrick => "Красный кирпич",
            Self::Moon => "Лунная поверхность",
            Self::Mars => "Марс",
            Self::Lake => "Озеро",
            Self::Snow => "Снег",
            Self::Clay => "Глина",
            Self::Moss => "Мох",
            Self::BlueStudio => "Синяя студия",
            Self::PurpleStudio => "Пурпурная студия",
        }
    }
    /// Pattern and two colours (sRGB-ish display values).
    pub fn look(self) -> (Pattern, [f32; 3], [f32; 3]) {
        use Pattern::*;
        match self {
            Self::StudioGrid => (Grid, [0.55, 0.58, 0.62], [0.55, 0.58, 0.62]),
            Self::DarkMatte => (Matte, [0.10, 0.11, 0.12], [0.20, 0.21, 0.23]),
            Self::LightStudio => (Light, [0.64, 0.65, 0.67], [0.52, 0.53, 0.55]),
            Self::Checker => (Checker, [0.20, 0.21, 0.23], [0.42, 0.43, 0.46]),
            Self::Tiles => (Tiles, [0.52, 0.49, 0.45], [0.26, 0.24, 0.22]),
            Self::Grass => (Noise, [0.20, 0.42, 0.12], [0.34, 0.58, 0.18]),
            Self::DryGrass => (Noise, [0.50, 0.44, 0.22], [0.66, 0.58, 0.32]),
            Self::Countryside => (Fields, [0.36, 0.52, 0.20], [0.62, 0.52, 0.26]),
            Self::Sand => (Noise, [0.76, 0.66, 0.46], [0.86, 0.78, 0.58]),
            Self::Asphalt => (Noise, [0.15, 0.15, 0.16], [0.27, 0.27, 0.28]),
            Self::Cobblestone => (Stones, [0.45, 0.43, 0.40], [0.20, 0.19, 0.18]),
            Self::RedBrick => (Bricks, [0.58, 0.22, 0.16], [0.72, 0.68, 0.62]),
            Self::Moon => (Noise, [0.42, 0.42, 0.44], [0.62, 0.62, 0.64]),
            Self::Mars => (Noise, [0.55, 0.24, 0.12], [0.74, 0.40, 0.22]),
            Self::Lake => (Water, [0.06, 0.24, 0.40], [0.18, 0.46, 0.62]),
            Self::Snow => (Noise, [0.84, 0.87, 0.92], [0.96, 0.97, 1.00]),
            Self::Clay => (Noise, [0.62, 0.34, 0.22], [0.72, 0.46, 0.30]),
            Self::Moss => (Noise, [0.14, 0.24, 0.10], [0.24, 0.34, 0.14]),
            Self::BlueStudio => (Light, [0.16, 0.30, 0.58], [0.12, 0.22, 0.44]),
            Self::PurpleStudio => (Light, [0.42, 0.20, 0.52], [0.32, 0.14, 0.40]),
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
    /// spacing, major every N, pattern, opacity
    pub style: [f32; 4],
    /// theme colours a and b (rgb, w unused)
    pub color_a: [f32; 4],
    pub color_b: [f32; 4],
}
impl GridPush {
    pub fn new(view_projection: [[f32; 4]; 4], grid: GridSettings) -> Self {
        let (pattern, a, b) = grid.theme.look();
        Self {
            view_projection,
            plane: [grid.center[0], grid.floor_y, grid.center[1], grid.extent],
            style: [grid.spacing, 5., pattern as u32 as f32, 1.],
            color_a: [a[0], a[1], a[2], 0.],
            color_b: [b[0], b[1], b[2], 0.],
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
        // 128 bytes is the push-constant size every Vulkan device guarantees.
        assert_eq!(std::mem::size_of::<GridPush>(), 128);
        assert_eq!(std::mem::offset_of!(GridPush, plane), 64);
        assert_eq!(std::mem::offset_of!(GridPush, style), 80);
        assert_eq!(std::mem::offset_of!(GridPush, color_a), 96);
        assert_eq!(std::mem::offset_of!(GridPush, color_b), 112);
    }
    #[test]
    fn floor_themes_are_distinct_and_reach_the_shader() {
        let labels: std::collections::BTreeSet<_> =
            FloorTheme::ALL.iter().map(|t| t.label()).collect();
        assert_eq!(labels.len(), FloorTheme::ALL.len(), "unique labels");
        // Distinct looks: no two themes share pattern and both colours.
        let looks: std::collections::BTreeSet<_> = FloorTheme::ALL
            .iter()
            .map(|t| {
                let (p, a, b) = t.look();
                (p as u32, a.map(f32::to_bits), b.map(f32::to_bits))
            })
            .collect();
        assert_eq!(looks.len(), FloorTheme::ALL.len(), "distinct looks");
        // Every colour is a finite display value.
        for theme in FloorTheme::ALL {
            let (_, a, b) = theme.look();
            assert!(
                a.iter().chain(&b).all(|v| (0.0..=1.0).contains(v)),
                "{theme:?}"
            );
        }
        let mut g = grid_for_bounds([0.; 3], [1.; 3]).unwrap();
        assert_eq!(g.theme, FloorTheme::StudioGrid);
        g.theme = FloorTheme::Lake;
        let push = GridPush::new([[0.; 4]; 4], g);
        assert_eq!(push.style[2], Pattern::Water as u32 as f32);
        assert_eq!(push.color_a[..3], FloorTheme::Lake.look().1);
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
