# Product specification — first baseline

Asset Doctor RT-PT 2.0 is a new native Windows x64 tool for inspecting, diagnosing,
repairing, editing, comparing and exporting 3D assets. Rust owns product state;
Vulkan renders local GPU data. Source root: Main; parent: working area.

## Confirmed scope

The consolidated original roadmap and current status of all76catalog/request
rows are maintained in [MASTER_PLAN_AND_STATUS_RU.md](MASTER_PLAN_AND_STATUS_RU.md).
It distinguishes working subsets, remaining functions and acceptance criteria.

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

Five independent crates now implement immutable static document data, bounded GLB
formats, ash Vulkan presentation, source analysis and a native viewer shell. Embedded PNG/JPEG
base-color maps, selected UVs, linear vertex colors and OPAQUE/MASK rendering are
implemented in ADR-0015. ADR-0016 adds authored normal preservation and optional
direct PBR with metallic/roughness factors, exposure and approximate fill. ADR-0017
adds linear metallic/roughness maps with independent UV selection and color-space
aware image sharing/admission. ADR-0018 adds normal maps with source tangent/scale
preservation and explicit non-Mikk fallback. ADR-0019 adds linear source occlusion
only to approximate fill. ADR-0020 separates frame/material bindings and ADR-0021
adds core emission with SRGB maps. ADR-0022 adds bounded source geometry findings in independent ad-analysis.
ADR-0023 adds indexed edge incidence and per-rule coverage, with algorithm3 typed
edge evidence. ADR-0024 adds vertex-index connected components and algorithm4
typed component summaries. ADR-0025's data increment preserves all declared source
UV channels (up to8), including unused ones, with explicit material role IDs and
decoded admission. G009–G011 remain planned. Current code615cbff has99tests;
native static/textured/PBR/MR/normal/AO import,
rejected replacement and resize/minimize/restore/close smoke pass with validation.
Read-only diagnostics currently cover eight source geometry rules only.
Full UI/parity, topology/UV/material/profile diagnostics, repair, rig/animation, export, Mikk fidelity,
environment/shadows and RT/PT remain open. Current evidence and limits are in
COMPONENTS_REPORT_2026-11-16.md. Development/self-verification continues
autonomously under the owner's instruction, without per-increment owner acceptance.
Application name includes 2.0; engineering package starts at 0.1.0 to avoid
confusing this new prototype with Legacy 2.0.14 or a released product.
