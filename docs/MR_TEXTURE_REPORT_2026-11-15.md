# Metallic/roughness texture integration — 2026-11-15

Owner continued after supplying a PBR RedCat screenshot with exposure/key controls.
That is evidence for the prior factor-only example, not this MR build's visual
acceptance. Relevant read-only audit and independent contract: SLICE_05/ADR-0017.

## Current behavior

- Import referenced embedded8-bit PNG/JPEG metallicRoughnessTexture, preserving its
  sampler and selected TEXCOORD set independently of base color. B supplies metal,
  G roughness, each multiplied by its source factor. R/A are ignored. Vulkan uses
  linear UNORM sampling/mips for data and SRGB RGB/linear alpha for base color.
- CPU pixels/source texture identity are shared across roles. GPU image cache keys
  include transfer encoding; descriptor cache keys include source texture+encoding.
  Two compatible descriptor sets and five attribute buffers bind independent base
  and MR UV streams. Push data remains128bytes. White fallbacks preserve untextured
  factors and MR-only materials. Base-color/MASK behavior stays unchanged.
- **Сводка → PBR · направленный свет → Карты металла/шероховатости** enables/disables
  only the MR map contribution. Source factors still apply when disabled. It is
  independent of **Базовые текстуры** and is not reset by resetting illumination.
  All controls are session-only and never alter the document or source file.

MR resources now undergo the same strict import checks even in the default view.
Invalid images/UVs, unsupported data URI/format/texture transforms or excess budgets
fail explicitly and preserve the current model. Normal/occlusion/emissive maps,
HDR/IBL/shadows, BLEND, tangent processing and material extensions remain open.
Existing two-sided preview/fill approximation/roughness floor remain as ADR-0016.

## Resource accounting

Image pixels decode once per source image; each (image,transfer) use consumes a
distinct sampled image variant. Unique image/source texture/variant counts each
stay within256, total prospective full-mip RGBA within256MiB, side within8192.
MR UVs make geometry admission56bytes/vertex+indices. Decode dimensions/mip/count
refusal occurs before a new image's codec allocation. Source review found a missing
variant-count preflight: reproduced with exhausted variants and malformed next PNG
(RED InvalidFormat), fixed to refuse ResourceLimit before codec (GREEN).

Limits are not total process/VRAM measurements: alignment, source/metadata, codec
scratch, staging, old+candidate scenes, UI and two fixed white fallback images add
memory. Mip estimates include separate SRGB/UNORM copies even for shared pixels.

## Verification

| Check | Result |
|---|---|
| Workspace |49testsPASS: desktop11/document10/formats18/renderer10 |
| fmt/all-target clippy with warnings denied | PASS |
| Windows release workspace build | PASS; desktop EXE7,694,848bytes |
| cargo-deny advisories/bans/licenses/sources | PASS; no new package/policy exception |
| MR_Checker native PBR maps on |1decoded image,1source texture,2GPU images(1SRGB/1UNORM),40mipbytes;15MR presents; malformed MR replacement preserved;0errors |
| Same fixture maps off / default view |15mesh presents each,MR0; same admitted resources; failed replacement preserved;0errors |
| Prior color/MASK PBR, factors PBR, plain default, empty UI | PASS;15mesh presents for imported scenes; lifecycle/replacement;0errors |
| RedCatCar_model_60Ktris_Last |31471vertices/46731triangles;2images,128MiB baseRGBA/170.67MiB mips;15MR presents; failed replacement preserved;0errors; originalSHA256 unchanged |
| Violet_Vanguard2 heavy model |852516vertices/1559826triangles;2images/106.67MiB mips;15MR presents; failed replacement preserved;0errors; originalSHA256 unchanged |

Every native run completed resize/minimize/restore/native close. Empty UI has no
model replacement step. Import runs requested child VK_LAYER_VALIDATE_SYNC=1;
effective setting was not independently queried. Self-closing app windows only,
without computer input or capture; windows closed. Native logs are Git-ignored
artifacts/release-import-smoke*mr*.*, redcat-mr-smoke.*, vanguard-mr-smoke.* and
regression logs. No user geometry or texture was copied into the repository.

Tests cover CPU sharing/encoding budgets/atomic refusal, independent UVs/raw pixels,
source preservation, malformed MR image/missing or invalid UV, descriptor fallback
plan, actual UI checkbox and reset behavior. Source-only independent review found
no remaining concrete blocker after the predecode fix. Shader code selects B/G;
runtime counters show the enabled path, not a pixel oracle. Visual/channel/LOD
calibration, memory peaks/performance and device-loss/allocation injection are open.

## Reproduce

Open project-authored tests/fixtures/MR_Checker.glb in PBR. Three spheres share one
numeric2×2 PNG; constant base UV0 differs from varying MR UV1. First uses unit
factors, second0.5 factors, third MR-only/white base. R/A vary independently of B/G.
Bad_MR_Image references malformed PNG only through MR. Generator/hash provenance
is in tools/make_mr_fixtures.py and the fixture manifest.

`tools/import-smoke.ps1 -Scene mr -Pbr -SynchronizationValidation` runs the bounded
check; add `-NoMrMaps` to exercise disabled sampling. Omit `-Pbr` for default mode.
Developer launch accepts `--no-mr-maps`; normal app startup remains base-color mode.

No new dependency/version/license policy, legacy/report change or remote publication.
