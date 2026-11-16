# Slice14 — Static export/source fidelity and save behavior

2026-11-16, read-only Legacy source observation only. RepairedExportService.ts:
127–185 guards source structure, repair verification and target association before
export. Its source/runtime scene mapping and separate rig export are not copied.
platform/FileIO.ts:78–107 distinguishes completed native writes from cancelled
dialogs/unconfirmed browser downloads. Native old Save/fault/roundtrip scenarios
remain unverified; no Legacy code/tests/EXE executed or modified.

New independent ADR-0028 format contract supports a source-index-only subset,
preserves all original source primitives and core metadata instead of projecting
visible draws, and independently compares the entire reimported document.
Unknown metadata/chunks are exact-copied or block changed output. This is not
certification of every extension/operation combination or automatic Heal approval.

ADR-0029 separates verified memory bytes from confirmed new-path disk save.
No original overwrite, silent partial destination or false success after Save
cancel. Source/metadata proof, source BIN preservation, maps/instances/inactive
data, stale/foreign identity and IO failures need explicit tests. Native dialog/
edit-publication and full first-Heal contracts remain open in ADR-0027.
