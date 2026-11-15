# Verification plan

## Bootstrap

- Unit: zero-height/width surface is not drawable; positive surface is drawable.
  Initial test failed on unimplemented behavior, then passed after implementation.
- GPU capability probe: dynamic loader, instance, real device enumeration and clean
  teardown. --require-validation must fail clearly if layers unavailable.
- Windows/MSVC: fmt, clippy, test, build in CI. CI has not run until pushed.
- Dependency audit/license checks are distinct from compile success.

## Next GPU increment

Native window with supplied icon, real swapchain clear/present, resize → minimized
no submission → restore → present, close/teardown. Log frame/resize counters and
actual layers/device/driver. Validation required for certified GPU smoke. Add
device-lost handling by injectable result classification; never provoke driver
failure on the user's machine. Check high DPI/input focus before UI selection.

## Product gate

Complete semantic mapping of all indexed UI sites, settings and P0 error flows.
Then implement each independent document/import/edit/export contract with RED →
GREEN tests, synthetic assets and native smoke. A new product's tests do not
inherit historical Legacy PASS. Visual, structural, runtime and user acceptance
remain separate. See parity registry for individual acceptance scenarios.
