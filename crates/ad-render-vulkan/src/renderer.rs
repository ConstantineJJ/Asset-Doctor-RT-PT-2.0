use crate::frame::{FrameBinding, FrameData};
use crate::lighting::{ShadingMode, ViewLighting};
use crate::ui::{Overlay, UiFrame, ViewportRect};
use crate::{
    SurfaceSize,
    context::{Context, GpuResult},
    gpu::{Gpu, SurfaceTarget},
    swapchain::Swapchain,
};
use crate::{pipeline::Push, scene::GpuScene};
use ad_document::{DocumentId, GeometrySnapshot};
use ash::vk;
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq)]
pub enum FrameResult {
    Presented,
    Recreate,
    Suspended,
}

/// Injected failure point of `Renderer::stage_scene` for native smoke tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneFault {
    Upload,
    Wait,
}
impl std::str::FromStr for SceneFault {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "upload" => Ok(Self::Upload),
            "wait" => Ok(Self::Wait),
            _ => Err(format!("Unknown GPU fault stage: {value}")),
        }
    }
}

/// A fully uploaded scene after the idle wait. `activate` cannot fail: it
/// replaces the current scene and drops the old one, whose GPU use ended.
pub struct StagedScene<'a> {
    renderer: &'a mut Renderer,
    scene: GpuScene,
    vertices: usize,
    triangles: usize,
    stage_ms: u128,
}
impl StagedScene<'_> {
    pub fn tag(&self) -> (DocumentId, u64) {
        (self.scene.document_id, self.scene.revision)
    }
    pub fn activate(self) {
        let (document, revision) = self.tag();
        let bytes = self.scene.geometry_bytes();
        self.renderer.scene = Some(self.scene);
        eprintln!(
            "gpu_scene_published document={} revision={} vertices={} triangles={} stage_ms={} geometry_bytes={}",
            document.0, revision, self.vertices, self.triangles, self.stage_ms, bytes
        );
    }
}

/// One in-flight frame for the lifecycle experiment. No asset/document ownership.
pub struct Renderer {
    swapchain: Option<Swapchain>,
    gpu: Arc<Gpu>,
    pool: vk::CommandPool,
    command: vk::CommandBuffer,
    acquired: vk::Semaphore,
    submitted: vk::Fence,
    presented: vk::Fence,
    presentation_pending: bool,
    scene: Option<GpuScene>,
    scene_fault: Option<SceneFault>,
    overlay: Option<Overlay>,
    frame: Option<FrameBinding>,
    view_projection: [[f32; 4]; 4],
    /// ADR-0040 floor grid; None hides it.
    grid: Option<crate::grid::GridSettings>,
    grid_frames: u64,
    /// ADR-0040 environment sky; None or a preset without colours draws none.
    sky: Option<crate::sky::SkySettings>,
    sky_frames: u64,
    /// ADR-0040 environment light (PBR only); None = off.
    environment_light: Option<crate::sky::EnvironmentLight>,
    environment_light_frames: u64,
    /// ADR-0040 owner panorama; replaced only after waiting for idle.
    skybox: Option<crate::sky::Skybox>,
    panorama_frames: u64,
    sample_textures: bool,
    sample_mr_maps: bool,
    sample_normal_maps: bool,
    sample_occlusion_maps: bool,
    sample_emission: bool,
    eye: [f32; 3],
    lighting: ViewLighting,
    pub frames: u64,
    pub mesh_frames: u64,
    pub pbr_frames: u64,
    pub mr_frames: u64,
    pub normal_frames: u64,
    pub occlusion_frames: u64,
    pub emission_frames: u64,
    pub recreations: u64,
}

impl Renderer {
    pub fn new(
        target: Arc<dyn SurfaceTarget>,
        size: SurfaceSize,
        validation: bool,
    ) -> GpuResult<Self> {
        let gpu = Gpu::new(target, validation)?;
        let mut result = Self {
            swapchain: None,
            gpu,
            pool: vk::CommandPool::null(),
            command: vk::CommandBuffer::null(),
            acquired: vk::Semaphore::null(),
            submitted: vk::Fence::null(),
            presented: vk::Fence::null(),
            presentation_pending: false,
            scene: None,
            scene_fault: None,
            sample_textures: true,
            sample_mr_maps: true,
            sample_normal_maps: true,
            sample_occlusion_maps: true,
            sample_emission: true,
            eye: [0., 0., 4.],
            lighting: ViewLighting::default(),
            overlay: None,
            frame: None,
            view_projection: [
                [1., 0., 0., 0.],
                [0., 1., 0., 0.],
                [0., 0., 1., 0.],
                [0., 0., 0., 1.],
            ],
            grid: None,
            grid_frames: 0,
            sky: None,
            sky_frames: 0,
            environment_light: None,
            environment_light_frames: 0,
            skybox: None,
            panorama_frames: 0,
            frames: 0,
            mesh_frames: 0,
            pbr_frames: 0,
            mr_frames: 0,
            normal_frames: 0,
            occlusion_frames: 0,
            emission_frames: 0,
            recreations: 0,
        };
        // SAFETY: Live device, valid queue family. Self owns every successful
        // allocation immediately, including cleanup on partial initialization.
        unsafe {
            let device = &result.gpu.device;
            result.pool = device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(result.gpu.family)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )?;
            result.command = device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(result.pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )?[0];
            result.acquired = device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;
            result.submitted = device.create_fence(
                &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                None,
            )?;
            result.presented = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
        }
        result.frame = Some(FrameBinding::new(result.gpu.clone())?);
        result.recreate(size)?;
        Ok(result)
    }

    pub fn context(&self) -> Arc<Context> {
        self.gpu.surface.context.clone()
    }

    /// Uploads and idles in one step: stage + activate.
    pub fn set_scene(&mut self, snapshot: &GeometrySnapshot) -> GpuResult<()> {
        self.stage_scene(snapshot)?.activate();
        Ok(())
    }
    /// Every fallible GPU step of a scene replacement (ADR-0031): candidate
    /// upload, then an idle wait so the current scene is no longer in use.
    /// The returned stage borrows the renderer mutably, so no frame can be
    /// submitted before `activate`. Dropping it frees only the candidate.
    pub fn stage_scene(&mut self, snapshot: &GeometrySnapshot) -> GpuResult<StagedScene<'_>> {
        let started = std::time::Instant::now();
        let fault = self.scene_fault.take();
        let candidate = GpuScene::new(self.gpu.clone(), snapshot)?;
        if fault == Some(SceneFault::Upload) {
            return Err("Injected GPU upload failure; current GPU scene preserved".into());
        }
        self.wait_idle()?;
        if fault == Some(SceneFault::Wait) {
            return Err("Injected GPU idle-wait failure; current GPU scene preserved".into());
        }
        Ok(StagedScene {
            renderer: self,
            scene: candidate,
            vertices: snapshot.positions.len(),
            triangles: snapshot.indices.len() / 3,
            stage_ms: started.elapsed().as_millis(),
        })
    }
    /// Smoke/test seam: the next `stage_scene` fails at the given step after
    /// the candidate was really uploaded. One-shot; never set by product UI.
    pub fn inject_scene_fault(&mut self, fault: Option<SceneFault>) {
        self.scene_fault = fault;
    }
    pub fn scene_tag(&self) -> Option<(DocumentId, u64)> {
        self.scene
            .as_ref()
            .map(|scene| (scene.document_id, scene.revision))
    }
    /// ADR-0040: show the floor grid at `grid`, or hide it with None.
    pub fn set_grid(&mut self, grid: Option<crate::grid::GridSettings>) {
        self.grid = grid.filter(|g| {
            [g.floor_y, g.center[0], g.center[1], g.extent, g.spacing]
                .iter()
                .all(|v| v.is_finite())
                && g.extent > 0.
                && g.spacing > 0.
        });
    }
    /// ADR-0040: environment sky for the next frames (view only).
    pub fn set_sky(&mut self, sky: Option<crate::sky::SkySettings>) {
        self.sky = sky.filter(|s| {
            s.inverse_view_projection
                .iter()
                .flatten()
                .all(|v| v.is_finite())
        });
    }
    /// ADR-0040: environment light for PBR shading; lamps stay unchanged.
    pub fn set_environment_light(&mut self, light: Option<crate::sky::EnvironmentLight>) {
        self.environment_light = light.filter(|l| l.valid() && l.strength > 0.);
    }
    /// Presented PBR frames lit by the environment (smoke evidence).
    pub fn environment_light_frames(&self) -> u64 {
        self.environment_light_frames
    }
    /// ADR-0040: uploads the owner's panorama (or removes it with None). Waits
    /// for idle first; on failure the previous panorama is already gone and
    /// the preset sky is drawn.
    pub fn set_skybox_image(&mut self, image: Option<&ad_document::TextureImage>) -> GpuResult<()> {
        self.wait_idle()?;
        self.skybox = None;
        if let Some(image) = image {
            self.skybox = Some(crate::sky::Skybox::new(self.gpu.clone(), image)?);
        }
        Ok(())
    }
    pub fn has_skybox(&self) -> bool {
        self.skybox.is_some()
    }
    /// Frames in which the panorama was recorded (smoke evidence).
    pub fn panorama_frames(&self) -> u64 {
        self.panorama_frames
    }
    /// Frames in which the sky was recorded (smoke evidence).
    pub fn sky_frames(&self) -> u64 {
        self.sky_frames
    }
    /// Frames in which the floor grid was recorded (smoke evidence).
    pub fn grid_frames(&self) -> u64 {
        self.grid_frames
    }
    pub fn set_camera(&mut self, matrix: [[f32; 4]; 4], eye: [f32; 3]) -> GpuResult<()> {
        if matrix
            .iter()
            .flatten()
            .chain(eye.iter())
            .any(|v| !v.is_finite())
        {
            return Err("Nonfinite camera matrix/eye refused".into());
        }
        self.view_projection = matrix;
        self.eye = eye;
        Ok(())
    }
    pub fn set_lighting(&mut self, lighting: ViewLighting) -> GpuResult<()> {
        if !lighting.valid() {
            return Err("Invalid view lighting settings refused".into());
        }
        self.lighting = lighting;
        Ok(())
    }
    pub fn set_textures_enabled(&mut self, enabled: bool) {
        self.sample_textures = enabled;
    }
    pub fn set_mr_maps_enabled(&mut self, enabled: bool) {
        self.sample_mr_maps = enabled;
    }
    pub fn set_normal_maps_enabled(&mut self, enabled: bool) {
        self.sample_normal_maps = enabled;
    }
    pub fn set_occlusion_maps_enabled(&mut self, enabled: bool) {
        self.sample_occlusion_maps = enabled;
    }
    pub fn set_emission_enabled(&mut self, enabled: bool) {
        self.sample_emission = enabled;
    }

    fn wait_idle(&mut self) -> GpuResult<()> {
        // SAFETY: Fence is reset only immediately before a submitted presentation;
        // maintenance1 fences cover presentation payload lifetimes as well as GPU work.
        unsafe {
            if self.presentation_pending {
                self.gpu
                    .device
                    .wait_for_fences(&[self.presented], true, u64::MAX)?;
                self.gpu.device.reset_fences(&[self.presented])?;
                self.presentation_pending = false;
            }
            self.gpu.device.device_wait_idle()?;
        }
        Ok(())
    }

    pub fn recreate(&mut self, size: SurfaceSize) -> GpuResult<()> {
        self.wait_idle()?;
        self.swapchain = None;
        if size.drawable() {
            self.swapchain = Swapchain::new(self.gpu.clone(), size)?;
            if let (Some(overlay), Some(chain)) = (&self.overlay, &self.swapchain)
                && overlay.format != chain.format
            {
                return Err(
                    "Surface format changed; restart the viewer to rebuild UI color resources"
                        .into(),
                );
            }
        }
        self.recreations += 1;
        Ok(())
    }

    pub fn draw(&mut self) -> GpuResult<FrameResult> {
        self.draw_frame(None)
    }
    pub fn draw_ui(&mut self, frame: &mut UiFrame) -> GpuResult<FrameResult> {
        self.draw_frame(Some(frame))
    }
    pub fn drawable(&self) -> bool {
        self.swapchain.is_some()
    }

    fn draw_frame(&mut self, mut ui: Option<&mut UiFrame>) -> GpuResult<FrameResult> {
        let Some(chain) = self.swapchain.as_ref() else {
            return Ok(FrameResult::Suspended);
        };
        // SAFETY: One in-flight command buffer. Submit and presentation fences
        // are waited before reuse; image-specific semaphores cover image acquire.
        // Render pass transitions discard old color then finish in PRESENT layout.
        unsafe {
            let device = &self.gpu.device;
            device.wait_for_fences(&[self.submitted], true, u64::MAX)?;
            if self.presentation_pending {
                device.wait_for_fences(&[self.presented], true, u64::MAX)?;
                device.reset_fences(&[self.presented])?;
                self.presentation_pending = false;
            }
            if let Some(frame) = ui.as_deref_mut() {
                // No frame-data writes happen from the UI or camera setters.
                if self.overlay.is_none() {
                    self.overlay = Some(Overlay::new(
                        self.gpu.clone(),
                        chain.pass,
                        chain.format,
                        !chain.encode_srgb,
                    )?);
                }
                self.overlay
                    .as_mut()
                    .expect("UI painter initialized")
                    .update(frame, self.pool)?;
            }
            let sky_light = crate::sky::EnvironmentLight::frame_colors(self.environment_light);
            self.frame
                .as_ref()
                .expect("Frame binding initialized")
                .update(&FrameData {
                    view_projection: self.view_projection,
                    eye_exposure: [
                        self.eye[0],
                        self.eye[1],
                        self.eye[2],
                        self.lighting.exposure_ev,
                    ],
                    lighting: [self.lighting.key, self.lighting.fill, 0., 0.],
                    sky_zenith: sky_light[0],
                    sky_horizon: sky_light[1],
                    sky_ground: sky_light[2],
                    light_direction: [
                        self.lighting.direction[0],
                        self.lighting.direction[1],
                        self.lighting.direction[2],
                        0.,
                    ],
                    light_color: if self.lighting.enabled {
                        let [r, g, b] = self.lighting.color.map(crate::sky::srgb_to_linear);
                        [r, g, b, 1.]
                    } else {
                        [0., 0., 0., 0.]
                    },
                })?;
            let (image, suboptimal) = match chain.loader.acquire_next_image(
                chain.handle,
                u64::MAX,
                self.acquired,
                vk::Fence::null(),
            ) {
                Ok(result) => result,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(FrameResult::Recreate),
                Err(error) => return Err(error.into()),
            };
            device.reset_command_buffer(self.command, vk::CommandBufferResetFlags::empty())?;
            device.begin_command_buffer(
                self.command,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            let area = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: chain.extent,
            };
            let clear = [
                vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.018, 0.022, 0.03, 1.0],
                    },
                },
                vk::ClearValue {
                    // Reversed-Z: 0 is the far end (see crate::REVERSED_Z).
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 0.0,
                        stencil: 0,
                    },
                },
            ];
            device.cmd_begin_render_pass(
                self.command,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(chain.pass)
                    .framebuffer(chain.framebuffers[image as usize])
                    .render_area(area)
                    .clear_values(&clear),
                vk::SubpassContents::INLINE,
            );
            let scene_rect = if let Some(frame) = ui.as_deref() {
                frame.scene.and_then(|r| {
                    r.clipped(&SurfaceSize {
                        width: chain.extent.width,
                        height: chain.extent.height,
                    })
                })
            } else {
                Some(ViewportRect {
                    x: 0,
                    y: 0,
                    width: chain.extent.width,
                    height: chain.extent.height,
                })
            };
            if let (Some(scene), Some(rect)) = (self.scene.as_ref(), scene_rect) {
                let pipeline = chain
                    .pipeline
                    .as_ref()
                    .expect("Renderable swapchain owns mesh pipeline");
                device.cmd_bind_pipeline(
                    self.command,
                    vk::PipelineBindPoint::GRAPHICS,
                    pipeline.handle,
                );
                device.cmd_set_viewport(
                    self.command,
                    0,
                    &[vk::Viewport {
                        x: rect.x as f32,
                        y: rect.y as f32,
                        width: rect.width as f32,
                        height: rect.height as f32,
                        min_depth: 0.0,
                        max_depth: 1.0,
                    }],
                );
                device.cmd_set_scissor(
                    self.command,
                    0,
                    &[vk::Rect2D {
                        offset: vk::Offset2D {
                            x: rect.x as i32,
                            y: rect.y as i32,
                        },
                        extent: vk::Extent2D {
                            width: rect.width,
                            height: rect.height,
                        },
                    }],
                );
                // ADR-0040: sky behind everything, then the mesh pipeline again.
                let panorama = self.sky.and_then(|sky| sky.panorama.map(|yaw| (sky, yaw)));
                if let (Some((settings, yaw)), Some(skybox), Some(sky)) =
                    (panorama, self.skybox.as_ref(), chain.sky_panorama.as_ref())
                {
                    let push = crate::sky::SkyPush::panorama(settings, yaw, !chain.encode_srgb);
                    device.cmd_bind_pipeline(
                        self.command,
                        vk::PipelineBindPoint::GRAPHICS,
                        sky.handle,
                    );
                    device.cmd_bind_descriptor_sets(
                        self.command,
                        vk::PipelineBindPoint::GRAPHICS,
                        sky.layout,
                        0,
                        &[skybox.set],
                        &[],
                    );
                    let bytes = std::slice::from_raw_parts(
                        (&push as *const crate::sky::SkyPush).cast::<u8>(),
                        std::mem::size_of::<crate::sky::SkyPush>(),
                    );
                    device.cmd_push_constants(
                        self.command,
                        sky.layout,
                        vk::ShaderStageFlags::FRAGMENT,
                        0,
                        bytes,
                    );
                    device.cmd_draw(self.command, 3, 1, 0, 0);
                    self.panorama_frames += 1;
                    device.cmd_bind_pipeline(
                        self.command,
                        vk::PipelineBindPoint::GRAPHICS,
                        pipeline.handle,
                    );
                } else if let (Some(push), Some(sky)) = (
                    self.sky
                        .and_then(|sky| crate::sky::SkyPush::new(sky, !chain.encode_srgb)),
                    chain.sky.as_ref(),
                ) {
                    device.cmd_bind_pipeline(
                        self.command,
                        vk::PipelineBindPoint::GRAPHICS,
                        sky.handle,
                    );
                    let bytes = std::slice::from_raw_parts(
                        (&push as *const crate::sky::SkyPush).cast::<u8>(),
                        std::mem::size_of::<crate::sky::SkyPush>(),
                    );
                    device.cmd_push_constants(
                        self.command,
                        sky.layout,
                        vk::ShaderStageFlags::FRAGMENT,
                        0,
                        bytes,
                    );
                    device.cmd_draw(self.command, 3, 1, 0, 0);
                    self.sky_frames += 1;
                    device.cmd_bind_pipeline(
                        self.command,
                        vk::PipelineBindPoint::GRAPHICS,
                        pipeline.handle,
                    );
                }
                device.cmd_bind_vertex_buffers(
                    self.command,
                    0,
                    &[
                        scene.vertices.handle,
                        scene.uvs.handle,
                        scene.colors.handle,
                        scene.normals.handle,
                        scene.metallic_roughness_uvs.handle,
                        scene.normal_uvs.handle,
                        scene.tangents.handle,
                        scene.occlusion_uvs.handle,
                        scene.emissive_uvs.handle,
                    ],
                    &[0, 0, 0, 0, 0, 0, 0, 0, 0],
                );
                device.cmd_bind_index_buffer(
                    self.command,
                    scene.indices.handle,
                    0,
                    vk::IndexType::UINT32,
                );
                for draw in &scene.draws {
                    let push = Push {
                        base_color: draw.base_color,
                        material: [
                            draw.metallic_roughness[0],
                            draw.metallic_roughness[1],
                            draw.alpha_cutoff.unwrap_or(-1.),
                            draw.normal_scale,
                        ],
                        view: [
                            self.lighting.flags(
                                self.sample_textures,
                                self.sample_mr_maps,
                                self.sample_normal_maps && draw.has_normal_map,
                                self.sample_occlusion_maps && draw.has_occlusion_map,
                                self.sample_emission && draw.has_emission,
                                chain.encode_srgb,
                            ),
                            draw.occlusion_strength,
                            0.,
                            0.,
                        ],
                        emissive: [
                            draw.emissive_factor[0],
                            draw.emissive_factor[1],
                            draw.emissive_factor[2],
                            0.,
                        ],
                    };
                    device.cmd_bind_descriptor_sets(
                        self.command,
                        vk::PipelineBindPoint::GRAPHICS,
                        pipeline.layout,
                        0,
                        &[
                            self.frame.as_ref().expect("Frame binding initialized").set,
                            scene.textures.sets[draw.texture_slot],
                        ],
                        &[],
                    );
                    let bytes = std::slice::from_raw_parts(
                        (&push as *const Push).cast::<u8>(),
                        std::mem::size_of::<Push>(),
                    );
                    device.cmd_push_constants(
                        self.command,
                        pipeline.layout,
                        vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        0,
                        bytes,
                    );
                    device.cmd_draw_indexed(
                        self.command,
                        draw.index_count,
                        1,
                        draw.index_start,
                        0,
                        0,
                    );
                }
                // ADR-0040: floor grid after the model, depth-tested only.
                if let (Some(grid), Some(pipeline)) = (self.grid, chain.grid.as_ref()) {
                    let push = crate::grid::GridPush::new(self.view_projection, grid);
                    device.cmd_bind_pipeline(
                        self.command,
                        vk::PipelineBindPoint::GRAPHICS,
                        pipeline.handle,
                    );
                    let bytes = std::slice::from_raw_parts(
                        (&push as *const crate::grid::GridPush).cast::<u8>(),
                        std::mem::size_of::<crate::grid::GridPush>(),
                    );
                    device.cmd_push_constants(
                        self.command,
                        pipeline.layout,
                        vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        0,
                        bytes,
                    );
                    device.cmd_draw(self.command, 6, 1, 0, 0);
                    self.grid_frames += 1;
                }
            } else if ui.is_none() {
                // Visible clear marker only until a real scene has been imported.
                let span = chain.extent.width.min(chain.extent.height) / 4;
                let thickness = (span / 4).max(1);
                let cx = chain.extent.width / 2;
                let cy = chain.extent.height / 2;
                let rects = [
                    vk::ClearRect {
                        rect: vk::Rect2D {
                            offset: vk::Offset2D {
                                x: (cx - span / 2) as i32,
                                y: (cy - thickness / 2) as i32,
                            },
                            extent: vk::Extent2D {
                                width: span,
                                height: thickness,
                            },
                        },
                        base_array_layer: 0,
                        layer_count: 1,
                    },
                    vk::ClearRect {
                        rect: vk::Rect2D {
                            offset: vk::Offset2D {
                                x: (cx - thickness / 2) as i32,
                                y: (cy - span / 2) as i32,
                            },
                            extent: vk::Extent2D {
                                width: thickness,
                                height: span,
                            },
                        },
                        base_array_layer: 0,
                        layer_count: 1,
                    },
                ];
                let mark = [vk::ClearAttachment::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .color_attachment(0)
                    .clear_value(vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.9, 0.015, 0.035, 1.0],
                        },
                    })];
                device.cmd_clear_attachments(self.command, &mark, &rects);
            }
            if let Some(frame) = ui {
                self.overlay
                    .as_mut()
                    .expect("UI painter initialized")
                    .draw(self.command, chain.extent, frame)?;
            }
            device.cmd_end_render_pass(self.command);
            device.end_command_buffer(self.command)?;
            let waits = [self.acquired];
            let stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let commands = [self.command];
            let signals = [chain.finished[image as usize]];
            let submit = [vk::SubmitInfo::default()
                .wait_semaphores(&waits)
                .wait_dst_stage_mask(&stages)
                .command_buffers(&commands)
                .signal_semaphores(&signals)];
            // Reset only after acquire/record succeeded, avoiding an unsignaled
            // fence deadlock when an outdated surface skips frame submission.
            device.reset_fences(&[self.submitted])?;
            device.queue_submit(self.gpu.queue, &submit, self.submitted)?;
            let chains = [chain.handle];
            let images = [image];
            let fences = [self.presented];
            let mut fence_info = vk::SwapchainPresentFenceInfoEXT::default().fences(&fences);
            let present = vk::PresentInfoKHR::default()
                .wait_semaphores(&signals)
                .swapchains(&chains)
                .image_indices(&images)
                .push_next(&mut fence_info);
            let result = chain.loader.queue_present(self.gpu.queue, &present);
            match result {
                Ok(changed) => {
                    self.presentation_pending = true;
                    self.frames += 1;
                    if self.scene.is_some() {
                        self.mesh_frames += 1;
                        if self.lighting.mode == ShadingMode::DirectPbr {
                            self.pbr_frames += 1;
                            if self.environment_light.is_some() {
                                self.environment_light_frames += 1;
                            }
                            if self.sample_emission
                                && self.scene.as_ref().is_some_and(|scene| scene.has_emission)
                            {
                                self.emission_frames += 1;
                            }
                            if self.sample_occlusion_maps
                                && self
                                    .scene
                                    .as_ref()
                                    .is_some_and(|scene| scene.has_occlusion_maps)
                            {
                                self.occlusion_frames += 1;
                            }
                            if self.sample_normal_maps
                                && self
                                    .scene
                                    .as_ref()
                                    .is_some_and(|scene| scene.has_normal_maps)
                            {
                                self.normal_frames += 1;
                            }
                            if self.sample_mr_maps
                                && self.scene.as_ref().is_some_and(|scene| scene.has_mr_maps)
                            {
                                self.mr_frames += 1;
                            }
                        }
                    }
                    if suboptimal || changed {
                        Ok(FrameResult::Recreate)
                    } else {
                        Ok(FrameResult::Presented)
                    }
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.presentation_pending = true;
                    Ok(FrameResult::Recreate)
                }
                Err(error) => Err(error.into()),
            }
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        if let Err(error) = self.wait_idle() {
            eprintln!("GPU shutdown wait: {error}");
        }
        self.overlay = None;
        self.swapchain = None;
        self.scene = None;
        self.frame = None;
        // SAFETY: All queued work and presentation payloads were retired above;
        // on device loss Vulkan allows destruction. Partial-init handles may be null.
        unsafe {
            self.gpu.device.destroy_command_pool(self.pool, None);
            self.gpu.device.destroy_fence(self.submitted, None);
            self.gpu.device.destroy_fence(self.presented, None);
            self.gpu.device.destroy_semaphore(self.acquired, None);
        }
        eprintln!(
            "renderer_shutdown frames={} recreations={}",
            self.frames, self.recreations
        );
    }
}

#[cfg(test)]
mod tests {
    use super::SceneFault;

    #[test]
    fn scene_fault_names_are_explicit_and_unknown_names_refuse() {
        assert_eq!("upload".parse(), Ok(SceneFault::Upload));
        assert_eq!("wait".parse(), Ok(SceneFault::Wait));
        for bad in ["", "Upload", "none", "device-lost"] {
            assert!(bad.parse::<SceneFault>().is_err());
        }
    }
}
