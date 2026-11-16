# Slice13 — Command/history prerequisites for index-only operations

2026-11-16, read-only Legacy source observation, not a native acceptance pass.
Legacy RepairRegistry.ts:103–120 defines oriented duplicate-triangle removal as
conditional, preview/apply/undo/verify with an index-only export capability.
DuplicateTriangleRepair.ts:1–54 exposes oriented index matching and material
eligibility checks. SurgicalHealEngine.ts:1015–1085 checks stale preview/shared
geometry/history budget and retains restore data before postchecking. Its
undoLast entry is at1148. These are behavioral leads only; no implementation,
Three.js graph, worker protocol or export architecture is copied/executed.

New design keeps authoritative CPU data independent of renderer and history.
ADR-0026 approves only explicit triangle-removal commands with revision/origin
guards and retained-index ownership exchange. It does not select a repair set,
certify materials/metadata/export or bypass preview/verification requirements.
Shared source primitive edits affect all instances explicitly; future UI must
expose scope rather than silently fork or repair an instance.

ADR-0027 records the remaining first-Heal/writer decisions and acceptance matrix.
HEAL-01/02/03, REPAIR-06 and EXP-01 remain NOT_STARTED until their actual product
workflow exists. CPU tests of the generic command are not native Heal acceptance.
