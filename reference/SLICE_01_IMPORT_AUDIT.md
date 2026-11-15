# Static GLB slice: reviewed behavior and bounded gate

2026-11-15. Legacy HEAD rechecked: 17ca3549767aac78bf4ad51fa1c254c90dd15113,
clean. Read-only source review; no Legacy code executed or reused.

| Feature | Evidence in Legacy | New acceptance |
|---|---|---|
| Open/drop | TopToolbar.tsx:129,167; Viewport.tsx:109 | Ctrl+O native picker and GLB drop; no source writes |
| File admission | platform/FileIO.ts:34; performance/MemoryBudget.ts:4 | Source, JSON and decoded/snapshot budgets before expensive work |
| Self-contained files | loaders/GLBLoaderService.ts:145 | No network/sibling reads; reject external URIs |
| Immutable source | loaders/GLBLoaderService.ts:129 | Keep original Arc bytes separately from decoded data |
| Latest load wins | App.tsx:700; session/AssetSession.ts:7 | Cancellation + ticket guard; stale completion cannot replace current document |
| Error leaves current model | App.tsx:704–731 | Corrupt/unsupported/budget/cancel failure never publishes partial data |
| Authored transforms | viewer/AssetPresentation.ts:22 | Independent hierarchy/local matrices; camera fit changes presentation only |
| Camera fit/orbit | viewer/CameraController.ts:11,25 | Fit, orbit, pan, zoom; no autorotation; no authored-data mutation |

The existing 66-feature catalog and 10 owner requirements remain authoritative
for eventual scope. This closes source/contract questions for a NEW static-asset
foundation slice only. Full UI/native parity, Heal/Rig/animation/export/Compare
gates remain open. Unverified Legacy native behavior is not treated as PASS.

Initial compatibility: GLB 2 with BIN positions/triangle indices, selected/default
scene, multiple nodes/primitives and base-color factors. Static meshes only.
Reject skins, animation, morph targets, required extensions, nontriangle modes
and external resources explicitly. Embedded textures/optional metadata are
preserved in source bytes but not displayed or exported by this slice. This is
partial import support, not general GLB/glTF parity.
