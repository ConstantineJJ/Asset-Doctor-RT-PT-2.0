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
    overlay: Option<Overlay>,
    view_projection: [[f32; 4]; 4],
    sample_textures: bool,
    eye: [f32; 3],
    lighting: ViewLighting,
    pub frames: u64,
    pub mesh_frames: u64,
    pub pbr_frames: u64,
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
            sample_textures: true,
            eye: [0., 0., 4.],
            lighting: ViewLighting::default(),
            overlay: None,
            view_projection: [
                [1., 0., 0., 0.],
                [0., 1., 0., 0.],
                [0., 0., 1., 0.],
                [0., 0., 0., 1.],
            ],
            frames: 0,
            mesh_frames: 0,
            pbr_frames: 0,
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
        result.recreate(size)?;
        Ok(result)
    }

    pub fn context(&self) -> Arc<Context> {
        self.gpu.surface.context.clone()
    }

    pub fn set_scene(&mut self, snapshot: &GeometrySnapshot) -> GpuResult<()> {
        let candidate = GpuScene::new(self.gpu.clone(), snapshot)?;
        self.wait_idle()?;
        self.scene = Some(candidate);
        eprintln!(
            "gpu_scene_published document={} revision={} vertices={} triangles={}",
            snapshot.document_id.0,
            snapshot.revision,
            snapshot.positions.len(),
            snapshot.indices.len() / 3
        );
        Ok(())
    }
    pub fn scene_tag(&self) -> Option<(DocumentId, u64)> {
        self.scene
            .as_ref()
            .map(|scene| (scene.document_id, scene.revision))
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
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
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
                device.cmd_bind_vertex_buffers(
                    self.command,
                    0,
                    &[
                        scene.vertices.handle,
                        scene.uvs.handle,
                        scene.colors.handle,
                        scene.normals.handle,
                    ],
                    &[0, 0, 0, 0],
                );
                device.cmd_bind_index_buffer(
                    self.command,
                    scene.indices.handle,
                    0,
                    vk::IndexType::UINT32,
                );
                for draw in &scene.draws {
                    let push = Push {
                        view_projection: self.view_projection,
                        base_color: draw.base_color,
                        eye_exposure: [
                            self.eye[0],
                            self.eye[1],
                            self.eye[2],
                            self.lighting.exposure_ev,
                        ],
                        material: [
                            draw.metallic_roughness[0],
                            draw.metallic_roughness[1],
                            draw.alpha_cutoff.unwrap_or(-1.),
                            0.,
                        ],
                        lighting: [
                            self.lighting.key,
                            self.lighting.fill,
                            self.lighting.flags(self.sample_textures, chain.encode_srgb),
                            0.,
                        ],
                    };
                    device.cmd_bind_descriptor_sets(
                        self.command,
                        vk::PipelineBindPoint::GRAPHICS,
                        pipeline.layout,
                        0,
                        &[scene.textures.sets[draw.texture_slot]],
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
