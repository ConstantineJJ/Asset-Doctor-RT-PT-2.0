# Project pulse

## 2026-11-15 — Initial source inventory and independent GPU capability probe

- Source root: E:\MyCreations\Asset Doctor RT-PT 2.0\Main. Parent is the working area.
- Existing remote main starts at dd32635a892bd2fa517c3675ff63550ff11cb425 (README only); cloned without replacing history.
- Legacy: E:\MyCreations\Asset-Doctor, branch work/import-limit-20261115, HEAD 17ca3549767aac78bf4ad51fa1c254c90dd15113, version 2.0.14, clean at initial inspection. Read-only.
- Read the owner specification, selected Arc-XRay findings and source entry points. Source observations are not current native acceptance.
- Rust absent from PATH/user default location. Prepared project-local toolchain; Vulkan loader exists and Windows reports NVIDIA GeForce RTX 5060 Ti.
- Microsoft Build Tools installation was rejected by automatic approval policy; no successful system installation is claimed. Evaluating portable GNU Rust toolchain instead.
- SentinelX reports no enrolled host. Context7 supplied ash 0.38 API documentation.
- Initial local commit: b7e2db3. No push. Continued the owner's existing README history, not an empty/force-reset repository.
- Inventory: 66 curated feature/scenario rows; 10 owner requests; 60 reviewed settings/control rows; 197 lexical UI declaration sites; 110 transient-state candidates; 40 diagnostic ID declaration sites; 32 test-named files. Full semantic UI/native audit is OPEN, not complete. No Legacy code/tests were executed, copied as implementation or modified.
- Read attached advisory architecture v0.1. Accepted independent domain/application/UI/GPU responsibilities; adjusted history/derived diagnostics ownership and Compare placement. See docs/ARCHITECTURE_PROPOSAL_REVIEW.md and ADR-0001..0008. Only one implemented crate; no empty product subsystems.
- Icon original copied byte-for-byte to assets/app.png; multi-size ICO prepared. Window/executable icon resource integration still pending.
- Portable GNU Rust 1.100.0 builds/tests the independent ash probe. libloading 0.8.8 retained in Cargo.lock because 0.8.9 needs a missing GNU assembler in this fallback; reassess on MSVC. No global PATH/default Rust changed.
- Checks passed locally on portable GNU: cargo fmt, clippy (all targets, warnings denied), cargo test (1 unit test), and executable compile/run. Test initially failed on unimplemented SurfaceSize::drawable, then passed. No claim of broad test coverage.
- Actual probe result: NVIDIA GeForce RTX 5060 Ti; Vulkan 1.4.357; driver_raw 2604892160; device-local heap 16,829,644,800 bytes; system heap 17,140,611,072 bytes. Swapchain/acceleration_structure/ray_tracing_pipeline/ray_query extension presence true. Presence is not enabled RT support or free VRAM.
- Probe created loader/instance and enumerated physical devices/queues, then cleanly exited. It did NOT create a logical device, native window, swapchain or presented frame. Validation layers were absent at this probe run: validation_enabled=false; validation acceptance UNVERIFIED.
- Windows MSVC CI and cargo-deny policy prepared, not run remotely. Local cargo-deny 0.20.2 installation failed because its transitive raw-DLL imports need GNU assembler tooling absent from the portable fallback. No license/advisory PASS claimed; rerun with completed MSVC setup. Product license not selected.
- Owner reports Vulkan SDK 1.4.363.1 installed; Visual Studio Build Tools installation at 36%. Await explicit «Готово» before checking newly installed environment. Do not launch another system installer.
- Next single native increment after tool readiness: native window + ash device/swapchain clear frame + resize/minimize/restore/close + supplied icon, with actual validation-layer evidence. Separately continue closing audit UI/settings/P0 native gaps before product-domain operations. No asset importer, PBR, Heal, Rig, RT or PT implemented.
