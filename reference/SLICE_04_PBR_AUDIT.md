# Normal/material/studio behavior review — 2026-11-15

Legacy read-only HEAD17ca3549767aac78bf4ad51fa1c254c90dd15113 rechecked, clean.
ShadingMode.ts:59-67 retains authored material/normal splits for hybrid view;
forced smoothing is a separate presentation path. LightingManager provides
presentation lighting; existing inventory VIEW-02/VIEW-04 records source shading,
exposure and key/fill controls. No legacy implementation is copied or executed.

New contracts deliberately preserve authored normals instead of merging vertices
or recomputing every mesh smooth. Fallback is derived viewport data, not a repair.
Metallic/roughness source factors are immutable. Studio controls do not fulfill
REQ-LIGHT-001/002 authoritative editable lamp/gizmo/context-menu requirements.

PBR preview is distinct from the accepted base-color view. Missing full material
maps/environment/culling parity is explicitly labelled in ADR-0016 and UI notes.
No new Heal/Rig/animation/export behavior or parity claim is introduced.
