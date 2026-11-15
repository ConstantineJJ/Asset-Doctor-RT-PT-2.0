# Verification plan

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
