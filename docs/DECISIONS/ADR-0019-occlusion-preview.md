# ADR-0019 — Occlusion map in inspection fill

Status: ACCEPTED for bounded direct-PBR preview, 2026-11-15.

Preserve optional occlusionTexture, selected UVs and strength(default1,finite[0,1]).
Sample linear R only, ignore G/B/A. Effective occlusion is1-strength+strength*R.
It multiplies the existing approximate fill term only: direct diffuse/specular,
base-color sampling and source factors are unchanged. This is not ray-traced AO,
IBL or a geometry repair. A session-only independent AO toggle does not edit source.

Reuse the linear decoder/upload with MR/normal source identity; admit all referenced
AO resources under existing caps even in default mode. Missing UV/malformed/unsupported
images fail transactionally. A separate AO UV stream makes88bytes/vertex+indices.
Fourth compatible descriptor set uses the guaranteed Vulkan minimum of4sets; no
fifth set may be added without redesigning material/camera binding contracts.
Use spare lighting.w for strength, flag32 only for PBR+toggle+draw AO map.128byte
push stays unchanged. Missing maps retain white fill with no extra image upload.

Acceptance: independent UV2, linear MR/AO sharing, strength0/0.5/1, malformed AO-only
image, finite/range/UV guards, shader direct/fill separation, independent toggle/
reset, native on/off/default/regression/rejected replacement with0validation errors.
No calibrated pixel/LOD/physical-light equivalence is inferred from counters.

Authority: [Khronos glTF additional textures](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#additional-textures).
Behavior reference: reference/SLICE_07_OCCLUSION_AUDIT.md.
