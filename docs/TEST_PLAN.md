# Verification plan

## CPU command/history foundation — 2026-11-16

Code9a8e454:133workspace tests, fmt/clippy/release/dependency gates PASS. Exact
index changes, monotonic revisions, stale/foreign origin guards, no-op/failed
branch retention, byte/entry limits, pointer/source sharing, mid-copy cancellation
and interleaved Undo/Redo verified. Existing viewer UV/normal/empty native
regressions0validationerrors. This is not desktop edit/Heal/export acceptance.
COMMAND_HISTORY_REPORT_2026-11-16.md records scope and next contracts.

## Current UV diagnostic increment — 2026-11-16

Codeff355f9:113workspace tests and Windows/native verification PASS within
ADR-0025scope. Source UV0absence, per-channel exact area/range, explicit N-A,
atomic output, partial results, typed samples and deterministic mid-scan cancel
are covered. Full/partial compact UI, malformed unused-UV replacement, lifecycle
and heavy source preservation verified. UV_DIAGNOSTICS_REPORT_2026-11-16.md records
evidence and limits. Older numbered test counts below are historical increments.
Remote CI through615cbff also passed; local GPU evidence remains separate.

## Bootstrap

- Unit: 6 tests covering zero-size refusal, extent negotiation, image-count bounds
  and overlay-compatible format choice. RED before implementation/fix, then GREEN.
- GPU capability probe: dynamic loader, instance, real device enumeration and clean
  teardown. --require-validation must fail clearly if layers unavailable.
- Windows/MSVC: fmt, clippy, test, build in CI. CI has not run until pushed.
- Dependency audit/license checks are distinct from compile success.

## Native clear-frame slice — completed

Native window with supplied icon, real swapchain clear/present, resize → minimized
no submission → restore → present, close/teardown. Log frame/resize counters and
actual layers/device/driver. Validation required for certified GPU smoke. Add
device-lost fault classification remains unverified; never provoke actual driver
failure on the user's machine. Cross-monitor DPI and final UI input ownership are
still open. Actual baseline results: NATIVE_FOUNDATION_REPORT_2026-11-15.md.

## Product gate

Complete semantic mapping of all indexed UI sites, settings and P0 error flows.
Then implement each independent document/import/edit/export contract with RED →
GREEN tests, synthetic assets and native smoke. A new product's tests do not
inherit historical Legacy PASS. Visual, structural, runtime and user acceptance
remain separate. See parity registry for individual acceptance scenarios.

## Static import/raster slice — completed within ADR-0013 scope

- Unit: 5 document tests (transforms, authored source, indexed bounds, invalid
  indices, cycle rejection, cancellation, snapshot budget); 9 GLB tests (container,
  metadata/accessor bounds, external URI, unsupported data/material, source and
  hierarchy, budgets/cancel); 3 desktop camera/worker tests. With 6 presentation
  tests the workspace has 23 passing tests. Malformed accessor and shared-instance
  regressions were added during review; the worker processes a valid fixture after
  both truncated and out-of-range-accessor errors.
- Release `tools/import-smoke.ps1`: valid authored fixture → document → GPU,
  failed truncated replacement preserves CPU/GPU tags, 15 mesh presents, camera
  matrix exercise, resize/minimize/restore/native close, 0 validation errors.
- `-SynchronizationValidation` requests the dedicated modern layer setting in
  the child process only and restores the caller environment. Keep separate logs.
- Native picker, camera visual inspection and acceptance are recorded separately
  in STATIC_VIEWER_REPORT_2026-11-15.md. A smoke matrix exercise alone is not mouse
  event verification. No export/roundtrip or full legacy compatibility is claimed.

## Native viewer UI shell — bounded integration completed

ADR-0014 adds headless egui layout/input/action tests, a 10,000-level hierarchy
projection check and physical viewport/DPI clipping tests. Workspace total: 30.
UI hit-test ownership and zoom regressions were RED before their fixes. Font
texture deltas are consumed before drawing and deletion waits for frame retirement.
Default app smoke now exercises the UI paint path alongside geometry. Release
empty/import/rejected-replacement/lifecycle runs passed with validation enabled,
0 errors; final import run requested synchronization validation as documented.
Physical UI interaction, visual acceptance, accessibility and monitor transitions
remain separate checks. See VIEWER_UI_REPORT_2026-11-15.md.

## Base-color textures — bounded integration completed

ADR-0015 adds PNG/JPEG, selected UVs, linear COLOR_0, OPAQUE/MASK and resource
admission. Total workspace tests:36. Authored Color_Checker shares one image
across two samplers; malformed/oversized image fixtures fail without replacement.
Native static/color/Birch0 import/lifecycle checks passed with 0 validation errors.
Color smoke is tools/import-smoke.ps1 -Scene color -SynchronizationValidation.
Image orientation/visual color/mask appearance, blend/culling/PBR, fault injection
and peak memory remain separate unverified checks. See BASE_COLOR_REPORT.

## Direct PBR and MR textures — bounded integration completed

ADR-0016 adds inverse-transpose normal derivation, source factors, actual camera eye,
finite lighting controls and128-byte push layout checks. ADR-0017 adds independent
MR UVs, linear B/G sampling, source sharing and per-transfer mip/count admission.
Workspace49tests: desktop11/document10/formats18/renderer10. Resource regressions
were observed RED→GREEN, including refusing a new image before entering its codec
when sampled variants are exhausted. Actual UI checkbox/reset tests keep MR and
base-color sampling independent without changing source.

Native MR fixture runs in PBR with maps on/off and in default mode, plus prior
color/MASK/factor/static/empty UI regressions. Each completed15mesh presents where
applicable, failed replacement preservation, resize/minimize/restore/native close
and0 validation errors. Real RedCatCar and heavy Violet_Vanguard2 also passed in
PBR with MR enabled and originalSHA256 unchanged. See MR_TEXTURE_REPORT for exact
counts and requested synchronization-validation setting. Frame counters and clean
validation do not prove pixel accuracy or physical-input/visual acceptance.

## Tangent-space normal maps — bounded integration completed

ADR-0018 adds source TANGENT/normal scale/UV checks, mirrored world tangent parity,
orthogonalization, independent normal sampling flags and third descriptor sharing.
Workspace54tests. Normal_Checker uses authored/missing/singular-frame conventions,
mirrored/nonuniform scale, UV2 and zero/negative normal scale. Bad_Normal_Image
checks normal-only decoder failure. Native maps-on/off/default, prior MR/MASK/
factor/plain regressions, RedCat/Mini-bus/Vanguard import+failure+lifecycle all
passed with0validation errors. Original model hashes unchanged.

Owner explicitly authorized autonomous development and computer-based verification.
Native window checks confirmed normal-toggle response on spheres and RedCat, unchanged
zero-scale case, orbit, maximize and native close, with0validation errors. Ctrl+O
opened the OS picker; Escape cancellation retained the model. Automated filename
focus failed despite AX/screenshot targeting, so picker file commit remains unverified
by automation. It is not a demonstrated importer/picker application defect. No owner
acceptance gate remains. Mikk/quantitative pixel/LOD/fault-injection checks stay open.

## Source occlusion — bounded integration completed

ADR-0019 adds AO strength/range/UV/admission and shared ORM tests. Malformed AO-only
data was silently ignored (RED), then explicitly rejected (GREEN).58workspace tests
include actual AO checkbox and lighting reset independence. Native AO on/off/default,
normal/MASK/empty regression and heavy Vanguard normal/import/failure/lifecycle
passed with0validation errors. AO fixture uses source strength0/0.5/1/default1 and
independent UV2; one linear image serves MR+AO. Native interactive comparison at
key0/fill1 showed AO modulation and unchanged strength0; key8/fill0 looked unchanged
when toggling AO. This is qualitative validation, not a calibrated pixel metric.
