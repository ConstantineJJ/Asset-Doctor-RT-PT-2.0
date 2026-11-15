# Owner requests

These ten requests come from Get Starting §2.4, not proof of Legacy implementation. Current user clarification: carry functionality only; old architecture is rejected.

## REQ-IMP-001 — Increase import admission; decoded RAM/VRAM checks

REQUESTED / P0. Source cap already 192 MiB; decoded admission not complete. Evidence `src/performance/MemoryBudget.ts:4`. Native UNVERIFIED.

Acceptance: Boundary source+decoded budgets and cancel keep old document

## REQ-LIGHT-001 — Light is editable scene entity with attached gizmo

REQUESTED / P0. Legacy helper exists; requested authoritative entity behavior unverified. Evidence `src/viewer/LightingManager.ts:16`. Native UNVERIFIED.

Acceptance: Drag light in local/world without jumps or camera motion

## REQ-LIGHT-002 — Light context menu/color/temperature/enable

REQUESTED / P0. Partial lighting controls; full requested menu/temperature not established. Evidence `src/viewer/LightingManager.ts:13`. Native UNVERIFIED.

Acceptance: All displayed controls work, serialize and undo

## REQ-INPUT-001 — Rebind left/right/side mouse buttons

REQUESTED / P0. Keyboard-only binding schema found. Evidence `src/platform/Shortcuts.ts:22`. Native UNVERIFIED.

Acceptance: Mouse namespace conflicts/reset and no Back/Forward

## REQ-SESSION-001 — Automatic single at 1, Compare at 2+ with shared settings

REQUESTED / P0. Manual separate Compare found. Evidence `src/components/Viewport.tsx:67`. Native UNVERIFIED.

Acceptance: 1→2→1 keeps edits/selection/jobs/export correct

## REQ-GIZMO-001 — Model translate/scale gizmo; remove autorotation

REQUESTED / P0. Legacy turntable exists; requested model edit contract not established. Evidence `src/App.tsx:212`. Native UNVERIFIED.

Acceptance: Move/scale/undo/export; no automatic rotation

## REQ-TREE-001 — Start scene tree collapsed

REQUESTED / P0. Current source false (expanded). Evidence `src/App.tsx:213`. Native UNVERIFIED.

Acceptance: Fresh start collapsed, expand on request

## REQ-ANIM-001 — Autoplay imported animations

REQUESTED / P0. Autoplay branch found in current source; native unverified. Evidence `src/App.tsx:600`. Native UNVERIFIED.

Acceptance: Animated clip starts; static model remains safe; edit selection controlled

## REQ-FLOOR-001 — Distinct themes and no floor duplicates

REQUESTED / P0. Shared catalog exists; visual distinctness/native unverified. Evidence `src/viewer/SurfaceCatalog.ts:14`. Native UNVERIFIED.

Acceptance: Cycle themes twice; one floor, unique labels, persist choice

## REQ-ENV-001 — Environment toggle independent of lamps

REQUESTED / P0. Lighting graph found; independent environment contract not established. Evidence `src/viewer/LightingManager.ts:7`. Native UNVERIFIED.

Acceptance: Toggle environment without changing manual lamp states
