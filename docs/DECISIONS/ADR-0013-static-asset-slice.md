# ADR-0013 — Independent static asset import and raster slice

Status: ACCEPTED for this bounded increment, 2026-11-15.

Add ad-document (immutable domain data, IDs/revision, scene snapshot/invariants)
and ad-formats (GLB parsing/validation/admission). Both have zero Vulkan/window
dependencies. Renderer consumes a revision-tagged geometry snapshot; desktop
owns jobs, camera and current document. No umbrella app/core manager is added.

Use gltf 1.4.1 with default import/network/image-loading features disabled;
utils/names/extras only. Parse source memory, never fetch resources. Use glam
0.34.3 internally for transforms/camera. Public domain geometry uses plain arrays.
Use naga 30.0.2 as a BUILD-only GLSL→SPIR-V compiler: runtime stays ash/Vulkan,
without wgpu or WebView, and CI needs no prebuilt binary shader/SDK compiler.

Data basis: right-handed, Y-up, metres; authored local matrices/mesh references
are retained. Immutable source bytes preserve unknown metadata for future format
work; no export fidelity claim. Node IDs are source-local indices scoped by a
document ID; revision starts at zero. History/commands are not implemented until
editing. View-space centering/scaling never mutates document geometry.

The first snapshot expands active static instances into world-space vertices,
indices and base-color draw ranges. This deliberate simple CPU representation
is separately budgeted; it is replaceable by shared GPU geometry/instance data
later. No premature renderer/backend/event-bus abstractions.

Admission defaults: source 192 MiB, JSON chunk 16 MiB, decoded geometry 512 MiB,
snapshot/GPU geometry 512 MiB, nodes 50,000,
with a separate 100,000 snapshot draw/instance cap. Source mesh primitive-ID lists
are shared via immutable Arc slices and validated once per distinct list; repeated
node instances cannot cause a Cartesian-product ID allocation/admission scan.
These are initial operational caps,
not claims of whole-process RAM/VRAM limits. Account for source + decoded + snapshot
retention; parser metadata/allocator overhead and GPU depth targets are additional.
Checked arithmetic, cancellation and finite/range validation precede publication.
Preflight primitive accessor references before gltf-json validation; typed readers
require explicit position/index component/dimension checks. A third-party parser
unwind becomes a structured import error so the worker can process another file.

Desktop worker imports one request at a time; a newer request cancels and replaces
the queued request. Results carry tickets. GPU upload creates a complete candidate
before replacing the previous GPU scene; current document changes only after that
succeeds. This preserves current model on parse/cancel/budget/upload errors.

Static triangle/BIN GLB only initially; unsupported data is an explicit error.
Baseline shader displays base color with geometric face lighting and depth, not
PBR/textures/skin/animation. UNORM output needs explicit display encoding.

Acceptance: synthetic licensed GLB → document → raster; nested/negative transforms;
malformed/range/URI/unsupported/budget/cancel rejection; source unchanged;
late-result guard; camera controls; native import + failed replacement + resize/
minimize/restore/close with zero validation errors. Source review gate is recorded
in reference/SLICE_01_IMPORT_AUDIT.md. Full feature-parity gates remain open.
