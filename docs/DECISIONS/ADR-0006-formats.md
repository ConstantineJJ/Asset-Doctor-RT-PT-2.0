# ADR-0006 — GLB/glTF strategy

Status: OPEN; parser/encoder not selected or installed.

Choose maintained licensed parser/validator rather than implementing compression.
Read GLB and embedded glTF first. External resource grants, Draco/Meshopt/KTX2
and source-preserving edits require explicit capability tables and fixtures.
Never infer export fidelity from display support. Unknown required extension:
explicit failure; optional opaque data: preserve or block unsupported edit.

Export gate: source untouched, selected revision still current, independent
reimport validation, expected structural/semantic diff, then atomic destination
write with cancellation/failure semantics. Verified bytes are not a saved file.
Do not reproduce Legacy's separate Heal/rig patch architectures by imitation.
