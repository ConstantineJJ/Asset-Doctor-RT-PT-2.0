use crate::SurfaceSize;
use ash::vk;

/// Refuse zero-size surfaces before clamping to driver minimums.
pub fn extent(
    size: SurfaceSize,
    capabilities: &vk::SurfaceCapabilitiesKHR,
) -> Option<vk::Extent2D> {
    if !size.drawable() {
        return None;
    }
    let fixed = capabilities.current_extent;
    if fixed.width != u32::MAX {
        return (fixed.width > 0 && fixed.height > 0).then_some(fixed);
    }
    Some(vk::Extent2D {
        width: size.width.clamp(
            capabilities.min_image_extent.width,
            capabilities.max_image_extent.width,
        ),
        height: size.height.clamp(
            capabilities.min_image_extent.height,
            capabilities.max_image_extent.height,
        ),
    })
}

pub fn image_count(capabilities: &vk::SurfaceCapabilitiesKHR) -> u32 {
    let desired = capabilities.min_image_count.saturating_add(1);
    if capabilities.max_image_count == 0 {
        desired
    } else {
        desired.min(capabilities.max_image_count)
    }
}

pub fn surface_format(formats: &[vk::SurfaceFormatKHR]) -> Option<vk::SurfaceFormatKHR> {
    // RTSS can add STORAGE usage while recreating the swapchain. BGRA sRGB
    // cannot support that usage on the target GPU; UNORM avoids that conflict.
    // The later PBR pipeline must encode linear output for an UNORM target.
    formats
        .iter()
        .find(|f| {
            f.format == vk::Format::B8G8R8A8_UNORM
                && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .or(formats.first())
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps() -> vk::SurfaceCapabilitiesKHR {
        vk::SurfaceCapabilitiesKHR {
            min_image_count: 2,
            max_image_count: 3,
            current_extent: vk::Extent2D {
                width: u32::MAX,
                height: u32::MAX,
            },
            min_image_extent: vk::Extent2D {
                width: 16,
                height: 16,
            },
            max_image_extent: vk::Extent2D {
                width: 4096,
                height: 4096,
            },
            ..Default::default()
        }
    }

    #[test]
    fn minimized_window_does_not_create_swapchain_at_minimum_extent() {
        assert!(
            extent(
                SurfaceSize {
                    width: 0,
                    height: 0
                },
                &caps()
            )
            .is_none()
        );
    }

    #[test]
    fn variable_extent_clamps_to_driver_limits() {
        assert_eq!(
            extent(
                SurfaceSize {
                    width: 9999,
                    height: 8
                },
                &caps()
            ),
            Some(vk::Extent2D {
                width: 4096,
                height: 16
            })
        );
    }

    #[test]
    fn surface_fixed_extent_takes_precedence_over_requested_window_size() {
        let mut caps = caps();
        caps.current_extent = vk::Extent2D {
            width: 800,
            height: 600,
        };
        assert_eq!(
            extent(
                SurfaceSize {
                    width: 960,
                    height: 540
                },
                &caps
            ),
            Some(caps.current_extent)
        );
    }

    #[test]
    fn image_count_respects_finite_and_unbounded_maximum() {
        let mut caps = caps();
        assert_eq!(image_count(&caps), 3);
        caps.max_image_count = 2;
        assert_eq!(image_count(&caps), 2);
        caps.max_image_count = 0;
        assert_eq!(image_count(&caps), 3);
    }

    #[test]
    fn presentation_prefers_overlay_compatible_unorm_without_storage_on_srgb() {
        let formats = [
            vk::SurfaceFormatKHR {
                format: vk::Format::B8G8R8A8_SRGB,
                color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
            },
            vk::SurfaceFormatKHR {
                format: vk::Format::B8G8R8A8_UNORM,
                color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
            },
        ];
        assert_eq!(
            surface_format(&formats).unwrap().format,
            vk::Format::B8G8R8A8_UNORM
        );
        assert!(surface_format(&[]).is_none());
    }
}
