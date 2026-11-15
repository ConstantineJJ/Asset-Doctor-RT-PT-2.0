# Tasks

- [x] A1a: Initial source inventory: 66 functions/scenarios, all 10 REQ IDs, exact evidence and source hashes. Verify: tools/audit_legacy.py. Full semantic UI/native audit remains open below.
- [ ] A1b: Map all 197 UI declaration sites; classify 110 state candidates; complete parameter/error/native workflow acceptance. See reference/LEGACY_GAP_REGISTER.md.
- [ ] A2: Create independent buildable workspace and Windows CI. Acceptance: format/lint/test/build commands work; icon and dependencies tracked. Depends A1 identity only.
- [ ] A3: Vulkan capability probe. Acceptance: GPU/driver/API/queues/extensions logged, unavailable loader errors clean. Depends A2.
- [ ] A4: Native Vulkan window/clear spike. Acceptance: actual present, resize/minimize/restore and shutdown; validation status reported. Depends A3.
- [ ] Gate: Close audit mapping/native behavior gaps and decide ADR-0002/4/5/6/7 before product logic.
- [ ] Next product slice: GLB import → independent document → Vulkan raster view with a synthetic licensed fixture, cancellation/resource admission and roundtrip checks.
- [ ] Later: PBR, diagnostics, Heal, manual rig/animation, unified Compare, verified export and installer.
- [ ] Later: RT → PT → Mixamo rights check/autorig/retargeting.
