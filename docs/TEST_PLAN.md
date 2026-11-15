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
