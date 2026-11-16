# ADR-0026 — Static triangle commands and bounded linear history

Status: ACCEPTED, 2026-11-16, for internal CPU command foundation only.
Extends ADR-0002/0005 static subset. Does not approve a user-facing Heal or writer.

## Scope and ownership

Introduce ad-commands, depending only on ad-document at runtime. It owns one
AssetDocument and one linear history; read access cannot mutate document parts.
Creation requires a fresh revision0import and rejects nonzero revisions before
history allocation. It cannot relabel an already edited document as pristine,
even if a standalone exchange restored earlier content. Constructor failure
consumes its candidate input; desktop integration must retain the current asset
until a newly imported candidate session has been admitted successfully.
No UI/GPU/IO or repair algorithms enter this crate. No external packages added.
The first supported command removes an explicit sorted unique list of current
triangle ordinals from one source MeshId. It preserves all vertex attributes,
materials, nodes, primitive IDs and original source bytes. Mesh/Node IDs stay
stable because no entities/vertices are added, deleted or reordered in this subset.
Triangle ordinals are revision-local, not persistent IDs across edits.

Source primitives may be shared by multiple scene instances or inactive; the
command targets the primitive and affects all uses. Future UI preview must make
that scope explicit. This is not a per-instance geometry clone or automatic repair.
Removing every triangle is explicitly unsupported until empty-primitive/scene
contracts exist. Empty removal is a no-op, never clears redo or changes revision.

## Preparation and commit

DocumentTag carries ID and expected revision. Preparation checks tag, target,
sorted/unique/range and nonempty-result invariants before constructing a candidate
index Vec with fallible reserve/cancellation. Source data is untouched. An opaque
IndexExchange additionally retains a private document-origin token, so a prepared
change cannot be used on another document even if a caller reused its public ID.
Source Vec ownership moves at commit by swap; positions/normals/UV/images are
never cloned for history. Only the validated command exchange may mutate indices.

Commit checks current tag, origin, cancellation, index/decoded limits, history
admission and revision overflow before any mutation. Successful Apply/Undo/Redo
each increase revision monotonically; snapshots carry that revision. Returning
to the same content through Undo does not restore the old revision or revive
old diagnostics/jobs/previews. All fallible work precedes the swap/state update.

This CPU commit is not desktop publication. Future app integration must prepare
and verify candidate snapshots/analysis/GPU upload and only publish the complete
transaction, with clear failure/rollback behavior. No UI edit button before that
contract, preview and safe export acceptance are complete.

## History and resources

One preallocated Vec of entries plus cursor represents Undo and Redo. New commit
after Undo discards the redo suffix only after successful admission and exchange;
failed/cancelled/no-op commands leave it intact. No eviction in this first subset:
when full, reject before mutation. Undo/Redo allocate no index/geometry buffers.

Prototype defaults:64MiB retained history /128entries, configurable and separate
from ImportLimits. Account actual entry-vector capacity×entry size plus a fixed
reservation per entry equal to max(before index capacity, candidate capacity)×4.
The reservation remains valid as the alternate Vec swaps between Undo/Redo.
Check arithmetic/capacity before mutation. Count redo payload in retained bytes;
on a successful branch only the retained prefix and new payload must fit.
Candidate preparation is additionally bounded by decoded ImportLimits, with a
history preflight before a nontrivial copy. These are accounted buffer bounds,
not allocator overhead or whole-process/VRAM peak guarantees. Multiple pending
commands are caller-owned; future app scheduling must bound outstanding work.

Track imported-content state separately from monotonically increasing revision.
is_modified means current history state differs from import baseline; this is
not full unsaved-close/persistence state. No API may claim a disk save succeeded
before a verified writer exists. Original bytes remain the import provenance.

## Acceptance

Preview/no-op immutability; exact apply/Undo/Redo arrays and pointer reuse;
monotonic revisions; old report/snapshot/prepared-command invalidation; foreign
document rejection including identical public IDs; wrong target/invalid triangle
lists/all-face refusal; cancellation before and during copying; decoded/history/
entry/revision overflow refusal without mutation; branch/no-op/failed branch
semantics; capacities and source sharing; multi-primitive/instance scope.
Existing viewer/import/diagnostics and native lifecycle must stay working.

General commands, geometry remap, gestures, rig/animation, reset original, history
eviction, project persistence, background command workers, native dirty protection,
safe file export and end-to-end Heal remain future contracts/increments.
