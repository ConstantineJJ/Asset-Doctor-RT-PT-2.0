# ADR-0027 — First safe Heal and static export

Status: PROPOSED, 2026-11-16. No user operation or writer approved by this proposal.

Target first complete workflow: oriented same-index duplicate findingG002 →
read-only preview of full removal set → explicit apply → independent postcheck →
Undo/Redo → Save Copy → fresh reimport and structural verification. Never use
the report's bounded16examples as the complete repair set.

ADR-0026 supplies only the generic index command/history foundation. Duplicate
detection remains independent analysis; a Heal planner must prove source-operation
eligibility, full scope (including shared primitive instances), unchanged source
revision and exact removal-only diff. Reversed winding is not the same duplicate.
BLEND, skin/morph/animation and unknown extension/metadata interactions require
explicit certification or refusal, not a silent fallback.

Before a writer implementation, decide source-primitive/accessor provenance,
supported optional extras/extensions, index-only BIN patch vs reconstruction,
independent readback tolerances and unknown-data preservation. Static view support
is not export fidelity. Do not copy Legacy's source-patch/export architecture.

Native publication must handle snapshot/analysis/upload failure atomically while
retaining current scene/history. Export must be bound to the selected revision,
verify candidate bytes, write safely to a distinct destination, handle Save cancel/
write failure/stale state and independently validate the saved file. A verified
memory buffer is not a confirmed disk save. No silent original overwrite.

Needed fixtures: nested/mirrored transforms, instances/inactive primitives,
multiple materials/maps/UV/color/tangent attributes, integer/nonindexed source
indices, optional metadata/extensions, malformed/cancel/limits/stale cases,
file-write refusal, and combination/roundtrip checks. No user assets vendored.
Close these choices/tests before exposing a production Heal button.
