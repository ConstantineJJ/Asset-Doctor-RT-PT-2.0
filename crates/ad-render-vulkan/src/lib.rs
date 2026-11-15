//! Isolated GPU experiment. No asset/document ownership or product editing.

pub mod context;
mod gpu;
pub mod lighting;
mod pipeline;
pub mod presentation;
pub mod renderer;
mod resources;
mod scene;
mod swapchain;
mod textures;
pub mod ui;
pub use gpu::SurfaceTarget;

#[derive(Debug, PartialEq, Eq)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

impl SurfaceSize {
    pub fn drawable(&self) -> bool {
        self.width > 0 && self.height > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimized_or_zero_width_surface_is_not_drawable() {
        assert!(
            !SurfaceSize {
                width: 1280,
                height: 0
            }
            .drawable()
        );
        assert!(
            !SurfaceSize {
                width: 0,
                height: 720
            }
            .drawable()
        );
        assert!(
            SurfaceSize {
                width: 1280,
                height: 720
            }
            .drawable()
        );
    }
}
