# Authored normals and direct PBR preview — 2026-11-15

The owner reported PASS for the previous textured car examples. This increment
adds an optional lighting mode; visual acceptance of this new mode remains open.
ADR-0016 and SLICE_04_PBR_AUDIT record the bounded contract and behavior references.
No legacy implementation/architecture is carried over.

## Behavior

- Preserve optional F32 VEC3 NORMAL and metallic/roughness factors, default1.
  Validate normal shape/count/finite values and factor ranges before publication.
  Derive world normals using a double-precision inverse-transpose of the world
  linear transform; normalize only derived data. Missing/zero/singular normals
  use geometric face fallback. Count fallback vertices, including instances and
  unused source vertices. Source bytes/local normals/material values stay intact.
- Geometry admission conservatively counts48 bytes/vertex plus indices
  (position12+UV8+RGBA16+normal12). Existing byte/node/draw/texture caps remain.
  These limits do not bound total process/peak memory.
- **Сводка → PBR · направленный свет** enables GGX distribution, correlated Smith
  visibility, Schlick Fresnel and metallic diffuse/specular under a fixed key.
  Source roughness stays unchanged; the shader uses a0.045 floor for finite lobes.
  A clearly approximate fill is adjustable. ExposureEV and Reinhard mapping apply
  only in PBR; SRGB output transfer remains shared. Camera eye matches actual
  viewport aspect/pan/orbit/zoom. Both sides of surfaces remain visible.
- **Базовый вид** is the default and retains accepted geometric face lighting.
  Embedded base-color PNG/JPEG, vertex colors, samplers/mips and MASK work in both.
  Lighting controls/reset affect presentation only; no source material edits,
  saved settings or scene light entities are introduced.
- The shader push layout is128 bytes, within Vulkan's guaranteed minimum.
  Four GPU attribute buffers avoid another CPU interleaved copy. Existing candidate
  upload/retirement ownership and texture sharing stay intact.

Metallic/roughness textures, normal maps/tangents, occlusion/emission, IBL/HDR,
shadows, BLEND and material extensions are outside this preview. Ignored maps
receive a Russian import note in **Ограничения просмотра**. Source culling parity,
photometric units and calibrated reference-renderer equivalence are not claimed.
Editable lamp requirements remain open.

## Verification

| Check | Result |
|---|---|
| Workspace tests |43PASS: desktop10/document9/formats15/renderer9 |
| fmt / all-target clippy warnings denied | PASS |
| Windows release workspace build | PASS; desktop EXE7,677,440bytes |
| cargo-deny advisories/bans/licenses/sources | PASS; no new package or exception |
| Pbr_Factors native PBR |15PBR mesh presents; failed replacement preserved; lifecycle;0validation errors |
| Color_Checker native PBR and default |15mesh presents each; shared image2samplers; malformed image refusal; lifecycle;0errors |
| Plain static native default regression |15mesh presents; failed replacement preserved; lifecycle;0errors |
| Birch0 native PBR |6853vertices/4562triangles/2images;15PBR presents; failed replacement preserved; lifecycle;0errors; originalSHA256 unchanged |

Every native run resized, minimized, restored and received native close. Child
VK_LAYER_VALIDATE_SYNC=1 requested; effective configuration was not independently
queried. Tests used short self-closing windows, without computer input or capture.
All test windows closed. Logs remain Git-ignored under artifacts/. No screenshot,
pixel readback, visual BRDF comparison or benchmark was collected.

The inverse-transpose regression failed before implementation and passed after;
tests include mirrored/nonuniform/tiny/singular transforms, fallback/source
preservation, malformed normal references/layouts and material factors. The UI
reset test presses/releases the actual button, keeps PBR selected and leaves source
and texture selection intact. Normal budgets, push offsets, settings bounds and
camera eye are checked. Source-only independent CPU/GPU review found no concrete
blocker. Allocator/device-loss injection and complex-model performance are untested.

## Reproduce

Open tests/fixtures/Pbr_Factors.glb and select PBR in Summary. Six smooth spheres:
top row dielectric/bottom metal, roughness0.15/0.5/0.95 left-to-right. Two triangle
markers exercise missing/singular normal fallback. Generator and manifest provide
numeric provenance; no user model or external image is bundled.

`tools/import-smoke.ps1 -Scene pbr -Pbr -SynchronizationValidation` runs the short
factor check; `-Scene color -Pbr` runs texture/MASK integration. Developer launch
`AssetDoctorRTPT.exe --pbr --open model.glb` selects this mode at startup.

Only glam's existing f64 feature was enabled; versions/licenses remain registered
in DEPENDENCIES.md. Work is local, without publication or changes to legacy/reports.
