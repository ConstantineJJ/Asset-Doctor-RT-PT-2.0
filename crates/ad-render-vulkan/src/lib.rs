//! Isolated GPU experiment. No asset/document ownership or product editing.

pub mod context;
mod frame;
mod gpu;
pub mod grid;
pub mod ibl;
pub mod lighting;
mod pipeline;
pub mod presentation;
pub mod renderer;
mod resources;
mod scene;
pub mod shadow;
pub mod sky;
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

/// Depth convention of every scene pass: callers supply a view-projection
/// matrix mapping the near plane to depth 1 and the far plane to 0 (reversed-Z
/// on a D32 float buffer); depth clears to 0 and the GREATER fragment wins.
pub const REVERSED_Z: bool = true;

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
