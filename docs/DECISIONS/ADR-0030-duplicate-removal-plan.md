# ADR-0030 — Complete oriented duplicate-removal plans

Status: ACCEPTED, 2026-11-17, internal CPU planning/command preparation only.
This closes one part of ADR-0027, not native Heal/Save publication.

Introduce ad-heal as a small application-policy boundary over ad-document and
ad-commands. No renderer, UI, filesystem, format parser or diagnostic-report
dependency. Runtime dependencies are local only. A read-only plan scans one
explicit source MeshId in full; it never consumes the diagnostic report's bounded
samples or treats a partial report as a complete repair set.

Same three source vertex indices, up to cyclic rotation, define a duplicate.
Reverse winding remains distinct unless repeated indices make the rotations
identical. Coincident positions with different indices never merge. Keep the
lowest original triangle ordinal for each key, and retain all survivors byte-for-
byte in original order. No welding, normal/attribute/UV/material edits or cross-
primitive deduplication. Repeated-index/zero-area triangles are not repaired by
this operation; exact repeated copies may be removed while one remains.

Plan stores a private document-origin/revision stamp, selected source primitive,
before/after counts and the complete sorted removal ordinals. Preview reports
all unique source NodeIds referencing it, including inactive nodes, separately from displayed
instances. A zero-reference primitive is a valid explicit scope. Never imply that
only one visible instance changes. No duplicates produces a successful no-op.

Use bounded in-place sorted key/ordinal records, fallible reservation and checks
around sort and every1024items in scans. Sorting itself is not interruptible.
Default limits:4million scoped triangles,64MiB planner scratch,16MiB retained
removal storage. Admission uses checked arithmetic and actual Vec capacities;
no partial plans after cancellation/resource/allocation failure. These are phase
budgets, not whole-process memory promises. Plan header/stack data is small fixed
overhead; callers must bound outstanding plans.
Reference counting runs after planner records retire, using the same scratch
budget for borrowed node-list descriptors. Group shared list identities and scan
each once; never rescan a huge shared primitive-ID list for every instance.

Preparing a command rechecks private origin/revision against Session, uses its
existing history/document admission and exposes only immutable candidate indices
from PreparedRemoval after the same origin/revision checks. Independent postcheck
must finish before handing out the command: exact streaming deletion-only diff,
full source/candidate oriented-key coverage, no duplicate candidate keys, and each
group's earliest source ordinal retained. Rebuild/sort verification data from
source and candidate independently of the planner records/removal discovery.
Default verification scratch128MiB. Refusal/cancel leaves document/history/redo
unchanged. Generic apply/Undo/Redo stay in ad-commands; no second history.
Exception: an empty plan exposes the identical current index slice and needs no
verification scratch. Uniqueness in this no-op path relies on the private complete
planner, rather than independently rebuilding a second key set. Identity/revision
and cancellation checks still run, including full-history no-op cases.

This certifies the index/topology transformation only. Before product Apply,
the application must separately certify material/extension/source export
eligibility, independent result analysis, and atomic history/document/analysis/
snapshot/GPU publication. Existing static core export can certify authored test
results, not arbitrary opaque metadata or rig combinations. Removing duplicates
can legitimately expose boundary-edge findings hidden by repeated incidence;
incidence-count changes alone are not a newly created surface hole.

Acceptance: more than16duplicates, rotations/reversed winding/repeated indices,
different positions/indices and primitive boundaries, inactive/shared references,
deterministic earliest survivor, exact limits/no-op/invalid scope, mid-scan cancel,
stale/foreign same-tag refusal, hostile candidate postcheck failure, Session
commit/Undo/Redo, fresh analysis and static export/reimport of authored fixtures.
