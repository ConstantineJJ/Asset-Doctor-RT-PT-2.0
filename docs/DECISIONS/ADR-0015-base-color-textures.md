# ADR-0015 — Bounded base-color texture slice

Status: ACCEPTED for this increment, 2026-11-15. PBR lighting/normal maps remain
open; this does not claim complete glTF material fidelity.

Implement embedded PNG/JPEG base-color maps, referenced UV set, linear COLOR_0
and alpha MASK. OPAQUE ignores alpha; BLEND remains explicitly unsupported.
Keep the current two-sided face-lighting geometry view until authored normals,
culling and PBR contracts are implemented. No new lamp/entity/editing system.
The owner's Birch0 example has two embedded base-color images; they are currently
omitted by the viewer. Test that asset read-only locally, never vendor its data.

ad-document owns independent immutable image/sampler data shared through Arcs;
primitives retain optional selected UVs/colors and base-color texture references.
Snapshots copy UV/color attributes with instances but share image bytes. Original
source stays untouched. Missing UV on a textured primitive, invalid layouts or
image data fail explicitly. Only referenced base-color images are decoded;
other maps remain preserved in original source, not silently used as base color.

Use image 0.25.10 with PNG/JPEG features only; no import helper, file/network API,
other codecs or legacy decoder implementation. Check dimensions and cumulative
RGBA/mipmap admission before decode; enforce decoder limits and cancellation
before/after codec calls. One worker remains bounded and latest-ticket guarded.
No interruption inside a synchronous codec call or total process-memory bound is
promised. Reusing an image with different samplers never duplicates CPU pixels.

Initial texture admission: 256 unique referenced textures/images, side <=8192,
256 MiB prospective full mip-chain RGBA storage. Geometry caps now account for
36 bytes/vertex (position+UV+linear RGBA) plus indices; decoded admission is
conservative when optional attributes are absent. Existing source/JSON/node/draw
budgets remain. Old/candidate allocations coexist during transactional upload.

Vulkan owns sampled SRGB images with linear alpha, generated mipmaps, source
sampler wrap/filter settings, descriptor resources and a white fallback. Candidate
image/buffer upload completes before replacing the scene. Resource owners handle
partial failures and retire before destruction. UI toggles base-color sampling;
it changes presentation only. No source image/color conversion is written back.

Referenced image data URIs, texture-transform extensions and compressed image
formats are explicit unsupported cases initially. Existing external-resource
rejection remains global. Unreferenced retained maps do not create decoder work.
No texture normal/metallic/roughness/emissive maps, transparent BLEND, PBR/RT/PT or
export fidelity claim. Whole-model summary/UI compatibility must match this scope.

Acceptance: authored PNG/JPEG/UV/COLOR/MASK fixtures, malformed headers/ranges,
missing UV/layout/budget/cancel rejection, source/image-sharing checks, plain
geometry regression, native textured import/rejected replacement/resize lifecycle
with validation. Native window checks remain subject to PC handback preferences.
Full UI/Heal/Rig/format parity gate stays open. Behavior audit: SLICE_03_COLOR_AUDIT.
