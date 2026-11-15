# Tasks

- [x] A1a: Initial source inventory: 66 functions/scenarios, all 10 REQ IDs, exact evidence and source hashes. Verify: tools/audit_legacy.py. Full semantic UI/native audit remains open below.
- [ ] A1b: Map all 197 UI declaration sites; classify 110 state candidates; complete parameter/error/native workflow acceptance. See reference/LEGACY_GAP_REGISTER.md.
- [x] A2a: Independent workspace, icon assets, dependency register, Windows CI definition; portable GNU fmt/clippy/test/executable build passed.
- [ ] A2b: Verify MSVC after owner «Готово», run dependency/license checker and later remote CI. Local checker installation failed on missing portable GNU assembler.
- [x] A3a: Vulkan capability probe ran on RTX 5060 Ti: loader/instance/physical devices/queues/extensions logged; clean exit.
- [ ] A3b: Validation-enabled probe and unavailable-loader fault path. The first probe reported validation_enabled=false.
- [ ] A4: Native Vulkan window/clear spike. Acceptance: actual present, resize/minimize/restore and shutdown; validation status reported. Depends A3.
- [ ] Gate: Close audit mapping/native behavior gaps and decide ADR-0002/4/5/6/7 before product logic.
- [ ] Next product slice: GLB import → independent document → Vulkan raster view with a synthetic licensed fixture, cancellation/resource admission and roundtrip checks.
- [ ] Later: PBR, diagnostics, Heal, manual rig/animation, unified Compare, verified export and installer.
- [ ] Later: RT → PT → Mixamo rights check/autorig/retargeting.
