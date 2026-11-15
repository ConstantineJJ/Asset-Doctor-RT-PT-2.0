# Tasks

- [x] A1a: Initial source inventory: 66 functions/scenarios, all 10 REQ IDs, exact evidence and source hashes. Verify: tools/audit_legacy.py. Full semantic UI/native audit remains open below.
- [ ] A1b: Map all 197 UI declaration sites; classify 110 state candidates; complete parameter/error/native workflow acceptance. See reference/LEGACY_GAP_REGISTER.md.
- [x] A2a: Independent workspace, icon assets, dependency register, Windows CI definition; portable GNU fmt/clippy/test/executable build passed.
- [x] A2b: MSVC 1.100.0 toolchain verified after owner «Готово»; local cargo-deny license/advisory/source/bans PASS.
- [ ] A2c: Execute remote CI after an explicit publication instruction; no push requested.
- [x] A3a: Vulkan capability probe ran on RTX 5060 Ti: loader/instance/physical devices/queues/extensions logged; clean exit.
- [x] A3b: Validation-enabled physical-device probe PASS with installed SDK layers.
- [ ] A3c: Unavailable-loader, unsupported GPU/features and device-lost fault injection remain unverified.
- [x] A4: Native window/clear/icon slice: debug and release smoke, 15 presents + resize/minimize/restore/real WM_CLOSE, 0 validation errors with normal RTSS enabled. CUA inspection/maximize/Alt+F4 also passed. See native report/ADR-0012.
- [ ] Gate: Close audit mapping/native behavior gaps and decide ADR-0002/4/5/6/7 before product logic.
- [ ] Next product slice: GLB import → independent document → Vulkan raster view with a synthetic licensed fixture, cancellation/resource admission and roundtrip checks.
- [x] B0: Review static-import behavior and decide bounded contracts (SLICE_01_IMPORT_AUDIT, ADR-0013). Broader parity gate remains open.
- [ ] B1: Independent document/snapshot and bounded GLB parser with failure/cancel/source-preservation tests.
- [ ] B2: Depth-tested Vulkan mesh raster and transactional scene upload.
- [ ] B3: Native picker/drop, background import, latest-result guard and orbit/pan/zoom/fit.
- [ ] B4: Native valid import, failed replacement, camera and lifecycle smoke; document actual compatibility and limitations.
- [ ] Later: PBR, diagnostics, Heal, manual rig/animation, unified Compare, verified export and installer.
- [ ] Later: RT → PT → Mixamo rights check/autorig/retargeting.
