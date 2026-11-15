# ADR-0012 — Native window and presentation lifecycle experiment

Status: EXPERIMENTAL, verified on the development RTX 5060 Ti, 2026-11-15.

Add ad-desktop because window/events/OS resource integration now have a real
responsibility separate from the renderer. winit 0.30.13 owns one native event
loop. ad-render-vulkan depends on raw-window-handle/ash-window, not winit or UI
types; it retains an Arc to the surface target until the Vulkan surface is gone.
No domain crate or product editing operation is introduced.

Use one in-flight frame, color-attachment clear and a red test mark. This proves
surface/device/queue/swapchain presentation and lifecycle, not mesh rasterization,
PBR, an asset viewport, UI backend selection or RT/PT. Native UI candidates in
ADR-0004 remain open; winit passing this experiment does not select egui.

The experiment requires VK_EXT_surface_maintenance1 and the device feature
VK_EXT_swapchain_maintenance1. Submission fences and presentation fences are both
waited before reuse/recreation/destruction. Presentation semaphores are indexed
by swapchain image. Unsupported devices get an explicit error; broader fallback
must be designed before treating this spike as a production compatibility floor.

## Overlay compatibility, measured

The initial BGRA sRGB swapchain passed startup but generated 14 validation errors
after minimize/restore: STORAGE/TRANSFER usage appeared below the application's
COLOR_ATTACHMENT-only request. Scoped isolation experiments left the binary
unchanged: disabling all implicit layers passed; disabling only NV_present still
failed; disabling only RTSS passed. Registry/loader evidence identified RTSS.
No global settings or registry entries were changed.

Prefer advertised B8G8R8A8_UNORM + SRGB_NONLINEAR instead. That format supports
RTSS's augmented usage on the target. A selection regression test failed with
the old preference and passed after changing it. Native debug and release smoke
then passed with normal implicit layers enabled. No validation IDs are filtered
or suppressed. A later PBR final pass must explicitly encode linear output for an
UNORM target; these display test colors are not a completed color pipeline.

PNG original is retained. A 64px derivative becomes the window icon, and the
multi-resolution ICO is embedded into the Windows EXE with version resources.
Product/app ID remains isolated from Legacy; no installer/associations are added.

Sources: Khronos Vulkan Guide, Swapchain Semaphore Reuse;
VkSwapchainPresentFenceInfoKHR/EXT reference; ash-window 0.13 safety contract;
winit 0.30.13 ApplicationHandler documentation. Dependency versions/licenses and
the one upstream miniz duplication exception are in DEPENDENCIES.md/deny.toml.
