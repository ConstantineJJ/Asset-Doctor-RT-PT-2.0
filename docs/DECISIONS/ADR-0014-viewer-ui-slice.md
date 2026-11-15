# ADR-0014 — Native viewer UI integration experiment

Status: ACCEPTED for a bounded viewer shell experiment, 2026-11-15. The permanent
backend choice in ADR-0004 remains open; no full UI/accessibility parity claimed.

Use egui/egui-winit 0.36.2 and egui-ash-renderer 0.13.0 with ash 0.38, one native
event loop and the existing Vulkan render pass. No eframe/wgpu/WebView is added.
egui-winit default features are disabled (no clipboard, URL opening or persistence
integration); egui's default fonts are the only initial UI asset dependency.
The maintained ash renderer owns UI texture/descriptor/buffer details, replaceable
inside the Vulkan presentation boundary. UI state and actions stay in desktop.
Domain/formats acquire no UI/GPU dependency.

The complete slice is Open/Fit/tree/summary controls, loading/cancel/status,
read-only active scene hierarchy and selected-node details. Tree is hidden at
startup per REQ-TREE-001. Selection is presentation only and scoped to document ID;
new document clears selection, failed/cancelled import retains it. Node selection
does not yet highlight or edit geometry. No fake diagnostics/repair controls.
Source matrices, names and bytes remain untouched.

Flatten the active hierarchy iteratively once per accepted document, then draw
only visible rows. No recursive widget walk on untrusted hierarchy or geometry
counting per frame. Snapshot counts are labelled as expanded display geometry.
Summary explicitly describes current static/base-color compatibility.

Desktop routes input to egui first; consumed UI input cannot start viewport drags.
Pointer drags retain their initial owner until release/focus loss. Camera aspect
and Vulkan viewport/scissor use the remaining central rectangle, with logical to
physical DPI conversion and clipping. Compact windows hide optional side panels.

Renderer accepts a prepared UI frame (clipped paint primitives, pixels-per-point,
texture deltas and physical scene rectangle), not product controls or documents.
Wait the single frame before UI buffer/texture update. Retain UI texture ownership
across compatible swapchain recreation. A changed surface format currently fails
explicitly with a restart request; general color-mode recreation remains open.
Free texture deltas only after submitted work is retired. UI and mesh share one
submission and lifetime fences. No discarded acquired image on a UI update error.

Acceptance: headless egui layout/input tests + viewport clipping/DPI tests, current
import/worker tests, fmt/clippy/test/Windows build/dependency review. Native UI/GPU
verification needs the owner's permission to reopen a window after PC handback.
Owner explicitly renewed permission for a 20–30-second automatic window check;
release empty/import/rejected-replacement/lifecycle smoke passed with zero errors.
No computer input/capture was used. Owner's preceding manual
tests confirm current viewer functions and displayed models up to 1,568,489
triangles; their observed 142 FPS is not a benchmark or peak-memory proof.

Sources: [egui official integration docs](https://github.com/emilk/egui/tree/0.36.2)
(Context7, 0.36.2), [renderer compatibility and texture lifetime instructions](https://github.com/adrien-ben/egui-ash-renderer).
See the bounded
behavior audit in reference/SLICE_02_VIEWER_UI_AUDIT.md.
