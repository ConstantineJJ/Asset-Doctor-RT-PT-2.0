# ADR-0016 — Authored normals and direct PBR preview

Status: ACCEPTED for a bounded preview, 2026-11-15. Full material/lighting fidelity
and scene light entities remain open. Current base-color view stays the default.

Import optional F32 VEC3 NORMAL plus metallicFactor/roughnessFactor (default1).
Validate component/dimension/count before typed readers and finite factor ranges.
Preserve original local normals/factors/source unchanged. Snapshots derive world
normals via inverse-transpose of the world linear transform, using double precision
for inversion/normalization before f32 output. Missing/zero/singular-transform
normals use a zero sentinel for shader geometric-face fallback; no silent source
normal repair or forced smoothing. Count fallback vertices in the snapshot.
Geometry admission becomes48 bytes/vertex+indices; prior caps stay unchanged.

Add an opt-in studio PBR view: GGX normal distribution, correlated Smith visibility,
Schlick Fresnel, metallic workflow and Lambert diffuse under one fixed directional
key. A simple fill term improves inspection; it is an ambient approximation, not
image-based lighting. Exposure in EV and Reinhard mapping apply only to PBR.
Current base-color/face-light mode remains visually unchanged for comparison.
Both modes retain the two-sided geometry view; source culling parity is not added.

UI owns session-only mode/exposure/key/fill presentation settings. They never edit
source material factors or create scene lamps. Renderer validates settings and
receives actual camera eye matching viewport aspect. A128-byte push block carries
MVP, base factor, eye/exposure, material factors/cutoff and view flags/intensities;
this fits Vulkan's guaranteed push-constant minimum. No new descriptor or library.

Metallic/roughness textures, normal maps/tangents, emissive/occlusion maps, IBL/HDR
environments, shadows, BLEND and extra material extensions remain unsupported in
this view. Import notes must expose ignored material maps to the user. The full
source remains available for future implementation. No claim of reference-renderer
equivalence, physically calibrated light units or final PBR parity.

Acceptance: inverse-transpose/nonuniform/mirrored/singular/zero-normal fixtures;
malformed normal reader and factor validation; geometry/cancel/source regression;
camera eye/settings validation; GPU PBR/static/textured lifecycle and replacement
smokes with0 validation errors. --pbr is a developer launch/smoke option. Native
tests use self-closing windows; no computer input or capture.

Sources: [Khronos glTF2 normal/material requirements](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html),
[Khronos reference BRDF definitions](https://github.com/KhronosGroup/glTF-Sample-Renderer/blob/main/source/Renderer/shaders/brdf.glsl).
Source behavior review: reference/SLICE_04_PBR_AUDIT.md. Full audit gates remain open.
