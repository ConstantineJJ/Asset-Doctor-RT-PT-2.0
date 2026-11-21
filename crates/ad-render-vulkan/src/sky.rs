//! ADR-0040 increment 3: environment sky behind the model. View only; it never
//! changes lamps, exposure or fill, and they never change it (REQ-ENV-001).
use crate::{
    context::GpuResult,
    gpu::Gpu,
    pipeline::Shader,
    textures::{GpuImage, Sampler},
};
use ad_document::{Filter, Mipmap, TextureImage, TextureSampler, Wrap};
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
pub fn srgb_to_linear(c: f32) -> f32 {
    linear(c)
}
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

/// sRGB display value from linear.
fn display(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1. / 2.4) - 0.055
    }
}

/// ADR-0040 skybox light: average display colours of the panorama's upper cap
/// (zenith), the band around the horizon and the lower half (ground). Averaged
/// in linear light over a sparse grid, so an 8K image costs little.
pub fn panorama_light_colors(image: &TextureImage) -> Option<[[f32; 3]; 3]> {
    let (w, h) = (image.width as usize, image.height as usize);
    if w == 0 || h == 0 || image.rgba.len() != w * h * 4 {
        return None;
    }
    let step_x = (w / 256).max(1);
    let step_y = (h / 128).max(1);
    let mut sums = [[0f64; 4]; 3];
    for y in (0..h).step_by(step_y) {
        // Row centre to elevation: v = 0 is straight up, v = 1 straight down.
        let v = (y as f64 + 0.5) / h as f64;
        let band = if v < 0.2 {
            0
        } else if (0.42..0.58).contains(&v) {
            1
        } else if v > 0.6 {
            2
        } else {
            continue;
        };
        for x in (0..w).step_by(step_x) {
            let i = (y * w + x) * 4;
            for c in 0..3 {
                sums[band][c] += f64::from(linear(f32::from(image.rgba[i + c]) / 255.));
            }
            sums[band][3] += 1.;
        }
    }
    let mut out = [[0f32; 3]; 3];
    for (band, sum) in sums.iter().enumerate() {
        if sum[3] == 0. {
            return None;
        }
        for c in 0..3 {
            out[band][c] = display((sum[c] / sum[3]) as f32);
        }
    }
    Some(out)
}

/// ADR-0040 environment light, separate from lamps, exposure and fill.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvironmentLight {
    /// Display zenith, horizon and ground colours (preset or panorama).
    pub colors: [[f32; 3]; 3],
    /// Multiplier, 0..=2.
    pub strength: f32,
}
impl EnvironmentLight {
    pub const MAX_STRENGTH: f32 = 2.;
    pub fn preset(environment: Environment, strength: f32) -> Self {
        Self {
            colors: environment.light_colors(),
            strength,
        }
    }
    pub fn valid(self) -> bool {
        self.strength.is_finite()
            && (0.0..=Self::MAX_STRENGTH).contains(&self.strength)
            && self
                .colors
                .iter()
                .flatten()
                .all(|c| (0.0..=1.0).contains(c))
    }
    /// Linear zenith (w = strength), horizon and ground for the frame buffer;
    /// zeros (off) when None or invalid.
    pub(crate) fn frame_colors(light: Option<Self>) -> [[f32; 4]; 3] {
        match light.filter(|l| l.valid() && l.strength > 0.) {
            None => [[0.; 4]; 3],
            Some(l) => {
                let [z, h, g] = l.colors;
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
    /// Some(yaw in radians): draw the loaded panorama instead of the preset.
    pub panorama: Option<f32>,
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
    /// Push for the panorama pass: decode flag, HDR flag with the exposure
    /// (EV) it is shown at, and yaw.
    pub fn panorama(sky: SkySettings, yaw: f32, decode: bool, hdr: bool, exposure_ev: f32) -> Self {
        Self {
            inverse_view_projection: sky.inverse_view_projection,
            zenith: [0., 0., 0., if decode { 1. } else { 0. }],
            horizon: [if hdr { 1. } else { 0. }, exposure_ev, 0., 0.],
            ground: [0., 0., 0., yaw],
        }
    }
}

/// Descriptor layout of the panorama pass: image (0) and sampler (1).
fn panorama_layout(gpu: &Gpu) -> GpuResult<vk::DescriptorSetLayout> {
    let bindings = [
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
    // SAFETY: bindings live through creation; the owner destroys the layout.
    Ok(unsafe {
        gpu.device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )?
    })
}

/// The owner's panorama on the GPU (ADR-0047: linear RGBA16F with its
/// prefiltered levels) with its descriptor set, the irradiance coefficients
/// and the HDR flag. Replaced only after the renderer waited for idle; never
/// part of the document.
pub(crate) struct Skybox {
    pub set: vk::DescriptorSet,
    pub sh: [[f32; 3]; 9],
    pub max_lod: f32,
    pub hdr: bool,
    layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    pub(crate) sampler: Sampler,
    pub(crate) image: GpuImage,
    gpu: Arc<Gpu>,
}
impl Skybox {
    pub fn new(gpu: Arc<Gpu>, map: &crate::ibl::EnvironmentMap) -> GpuResult<Self> {
        let image = GpuImage::upload_levels(gpu.clone(), map.width, map.height, &map.levels)?;
        let sampler = Sampler::new(
            gpu.clone(),
            TextureSampler {
                min: Filter::Linear,
                mag: Filter::Linear,
                mipmap: Mipmap::Linear,
                u: Wrap::Repeat,
                v: Wrap::ClampToEdge,
            },
            map.levels.len() as u32,
        )?;
        let mut skybox = Self {
            set: vk::DescriptorSet::null(),
            sh: map.sh,
            max_lod: map.max_lod(),
            hdr: map.hdr,
            layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            sampler,
            image,
            gpu,
        };
        skybox.layout = panorama_layout(&skybox.gpu)?;
        // SAFETY: each handle is owned before the next fallible call; Drop frees.
        unsafe {
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
            skybox.pool = skybox.gpu.device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(1)
                    .pool_sizes(&sizes),
                None,
            )?;
            let layouts = [skybox.layout];
            skybox.set = skybox.gpu.device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(skybox.pool)
                    .set_layouts(&layouts),
            )?[0];
            let image = [vk::DescriptorImageInfo {
                sampler: vk::Sampler::null(),
                image_view: skybox.image.view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            let sampler = [vk::DescriptorImageInfo {
                sampler: skybox.sampler.handle,
                ..Default::default()
            }];
            skybox.gpu.device.update_descriptor_sets(
                &[
                    vk::WriteDescriptorSet::default()
                        .dst_set(skybox.set)
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                        .image_info(&image),
                    vk::WriteDescriptorSet::default()
                        .dst_set(skybox.set)
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::SAMPLER)
                        .image_info(&sampler),
                ],
                &[],
            );
        }
        Ok(skybox)
    }
}
impl Drop for Skybox {
    fn drop(&mut self) {
        // SAFETY: the renderer waits for idle before replacing or dropping it.
        unsafe {
            self.gpu.device.destroy_descriptor_pool(self.pool, None);
            self.gpu
                .device
                .destroy_descriptor_set_layout(self.layout, None);
        }
    }
}

pub(crate) struct SkyPipeline {
    pub handle: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    set_layout: vk::DescriptorSetLayout,
    gpu: Arc<Gpu>,
}
impl SkyPipeline {
    /// `panorama` selects the image pass (one texture set) over the gradient.
    pub fn new(gpu: Arc<Gpu>, pass: vk::RenderPass, panorama: bool) -> GpuResult<Self> {
        let vert = Shader::new(
            gpu.clone(),
            include_bytes!(concat!(env!("OUT_DIR"), "/sky.vert.spv")),
        )?;
        let frag = Shader::new(
            gpu.clone(),
            if panorama {
                include_bytes!(concat!(env!("OUT_DIR"), "/sky_panorama.frag.spv")).as_slice()
            } else {
                include_bytes!(concat!(env!("OUT_DIR"), "/sky.frag.spv")).as_slice()
            },
        )?;
        let mut pipeline = Self {
            handle: vk::Pipeline::null(),
            layout: vk::PipelineLayout::null(),
            set_layout: vk::DescriptorSetLayout::null(),
            gpu,
        };
        if panorama {
            pipeline.set_layout = panorama_layout(&pipeline.gpu)?;
        }
        // SAFETY: create-info arrays and shader modules live through creation;
        // partial handles are freed on error and by Drop.
        unsafe {
            let ranges = [vk::PushConstantRange {
                stage_flags: vk::ShaderStageFlags::FRAGMENT,
                offset: 0,
                size: std::mem::size_of::<SkyPush>() as u32,
            }];
            let set_layouts = [pipeline.set_layout];
            let sets: &[vk::DescriptorSetLayout] = if panorama { &set_layouts } else { &[] };
            pipeline.layout = pipeline.gpu.device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(sets)
                    .push_constant_ranges(&ranges),
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
            self.gpu
                .device
                .destroy_descriptor_set_layout(self.set_layout, None);
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
            panorama: None,
        };
        let pano = SkyPush::panorama(sky(Environment::Day), 1.5, true, false, 2.);
        assert_eq!((pano.zenith[3], pano.ground[3]), (1., 1.5));
        assert_eq!(pano.horizon[..2], [0., 2.], "LDR panorama: no exposure");
        let hdr = SkyPush::panorama(sky(Environment::Day), 0., false, true, -1.5);
        assert_eq!(
            (hdr.zenith[3], hdr.horizon[0], hdr.horizon[1]),
            (0., 1., -1.5)
        );
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
            let light = EnvironmentLight::preset(Environment::Day, strength);
            assert_eq!(EnvironmentLight::frame_colors(Some(light)), off);
        }
        for environment in Environment::ALL {
            let light = EnvironmentLight::preset(environment, 1.);
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
    #[test]
    fn panorama_light_averages_sky_horizon_and_ground_bands() {
        let (w, h) = (64u32, 32u32);
        let mut rgba = Vec::new();
        for y in 0..h {
            let color: [u8; 3] = if y < h / 2 {
                [40, 90, 200]
            } else {
                [120, 80, 40]
            };
            for _ in 0..w {
                rgba.extend_from_slice(&[color[0], color[1], color[2], 255]);
            }
        }
        let image = TextureImage {
            width: w,
            height: h,
            rgba: rgba.into(),
        };
        let [zenith, _, ground] = panorama_light_colors(&image).unwrap();
        let close = |a: [f32; 3], b: [u8; 3]| {
            a.iter()
                .zip(b)
                .all(|(x, y)| (x * 255. - f32::from(y)).abs() < 1.)
        };
        assert!(close(zenith, [40, 90, 200]), "{zenith:?}");
        assert!(close(ground, [120, 80, 40]), "{ground:?}");
        let light = EnvironmentLight {
            colors: panorama_light_colors(&image).unwrap(),
            strength: 1.,
        };
        assert!(light.valid());
        let broken = TextureImage {
            width: 4,
            height: 2,
            rgba: vec![0; 3].into(),
        };
        assert!(panorama_light_colors(&broken).is_none());
        let mut bad = light;
        bad.colors[0][0] = f32::NAN;
        assert!(!bad.valid());
    }
}
