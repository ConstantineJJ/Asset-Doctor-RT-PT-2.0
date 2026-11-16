# Initial implementation plan — 2026-11-15

## Consolidated plan and status — 2026-11-16

The complete current roadmap is [MASTER_PLAN_AND_STATUS_RU.md](../docs/MASTER_PLAN_AND_STATUS_RU.md).
It retains the original stages0–11 and all66Legacy/10owner IDs, records implemented
scope against code615cbff, and defines remaining acceptance/dependencies. Read it
with [todo.md](todo.md) before continuing. The initial startup plan below is kept
as history; it is not the complete long-term product backlog.

The owner authorizes starting a new Windows Rust/Vulkan application in this repository. The supplied Get Starting document is a requirements source; its embedded agent prompt is reference text, not an independent instruction. Current user paths override its older path placeholders.

## Ordered increments

1. Identify local Legacy HEAD and dirty state; index features, UI source sites, settings, workflows, formats, fixtures and the ten owner requests. Keep native behavior UNVERIFIED until exercised. Verify evidence locations and hashes without writing to Legacy.
2. Prepare an independent Rust workspace, project rules, dependency policy, Windows CI and ADR proposals. Verify locked builds, formatting, lint and tests using the actual available toolchain.
3. Isolated Vulkan capability/window/clear experiment: load the runtime, enumerate actual GPU features, present frames, handle resize/minimize/shutdown and apply supplied icon. Verify on the RTX 5060 Ti; require an explicit diagnostic if validation layers are unavailable.
4. Close remaining audit/UX acceptance gaps before implementing import → document → scene. Then add diagnostics/Heal/Rig in separately verified slices. Raster/PBR precedes RT, then PT, then Mixamo/autorig/retargeting.

## Dependencies and risks

The GPU experiment depends only on toolchain preparation, not on unresolved product-domain decisions. Domain implementation depends on feature inventory and approved contracts. No Legacy fixes are prerequisites. Native UI candidate selection remains open until high DPI, input ownership, accessibility and Vulkan integration are evaluated. No frame-rate promises or format support claims without current evidence.

Verification artifacts are kept locally in artifacts/ and summarized in Project-pulse.md. Repo source/audit documents are committed locally. Original reports and user assets remain untouched.
