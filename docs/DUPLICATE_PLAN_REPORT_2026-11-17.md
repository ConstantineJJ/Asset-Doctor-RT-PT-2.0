# Complete duplicate-removal plan — 2026-11-17

Feature checkpoint: `dbd6091fb40c768117fbf550388172cfb30951fb`.
Internal CPU planning/command preparation only; desktop still has no Heal/Save
controls. [ADR-0030](DECISIONS/ADR-0030-duplicate-removal-plan.md) and
[slice15 audit](../reference/SLICE_15_DUPLICATE_HEAL_AUDIT.md) define this subset.

## Implemented

New ad-heal depends only on local document/commands at runtime. It scans one
explicit source MeshId in full, independent of partial diagnostic reports and
their16sample cap. Cyclic rotations of the same source indices are equivalent;
opposite winding and coincident positions with different indices stay distinct.
The earliest ordinal survives, with its original three indices and position in
the surviving sequence. Repeated-index duplicates retain one representative;
this is not degenerate-triangle repair or vertex welding.

Preview contains source scope, before/after counts, complete removal ordinals
and unique source-node references including inactive nodes. Shared primitive-ID
lists are grouped by identity and scanned once. A source with no node reference
is a valid explicit scope. Reference count is not currently visible instance
count. Every shared instance uses the same changed source data.

Private origin/revision binds a plan to its document. Preparation uses existing
Session history admission and a new immutable, origin/revision-checked
PreparedRemoval::indices_after view. Independent verification rebuilds source
and candidate keys, checks exact deletion-only output, full key coverage,
candidate uniqueness and earliest original survivor. Both live verification
arrays count against the same scratch cap. A no-op aliases current indices;
uniqueness then relies on the private complete planner, with no second key build.

Prototype limits:4million scoped triangles,64MiB planning scratch reused for
node-list descriptors,16MiB retained removal storage,128MiB verification scratch.
Fallible reservation, actual capacities and checked arithmetic refuse before
mutation. Cooperative checks cover scans and surround in-place sorts; sorting
itself is not interruptible. No whole-process peak or latency claim. Generic
apply/Undo/Redo remain in ad-commands; no additional history or GPU dependency.

## Verification

170workspace tests PASS: analysis35 / commands14 / desktop23 / document23 /
formats38 / heal12 / renderer12 / storage13. Formatting, all-target clippy with
warnings denied, locked MSVC release workspace build and cargo-deny all PASS.
Cargo.lock adds only ad-heal; no registry version or policy exception. Formats,
analysis and storage are dev/probe dependencies of heal, not runtime dependencies.

Tests cover40duplicates vs16report samples, interleaved cyclic/reversed/repeated
indices, distinct coincident vertices, primitive isolation, inactive/zero-reference
scope, exact memory boundaries, cancellation inside dense planner/postcheck
groups, stale/foreign equal-tag guards, full-history no-op and refusal, redo
preservation, hostile candidate indices and incorrect earliest-survivor selection.
A256case deterministic oracle uses explicit equivalence classes without production
key functions. Textured tests preserve positions/normals/tangents/texture pointers,
fresh export/reimport, and exact source-copy export after Undo. Initial missing
candidate/planner APIs failed tests before implementation.

Independent source review found no implementation blocker. It clarified the
no-op proof exception and identified that probe/test G002 absence must require
complete diagnostic coverage. Those checks now assert report.is_complete before
absence is treated as success. Source reviewer did not rerun tests/GPU/UI.

New authored Duplicate_Heal.glb derives only from the project's Normal_Checker:
32rotated copies are appended to primitive0. Generator repeats identically; all
21fixture hashes/sizes match the manifest and20old manifest rows/fixtures remain
unchanged. Source Normal_Checker is never rewritten. No owner assets are vendored.

Reproduction after a release build:

```powershell
.\tools\heal-smoke.ps1 -SynchronizationValidation
```

The developer probe performs full plan→verified preparation→Apply→complete fresh
analysis→Undo→Redo→verified export→new-path disk save. Primitive0 changes992→960
triangles,32removed,1source-node reference, revision3,79364verified bytes, no
cleanup warning. Original source is retained. Actual native saved-file readback
shows4source primitives/3840triangles,15PBR/MR/normal frames, damaged-replacement
preservation and resize/minimize/restore/close with validation enabled/0errors.
Existing UV and empty-viewer regressions also pass. Child sync validation was
requested; its effective setting was not independently queried. No pixel-match
or user-facing editing acceptance is claimed. Probe windows closed; no interactive
capture/input session was started. Logs/copies stay ignored under artifacts/.

Previous export/docs checkpoint7a7eb26 passed
[Windows CI run43866128041](https://github.com/ConstantineJJ/Asset-Doctor-RT-PT-2.0/actions/runs/43866128041).
New code's CI is separate from local native evidence.

## Remaining

This plan proves an index transformation, not arbitrary material/extension/export
eligibility. Before native Apply: certify that eligibility and atomically publish
document/history, complete fresh analysis, derived snapshot and candidate GPU
resources. Then add native preview/Apply/Undo/Redo, Save Copy and unsaved-close
protection. General batch repair, other six operations, skin/rig/animation and
opaque changed export remain unfinished. REPAIR-06/HEAL-01 stay NOT_STARTED as
user-facing workflows; their CPU subset is recorded explicitly in the full plan.
