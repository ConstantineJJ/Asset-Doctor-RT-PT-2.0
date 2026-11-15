# ADR-0018 — Tangent-space normal texture preview

Status: ACCEPTED for bounded static PBR preview, 2026-11-15. MikkTSpace parity open.

Preserve optional matching F32 VEC4 TANGENT (finite XYZ, W exactly+1/-1), normal
texture, its selected UV stream and finite normalTexture.scale (including negative
values and zero). Normal texture RGB is linear UNORM, reconstructed to[-1,1], XY
scaled, normalized; alpha ignored. Existing embedded PNG/JPEG/admission rules apply
even in base-color mode. Share existing linear GPU upload with MR if source is shared.

Snapshots derive authored tangents with the world linear transform, orthogonalize
against inverse-transpose normals, and multiply W by the world determinant sign.
Source normals/tangents/UV/bytes stay unchanged. Missing normals, zero/parallel
tangents or singular transforms produce a zero tangent sentinel. Snapshot adds
normal UV8+tangent16:80bytes/vertex+indices. Count fallback vertices only for normal
textured geometry, with instance/unused-vertex semantics matching prior counters.

Shader uses authored frame when usable, otherwise a per-fragment position/selected
UV derivative frame. Preserve mirrored-UV orientation using both U/V derivatives.
Degenerate/nonfinite derivatives or unusable frame ignore perturbation and retain
the base surface normal. Missing authored tangents do not trigger source smoothing,
geometry expansion or repair. This derivative fallback is explicitly not MikkTSpace;
full baked-map/seam equivalence stays open and the UI exposes the limitation.

Both sides remain visible under the existing preview policy: choose frame-facing
orientation from base normal vs view and reverse the complete perturbed result for
the back side. No culling/material-side parity is claimed. Surface/UV derivatives
must be evaluated before texture-alpha discard or other fragment divergence.

A third compatible descriptor set binds the normal texture, sharing the existing
linear white fallback only when sampling is disabled for that draw. Push remains
128bytes: material.w carries source normal scale; flag16 enables normal sampling
only for PBR+session normal switch+draw with a normal map. Never sample fallback
white as an actual normal. Normal switch is independent of color/MR/reset-light.

Acceptance: mirrored/nonuniform transforms and tangent W; source/malformed/count
guards; independent UV2, MR+normal shared linear image, bad normal-only PNG, invalid
frame/UV cases; actual UI switch/reset; native maps-on/off/default plus previous
texture/factor/large-model lifecycle/rejected replacement with0validation errors.
No pixel/Mikk fidelity claim from counters. No IBL/shadow/occlusion/emission/editing.

Sources: [Khronos tangent/normal requirements](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html),
[normal scale schema](https://github.com/KhronosGroup/glTF/blob/main/specification/2.0/schema/material.normalTextureInfo.schema.json).
Source behavior audit: reference/SLICE_06_NORMAL_MAP_AUDIT.md.
