# Embedded base-color rendering — 2026-11-15

Owner reported the native shell working and supplied Birch0/Adventur screenshots
showing tree, selection and summary. That is owner acceptance of the displayed
shell cases; accessibility, keyboard traversal, monitor transitions and large-tree
profiling remain separate. This increment adds the first actual image/material
sampling path, without claiming PBR or editing.

## Implemented scope

- GLB embedded buffer-view PNG/JPEG base-color images, up to 8 decoded bits/channel.
  Decode only images referenced by imported primitive base-color textures. Preserve
  original encoded source unchanged; no network, sibling or export writes.
- Selected TEXCOORD set and linear COLOR_0 with explicit shape/type/count checks.
  RGB/RGBA float and normalized U8/U16 colors; UV float or normalized U8/U16.
  Missing coordinates for a textured primitive fail explicitly.
- SRGB image RGB sampling, linear alpha and vertex tint. Fragment color is
  baseColorFactor × COLOR_0 × baseColorTexture, followed by existing geometric
  face lighting and output transfer. OPAQUE ignores alpha; MASK uses its cutoff.
  BLEND is still rejected. This remains the two-sided geometry view.
- Unique images shared on CPU and GPU even when different source textures use
  different samplers. Generated mip chains, source min/mag/mip filters and
  repeat/mirror/clamp wrapping. Unspecified sampler defaults use linear/trilinear.
  A 1×1 white fallback preserves untextured geometry.
- Summary now shows imported image count/prospective RGBA mip storage and a
  **Базовые текстуры** switch. It changes sampling only, without altering source.

No authored-normal rendering, metallic/roughness/normal/emissive maps, PBR lighting,
blend sorting, culling parity, texture transforms, compressed textures, animation,
editing or export fidelity is claimed. Referenced image data URIs and >8-bit
images are explicit unsupported cases. Required extensions remain rejected.

## Admission and lifetime

Defaults: at most 256 referenced textures/images, side <=8192, summed prospective
full mip RGBA storage <=256 MiB. Header dimensions/decoder limits and cumulative
mip bytes are checked before pixel decode. Cancellation surrounds synchronous
codec calls; it cannot interrupt inside a codec. Source/JSON/node/draw caps stay
unchanged. Geometry admission now conservatively counts 36 bytes/vertex plus
indices (position+UV+RGBA), including instances. Metadata/alignment/codec scratch,
staging, UI and concurrent old/candidate allocations are additional. These are
operational admission limits, not total process or peak VRAM bounds.

Texture admission commits counters only after success and retains Arc owners for
stable identity. Candidate Vulkan buffers/images complete their copy/mip fences
before old scene replacement. Partial resource owners clean failures; old scene
survives malformed images. Each mip transitions through transfer layouts before
shader reading. Separate image/sampler descriptors share a compatible layout
across swapchain recreation; egui texture ownership remains independent.

## Verification

| Check | Result |
|---|---|
| Final workspace tests | 36 PASS: desktop9, document7, formats13, presentation7 |
| fmt, all-target clippy warnings denied, Windows release build | PASS |
| cargo-deny advisories/bans/licenses/sources | PASS; no new policy exceptions |
| Plain/static release regression | 15 presents; resize/minimize/restore/native close; 0 validation errors |
| Color_Checker release fixture | 1 GPU image, 2 texture/sampler bindings, 20 mip bytes; 15 mesh presents; malformed PNG replacement preserves old CPU/GPU scene; lifecycle; 0 errors |
| Read-only Birch0 CPU probe | 4 nodes, 6853 expanded vertices, 4562 triangles, 2 images/textures, 9 MiB base RGBA / 12 MiB mip storage |
| Read-only Birch0 native run | 2 GPU images/textures, 15 mesh presents, failed image replacement retained old model, lifecycle; 0 validation errors; original file SHA256 unchanged |

Textured/static import runs requested child VK_LAYER_VALIDATE_SYNC=1; effective
layer setting was not separately queried. Self-closing app smoke windows only;
no mouse/desktop input or screen capture. Test windows are closed. Logs are local
Git-ignored artifacts/color-source-tests.log, release-import-smoke-color-sync.*
and birch-color-smoke.*. No screenshots/pixel readback were captured; exact visual
color/orientation/mask appearance and owner acceptance of this build remain open.

Tests include malformed image/header, oversized dimensions, mip/texture-count
limits, UV/color reader guards, PNG sharing, JPEG decode/cancel, immutable source
and worker recovery after image failure. Mip/admission and initial embedded-image
fixtures were observed RED then GREEN. Source review found no remaining concrete
CPU/GPU blocker; upload/device-loss/allocator failure and memory peaks are untested.

## Reproduction and dependencies

`tools/import-smoke.ps1 -Scene color -SynchronizationValidation` opens a short
self-closing native check. `cargo run -p ad-formats --example inspect_glb -- file.glb`
is a read-only developer CPU probe. Fixture provenance is in tests/fixtures/README;
all numerical geometry/PNG data is project-authored. User Birch0 remains a local
check and is not redistributed in the repository.

image 0.25.10 uses PNG/JPEG features only. Its six added resolved packages and
licenses/replacement boundary are in DEPENDENCIES.md; no new software install is
needed. Release EXE is 7,441,408 bytes. Shader compiler remains build-only naga;
explicit image/sampler bindings are mapped to SPIR-V. No GLSL compiler executable,
legacy decoder or second GPU backend was added.

Material reference: [Khronos glTF 2.0 color/alpha/texture specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html).
Next: owner visual review, then authored normals and bounded PBR lighting contracts.
