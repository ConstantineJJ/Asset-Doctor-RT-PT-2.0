# Static GLB viewer — engineering increment, 2026-11-15

The native application now loads an independent asset document and renders its
static triangle snapshot through Vulkan. This implements the bounded import/view
contracts in ADR-0013; it does not close full Legacy parity or deliver the final UI.
Legacy implementation and supplied Arc-XRay reports remain untouched.

## Delivered behavior

Ctrl+O opens a native single-file GLB dialog. Drop and --open use the same worker.
One worker runs one request with one replaceable pending slot; new requests cancel
the previous job. Ticket checks prevent late publication. Parse and GPU-upload
failure leave the previous document/GPU scene intact. Choosing Cancel preserves
the model. The application reads source bytes and never rewrites asset files.

Left drag orbits, right/middle drag pans, wheel zooms and F frames indexed bounds.
No automatic rotation. Esc cancels import; H opens basic controls help. The title
shows source filename and expanded snapshot vertex/triangle counts. These counts
are display geometry, not a full source analysis or diagnostic report.

The renderer uses authored node transforms, indexed triangles, per-primitive
base-color factors, geometric face lighting, depth testing and explicit display
encoding for the UNORM swapchain. Materials render two-sided in this geometry
view; authored culling/normals/metallic/roughness/lighting are not yet represented.
There is one in-flight frame. Geometry buffers are immutable; complete candidate
upload precedes replacement and old GPU resources retire after waits.

## Compatibility and resource boundaries

Supported: GLB 2, one embedded BIN buffer, default/first scene, static triangle
primitives, non-normalized F32 VEC3 positions, unsigned scalar indices or sequential
indices, opaque base-color factors, retained hierarchy/transforms/immutable source.
Primitive-ID lists are shared between source-mesh instances. Snapshot expansion
is admitted before allocating its world-space vertex/index arrays.

Explicitly rejected: external buffer/image resources, required extensions,
Draco/Meshopt/quantized geometry, sparse accessors, non-triangle primitives,
transparent/masked materials, skins, animations and morph targets. Embedded
images/data-image URIs are retained as source bytes, never decoded or displayed.
Textures, vertex colors, custom materials and optional metadata do not affect this
base-color view. Multiple scenes show only default/first. No glTF JSON sidecar,
PBR, repair, diagnostics, editing, export/roundtrip, RT/PT or installer is claimed.

Operational admission: source 192 MiB, JSON 16 MiB, decoded geometry 512 MiB,
snapshot/GPU geometry 512 MiB, nodes 50,000 and snapshot draws/instances 100,000.
These are not a total process/VRAM budget. Old and candidate source/CPU/GPU data
coexist during replacement; metadata, allocator overhead and depth/swapchain
images are additional. Peak use, large-scene performance and allocation-failure
injection remain unmeasured. Host-visible coherent mesh buffers prioritize the
first verified slice; device-local staging/instancing remain future decisions.

## Evidence and limits

| Check | Observed result |
|---|---|
| Final source fmt, all-target clippy with warnings denied, tests, Windows release build, cargo-deny | PASS; 23 tests, advisories/bans/licenses/sources pass |
| Release baseline lifecycle smoke | PASS; 15 presents, resize/minimize/restore/real WM_CLOSE, validation enabled, 0 errors |
| Release import smoke | PASS; authored Twin_Cubes, 16 expanded vertices/24 triangles, failed truncated replacement preserves document/GPU tags, 15 mesh presents, lifecycle, 0 errors |
| Same import smoke with child VK_LAYER_VALIDATE_SYNC=1 | PASS; 0 validation errors; setting requested using current layer documentation, effective layer configuration not independently queried |
| Debug native geometry image and left-drag orbit | Visually observed; artifacts/static-glb-preview.png |
| Release native picker display/cancel, maximize, Alt+F4 | Observed; clean shutdown, 0 validation errors |
| Committing a file through native picker; real OS file-drop; manual pan/zoom/fit | Not certified by UI automation; shared worker/camera paths are unit/smoke tested |
| GPU upload failure/device lost/unsupported GPU/high-DPI monitor transitions | Not injected; remains open |

Native runs above preceded the final review fixes: raw accessor preflight and
typed-reader checks, shared primitive IDs/deduplicated admission, draw cap, parser
unwind recovery and input reset on focus loss. Final source tests/clippy/build
passed after these changes. The owner reclaimed the PC; no later UI/GPU launch
was performed. Repeat native smoke when interactive testing is authorized again.
This report does not claim final-build native acceptance or owner acceptance.

Review reproduced gltf-json 1.4.1's invalid POSITION panic before the fix, then
passed the malformed-input regression. Repeated mesh nodes now share Arc slices;
snapshot expansion has a draw cap. The import worker test successfully loads the
valid source after malformed inputs. A separate source review found no remaining
concrete blocker in the reviewed paths; this is not a fuzzing or memory-peak proof.

Reproduce build-independent source tests with tools/dev.ps1. Native tests are
tools/smoke.ps1 and tools/import-smoke.ps1; the latter optionally accepts
-SynchronizationValidation. They open a window and must not run while the owner
has reclaimed the desktop. Logs/screenshots are local Git-ignored artifacts.

Layer setting reference: [Khronos validation settings migration](https://github.com/KhronosGroup/Vulkan-ValidationLayers/blob/main/docs/updating_from_VK_EXT_validation_features.md).
Fixture provenance/hashes: tests/fixtures/README.md and manifest.json. All fixture
geometry is project-authored mathematical data; no external model was bundled.

Next: repeat final native import/input verification when permitted, then complete
the relevant scene-tree/material/view-settings audit and contracts before adding
the real UI or a PBR/diagnostics increment. Broader audit/feature gates stay open.
