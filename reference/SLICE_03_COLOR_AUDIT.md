# Base-color behavior review — 2026-11-15

Legacy HEAD 17ca3549767aac78bf4ad51fa1c254c90dd15113, clean on recheck. Read-only
source study only. Existing format inventory and GLBLoaderService.ts:56-86,145-165
establish decoder setup, external-resource refusal and authored-material capture.
ShadingMode.ts:59-67 retains authored material/normal splits in its hybrid view.
No source implementation, Three.js object graph or decoder is copied/executed.

New behavior is independently contracted in ADR-0015 against the Khronos glTF2
specification: sampled SRGB base color multiplied by linear factor and COLOR_0;
alpha is linear; MASK applies the cutoff, OPAQUE coverage is one. Vertex UVs
select the base-color texture's declared coordinate set. Resource fetching is
never inferred from choosing one GLB. Native errors retain the previous model.

Owner screenshots confirm current shell works and show missing foliage/bark
color. Read-only JSON inspection of the existing Birch0 GLB confirms two images,
two textures and baseColorTexture references. It is a local compatibility check,
not a licensed fixture for redistribution or proof of whole material fidelity.

PBR, source-normal rendering, normal maps, BLEND/culling, compressed texture
decoders, editing, export, diagnostics and legacy material parity remain open.
Authoritative color semantics: https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html
