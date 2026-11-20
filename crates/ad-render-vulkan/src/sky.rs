//! ADR-0040 increment 3: environment sky behind the model. View only; it never
//! changes lamps, exposure or fill, and they never change it (REQ-ENV-001).
use crate::{context::GpuResult, gpu::Gpu, pipeline::Shader};
use ash::vk;
use std::sync::Arc;

/// Environment presets. `DarkStudio` is the plain clear colour (no sky pass).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Environment {
    DarkStudio,
    Day,
    Sunset,
    Night,
    Overcast,
    LightStudio,
}
impl Environment {
    pub const ALL: [Self; 6] = [
        Self::DarkStudio,
        Self::Day,
        Self::Sunset,
        Self::Night,
        Self::Overcast,
        Self::LightStudio,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::DarkStudio => "Тёмная студия",
            Self::Day => "День",
            Self::Sunset => "Закат",
            Self::Night => "Ночь",
            Self::Overcast => "Пасмурно",
            Self::LightStudio => "Светлая студия",
        }
    }
    /// Zenith, horizon and ground colours; None draws no sky (clear colour).
    pub fn colors(self) -> Option<[[f32; 3]; 3]> {
        match self {
            Self::DarkStudio => None,
            Self::Day => Some([[0.18, 0.40, 0.78], [0.70, 0.82, 0.94], [0.34, 0.36, 0.34]]),
            Self::Sunset => Some([[0.16, 0.18, 0.42], [0.96, 0.58, 0.32], [0.22, 0.16, 0.16]]),
            Self::Night => Some([[0.01, 0.02, 0.06], [0.08, 0.10, 0.20], [0.02, 0.02, 0.03]]),
            Self::Overcast => Some([[0.50, 0.53, 0.57], [0.70, 0.72, 0.74], [0.36, 0.37, 0.38]]),
            Self::LightStudio => Some([[0.78, 0.80, 0.83], [0.88, 0.89, 0.90], [0.66, 0.67, 0.69]]),
        }
    }
}

/// sRGB display value to linear.
fn linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
impl Environment {
    /// Colours that light the model: the sky, or a dim neutral studio for the
    /// dark studio (its clear colour would be almost black).
    pub fn light_colors(self) -> [[f32; 3]; 3] {
        self.colors()
            .unwrap_or([[0.30, 0.32, 0.36], [0.20, 0.21, 0.23], [0.08, 0.08, 0.09]])
    }
}

/// ADR-0040 environment light, separate from lamps, exposure and fill.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvironmentLight {
    pub environment: Environment,
    /// Multiplier, 0..=2.
    pub strength: f32,
}
impl EnvironmentLight {
    pub const MAX_STRENGTH: f32 = 2.;
    pub fn valid(self) -> bool {
        self.strength.is_finite() && (0.0..=Self::MAX_STRENGTH).contains(&self.strength)
    }
    /// Linear zenith (w = strength), horizon and ground for the frame buffer;
    /// zeros (off) when None or invalid.
    pub(crate) fn frame_colors(light: Option<Self>) -> [[f32; 4]; 3] {
        match light.filter(|l| l.valid() && l.strength > 0.) {
            None => [[0.; 4]; 3],
            Some(l) => {
                let [z, h, g] = l.environment.light_colors();
                let v = |c: [f32; 3], w| [linear(c[0]), linear(c[1]), linear(c[2]), w];
                [v(z, l.strength), v(h, 0.), v(g, 0.)]
            }
        }
    }
}

/// What the sky pass needs for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkySettings {
    /// Inverse of the camera's view-projection (reversed-Z).
    pub inverse_view_projection: [[f32; 4]; 4],
    pub environment: Environment,
}

#[repr(C)]
pub(crate) struct SkyPush {
    pub inverse_view_projection: [[f32; 4]; 4],
    pub zenith: [f32; 4],
    pub horizon: [f32; 4],
    pub ground: [f32; 4],
}
impl SkyPush {
    /// None when the environment draws no sky. `decode` asks the shader to
    /// write linear colours because the swapchain encodes sRGB itself.
    pub fn new(sky: SkySettings, decode: bool) -> Option<Self> {
        let [z, h, g] = sky.environment.colors()?;
        let v = |c: [f32; 3], w| [c[0], c[1], c[2], w];
        Some(Self {
            inverse_view_projection: sky.inverse_view_projection,
            zenith: v(z, if decode { 1. } else { 0. }),
            horizon: v(h, 1.),
            ground: v(g, 1.),
        })
    }
}

pub(crate) struct SkyPipeline {
    pub handle: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    gpu: Arc<Gpu>,
}
impl SkyPipeline {
    pub fn new(gpu: Arc<Gpu>, pass: vk::RenderPass) -> GpuResult<Self> {
        let vert = Shader::new(
            gpu.clone(),
            include_bytes!(concat!(env!("OUT_DIR"), "/sky.vert.spv")),
        )?;
        let frag = Shader::new(
            gpu.clone(),
            include_bytes!(concat!(env!("OUT_DIR"), "/sky.frag.spv")),
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
                stage_flags: vk::ShaderStageFlags::FRAGMENT,
                offset: 0,
                size: std::mem::size_of::<SkyPush>() as u32,
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
            // Background: drawn first, never tested or written to depth.
            let depth = vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(false)
                .depth_write_enable(false);
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
impl Drop for SkyPipeline {
    fn drop(&mut self) {
        // SAFETY: the swapchain/renderer waits for using work before release.
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
    fn sky_push_fits_the_budget_and_presets_are_distinct() {
        assert_eq!(std::mem::size_of::<SkyPush>(), 112);
        let labels: std::collections::BTreeSet<_> =
            Environment::ALL.iter().map(|e| e.label()).collect();
        assert_eq!(labels.len(), Environment::ALL.len());
        let colors: std::collections::BTreeSet<_> = Environment::ALL
            .iter()
            .filter_map(|e| e.colors())
            .map(|c| c.map(|v| v.map(f32::to_bits)))
            .collect();
        assert_eq!(colors.len(), Environment::ALL.len() - 1);
        let identity = [
            [1., 0., 0., 0.],
            [0., 1., 0., 0.],
            [0., 0., 1., 0.],
            [0., 0., 0., 1.],
        ];
        let sky = |environment| SkySettings {
            inverse_view_projection: identity,
            environment,
        };
        assert!(SkyPush::new(sky(Environment::DarkStudio), false).is_none());
        let day = SkyPush::new(sky(Environment::Day), false).unwrap();
        assert_eq!(day.zenith[..3], Environment::Day.colors().unwrap()[0]);
        assert_eq!(day.zenith[3], 0.);
        assert_eq!(
            SkyPush::new(sky(Environment::Day), true).unwrap().zenith[3],
            1.
        );
    }
    #[test]
    fn environment_light_is_off_by_default_bounded_and_linear() {
        let off = [[0.; 4]; 3];
        assert_eq!(EnvironmentLight::frame_colors(None), off);
        for strength in [0., -0.1, 2.1, f32::NAN, f32::INFINITY] {
            let light = EnvironmentLight {
                environment: Environment::Day,
                strength,
            };
            assert_eq!(EnvironmentLight::frame_colors(Some(light)), off);
        }
        for environment in Environment::ALL {
            let light = EnvironmentLight {
                environment,
                strength: 1.,
            };
            let [z, h, g] = EnvironmentLight::frame_colors(Some(light));
            assert_eq!(z[3], 1.);
            let display = environment.light_colors();
            for (linear, shown) in [z, h, g].iter().zip(display) {
                for i in 0..3 {
                    assert!(linear[i] > 0. && linear[i] <= shown[i], "{environment:?}");
                }
            }
        }
    }
}
