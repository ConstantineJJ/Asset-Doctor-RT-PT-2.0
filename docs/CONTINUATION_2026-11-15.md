# Development continuation checkpoint

Historical checkpoint. Current: [2026-11-16](CONTINUATION_2026-11-16.md).

Owner authorizes autonomous development and computer use without manual acceptance.
No push/release authorized. Source models, Legacy and supplied Arc-XRay are read-only.

Current local work:
- `6f759da`: frame/material binding redesign,96byte frameUBO/64byte push,2sets,
  deduplicated material tuple cap4096.
- `cff6693`: core emission SRGB image/factor/independent UV, strict admission and
  independent PBR switch; no bloom/GI/environment or emission-strength extension.
- `79cbef8`: independent ad-analysis with robust adaptive zero-area predicate,
  oriented same-index duplicates, unused vertices and missing/zero normals.
- `95a957a`: worker report plus native Diagnostics window and known-defect fixture.

Latest code gates:72tests (5analysis/17desktop/14document/24formats/12renderer),
fmt, all-target clippy with warnings denied, Windows release build and cargo-deny
advisories/bans/licenses/sources PASS. New robust1.2.0(MIT OR Apache-2.0) has no
transitive dependency. Current EXE: target/release/AssetDoctorRTPT.exe.

Native evidence: emission on/off/default, normal/AO/MASK/empty regressions and
geometry fixture lifecycle/rejected replacement all0validationerrors. Geometry
fixture has6source triangles/6findings versus11displayed triangles with instances.
Interactive Diagnostics top/bottom scroll/footer fit rechecked at800x540 after
fixing row minimum height/truncation. Final visual app closed normally0errors;
capture session reset. No test app is left open.

Heavy Vanguard:1559826source triangles/852516display vertices,1source primitive,
0findings under current5rules in77ms,4images/223696208mipbytes. Full lifecycle and
bad replacement0validationerrors; source hash unchanged. Timing is one local
observation, not a benchmark. Logs ignored under artifacts; durable scope/evidence
reports are EMISSION_REPORT and GEOMETRY_DIAGNOSTICS_REPORT dated2026-11-15.

Next: review ADR-0023 proposal and settle edge-incidence semantics plus per-check
coverage before implementation. Add source-authored fixtures/tests, then native
verification. Avoid an expensive-rule refusal erasing completed cheap checks.
No repairs until mutation/history/export contracts are accepted and verified.
Source review recommendations are appended to ADR-0023: exclude repeated-index
faces, include distinct-index collinear faces, count unique source-triangle
incidents per primitive, retain bounded typed edge pairs and independent coverage.
All16fixture hashes/sizes/manifest entries verified at this checkpoint.

Open limits: full topology/UV/material/rig/profile diagnostics, independent rescan,
editing/revision invalidation, calibrated pixel/Mikk/LOD equivalence, peak memory,
allocation/device-loss injection, full DPI/accessibility matrix. Whole-process
peak memory is not capped by existing admission budgets. Sort phases are not
interruptible internally; cancellation checks surround them.
