# Native foundation verification — 2026-11-15

## Implemented

New Windows executable `target/release/AssetDoctorRTPT.exe`, built from independent
Rust source. Native window + ash Vulkan logical device, graphics/present queue,
swapchain, render pass, clear-frame marker, resize/minimize/restore and orderly
close. Owner icon is embedded into the executable and applied to the window.
No asset data or Legacy implementation participates in the GPU loop.

## Environment actually checked

- Rust 1.100.0 x86_64-pc-windows-msvc; fmt/clippy installed.
- Visual Studio Build Tools 2026 at E:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools.
- Windows SDK 10.0.26100.9457.
- LunarG Vulkan SDK 1.4.363.1 at E:\VulkanSDK\1.4.363.1; registered Khronos validation layer loaded successfully.
- Actual GPU: NVIDIA GeForce RTX 5060 Ti; driver exposes Vulkan 1.4.357, raw driver
  version 2604892160; graphics/present family 0. These driver/runtime versions
  are different facts from the SDK version.
- Surface and swapchain maintenance features available; presentation fences enabled.

## Current evidence

| Check | Result |
|---|---|
| cargo fmt --all --check | PASS |
| cargo clippy --workspace --all-targets --locked -- -D warnings | PASS |
| cargo test --workspace --locked | PASS, 6 tests |
| cargo build --workspace --release --locked | PASS, MSVC |
| cargo deny check | advisories, bans, licenses, sources PASS |
| Probe with --require-validation | PASS, validation_enabled=true |
| Debug native --smoke, normal implicit layers | PASS, 15 presents, resize/minimize/restore/WM_CLOSE, 0 validation errors |
| Release native --smoke, normal implicit layers | PASS, 15 presents, same transitions, 0 validation errors |
| Visual inspection of release window | PASS, visible red Vulkan test mark and owner icon |
| UI title-bar maximize and Alt+F4 | PASS, 3840×2054 swapchain after maximize, normal close, 0 validation errors |

The interactive release run presented 5,375 frames before normal close; this is a
lifecycle observation, not an FPS benchmark. It preceded the final telemetry-only
guards that explicitly require a real close event and snapshot frame count at
observed minimization. The final release automated smoke includes those guards.
No GPU timing/VRAM peak claim is made.
Native smoke verifies no additional frames are submitted during its minimized
interval. OS WM_CLOSE reaches the real CloseRequested event before teardown.

Local raw evidence (ignored by Git): artifacts/release-smoke.stderr.log,
native-smoke.stderr.log, native-preview.png and preview.stderr.log. Initial RED
overlay run and isolation logs remain alongside them. See ADR-0012 for cause/fix.
No global overlay settings were changed. GitHub CI is configured but not remotely
executed or published in this work.

## Limits and next step

This is a Vulkan clear-frame experiment, not a completed Asset Doctor interface.
Mesh rendering, shaders/PBR, import/export, document commands, diagnostics, Heal,
rig/animation and RT/PT are not implemented. Device-lost, unavailable loader,
unsupported GPU/maintenance features, multi-GPU fallback and high-DPI transitions
between monitors have not been fault-injected. Code emits explicit failures,
but those paths are not marked runtime PASS.

First close the relevant source UI/settings/format audit gaps. Then implement
the next independent document/import/raster slice with synthetic licensed GLB
fixtures and agreed source-fidelity/resource/cancellation contracts.
