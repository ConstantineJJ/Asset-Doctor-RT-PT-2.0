# Product specification — first baseline

Asset Doctor RT-PT 2.0 is a new native Windows x64 tool for inspecting, diagnosing,
repairing, editing, comparing and exporting 3D assets. Rust owns product state;
Vulkan renders local GPU data. Source root: Main; parent: working area.

## Confirmed scope

Carry user functions and useful behavior from Legacy, never its architecture or
implementation. Legacy remains untouched. Its React/Three.js/Tauri code is not a
dependency, scaffold, porting template or source of runtime types.

The requirement registry is FEATURE_PARITY_MATRIX.csv: 66 initial source-observed
feature/scenario rows + 10 owner-requested rows. Source observation is separate
from runtime/native verification. The full audit gate is still open; see
reference/LEGACY_GAP_REGISTER.md. Native source-site and state indexes are not
complete semantic UI/settings acceptance.

The ten owner requirements cover import resource admission, editable lamps with
context settings, side-button camera bindings, automatic 1/2+ single/Compare,
model translation/scale gizmos without autorotation, initially collapsed tree,
animation autoplay, distinct floors and independent environment toggle.

## Implementation order

1. Inventory and contracts; independent toolchain/GPU experiment may run alongside.
2. GLB/glTF → independent document → native viewport, then raster/PBR.
3. Diagnostics, verified repair, manual rig/animation, revisioned Undo/Redo,
   automatic Compare, settings, verified export and recovery.
4. Stable native raster release and installer.
5. Hardware RT, then progressive PT, then Mixamo rights review/autorig/retargeting.

## Initial success criteria

- Original file never changes from preview/import; failed jobs keep current data.
- Commands validate document identity and expected revision before committing.
- Export means verified roundtrip plus confirmed disk write; cancel is not success.
- Errors, cancellation, memory refusal and late results are reproducible tests.
- Native/GPU verification is reported separately from compilation and unit tests.
- No product feature is claimed available because its backlog row exists.

## Current implementation

Two independently scoped crates: ash renderer experiment and native desktop
window/event integration. A release EXE presents a clear frame and passes
validation-enabled resize/minimize/restore/close smoke. Six presentation-policy
tests pass. No asset import, repair, rig, export, full product UI, PBR, RT or PT
is implemented yet.
Application name includes 2.0; engineering package starts at 0.1.0 to avoid
confusing this new prototype with Legacy 2.0.14 or a released product.
