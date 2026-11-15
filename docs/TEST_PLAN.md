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
