//! Paint-only UI boundary; no product widgets, jobs or document ownership.
use crate::SurfaceSize;
use crate::{context::GpuResult, gpu::Gpu};
use ash::vk;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl ViewportRect {
    pub fn clipped(self, surface: &SurfaceSize) -> Option<Self> {
        let right = self.x.saturating_add(self.width).min(surface.width);
        let bottom = self.y.saturating_add(self.height).min(surface.height);
        if right <= self.x || bottom <= self.y {
            return None;
        }
        Some(Self {
            width: right - self.x,
            height: bottom - self.y,
            ..self
        })
    }
    pub fn from_logical(
        min: [f32; 2],
        max: [f32; 2],
        scale: f32,
        surface: &SurfaceSize,
    ) -> Option<Self> {
        if !scale.is_finite()
            || scale <= 0.
            || !min.iter().chain(&max).all(|v| v.is_finite())
            || max[0] <= min[0]
            || max[1] <= min[1]
        {
            return None;
        }
        let x = (min[0] * scale).floor().clamp(0., surface.width as f32) as u32;
        let y = (min[1] * scale).floor().clamp(0., surface.height as f32) as u32;
        let right = (max[0] * scale).ceil().clamp(0., surface.width as f32) as u32;
        let bottom = (max[1] * scale).ceil().clamp(0., surface.height as f32) as u32;
        (right > x && bottom > y).then_some(Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }
}

pub struct UiFrame {
    pub primitives: Vec<egui::ClippedPrimitive>,
    pub textures: egui::TexturesDelta,
    pub pixels_per_point: f32,
    pub scene: Option<ViewportRect>,
}
impl Drop for UiFrame {
    fn drop(&mut self) {
        // Fatal GPU errors or zero-size frames may abandon an output deliberately.
        // Texture deltas must not panic during error unwinding (egui 0.36).
        self.textures.clear();
    }
}

pub(crate) struct Overlay {
    painter: egui_ash_renderer::Renderer<egui_ash_renderer::allocator::DefaultAllocator>,
    pub format: vk::Format,
    free: Vec<egui::TextureId>,
    gpu: Arc<Gpu>,
}
impl Overlay {
    pub fn new(
        gpu: Arc<Gpu>,
        pass: vk::RenderPass,
        format: vk::Format,
        srgb: bool,
    ) -> GpuResult<Self> {
        let painter = egui_ash_renderer::Renderer::with_default_allocator(
            &gpu.surface.context.instance,
            gpu.physical,
            gpu.device.clone(),
            egui_ash_renderer::RenderMode::RenderPass(pass),
            egui_ash_renderer::Options {
                in_flight_frames: 1,
                srgb_framebuffer: srgb,
                ..Default::default()
            },
        )?;
        Ok(Self {
            painter,
            format,
            free: Vec::new(),
            gpu,
        })
    }
    pub fn update(&mut self, frame: &mut UiFrame, pool: vk::CommandPool) -> GpuResult<()> {
        if !frame.pixels_per_point.is_finite() || frame.pixels_per_point <= 0. {
            return Err("Invalid UI display scale".into());
        }
        // Renderer retired the previous submit before texture deletion/update.
        for id in self.free.drain(..) {
            self.painter.free_texture(id)?;
        }
        for (id, deltas) in frame.textures.set.drain() {
            for delta in deltas {
                self.painter.set_texture(self.gpu.queue, pool, id, &delta)?;
            }
        }
        self.free.extend(frame.textures.free.drain());
        Ok(())
    }
    pub fn draw(
        &mut self,
        command: vk::CommandBuffer,
        extent: vk::Extent2D,
        frame: &mut UiFrame,
    ) -> GpuResult<()> {
        let bounds = egui::Rect::from_min_max(
            egui::Pos2::ZERO,
            egui::pos2(
                extent.width as f32 / frame.pixels_per_point,
                extent.height as f32 / frame.pixels_per_point,
            ),
        );
        for p in &mut frame.primitives {
            p.clip_rect = p.clip_rect.intersect(bounds);
        }
        frame.primitives.retain(|p| p.clip_rect.is_positive());
        self.painter
            .cmd_draw(command, extent, frame.pixels_per_point, &frame.primitives)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_rectangle_respects_dpi_panels_and_surface_clipping() {
        let size = SurfaceSize {
            width: 1200,
            height: 800,
        };
        assert_eq!(
            ViewportRect::from_logical([160.25, 40.], [700., 380.], 2., &size),
            Some(ViewportRect {
                x: 320,
                y: 80,
                width: 880,
                height: 680
            })
        );
        assert_eq!(
            ViewportRect::from_logical([-10., -2.], [10., 20.], 1., &size),
            Some(ViewportRect {
                x: 0,
                y: 0,
                width: 10,
                height: 20
            })
        );
        for (min, max, scale) in [
            ([10., 10.], [0., 0.], 1.),
            ([0., 0.], [10., 10.], 0.),
            ([f32::NAN, 0.], [10., 10.], 1.),
            ([1300., 0.], [1400., 10.], 1.),
        ] {
            assert_eq!(ViewportRect::from_logical(min, max, scale, &size), None);
        }
        assert_eq!(
            ViewportRect {
                x: 1100,
                y: 750,
                width: u32::MAX,
                height: 200
            }
            .clipped(&size),
            Some(ViewportRect {
                x: 1100,
                y: 750,
                width: 100,
                height: 50
            })
        );
    }
}
