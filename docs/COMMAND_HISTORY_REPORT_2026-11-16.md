# CPU command and history foundation — 2026-11-16

Code checkpoint: `9a8e454ac81b18e850af72401e3fea1ab19d0fa3`.
Accepted internal subset: ADR-0026. First-Heal/static-export ADR-0027 is PROPOSED.
This is a working CPU module, not a user-facing Heal or desktop Undo acceptance.

## Implemented

- New local ad-commands crate owns one fresh-import document and one linear
  history. Its only runtime dependency is ad-document; no GPU/UI/file IO or
  automatic repair planner. Test-only analysis/formats dependencies reuse the
  existing fixture stack; Cargo.lock adds one local package, no registry changes.
- Preparation takes expected document/revision, source MeshId and explicit sorted
  unique triangle ordinals. Validates range and nonempty result, constructs only
  a candidate index Vec with cooperative cancellation, and does not mutate data.
  Removing all faces is unsupported; empty removal is a true no-op.
- Opaque proposal stores private document-origin identity in addition to public
  ID/revision, preventing cross-document use even with accidentally reused IDs.
  Commit checks origin/tag/cancel/limits/overflow before mutation. Index Vec swap
  leaves positions/normals/UV/materials/nodes/images/original bytes untouched.
- Every Apply/Undo/Redo increases revision. Current snapshots carry it; old
  analysis never matches again just because Undo restored the old geometry.
  Stable primitive/node IDs survive because this subset never reorders entities.
  One source primitive edit affects all instances, including inactive data scope.
- History defaults64MiB/128entries. Entry vector allocated before use; actual
  capacity×entry size charged. Each entry reserves max index-buffer capacities×4,
  valid through alternating swaps. Redo payload remains counted until a successful
  new branch admits prefix+new and discards it. No eviction; full budgets refuse
  without mutation. Undo/Redo allocate no geometry/index buffers.
- No-op, failed, cancelled or stale commands preserve cursor/redo/content. Imported
  content state is separate from monotonic revision. is_modified is relative to
  import, not a confirmed disk-save/unsaved-close status. Session creation rejects
  nonzero revisions before allocation, preventing edited data being relabelled
  pristine. Constructor consumes its candidate on failure; future desktop keeps
  the current asset until a newly imported candidate session is admitted.

## Verification

- 133 workspace tests PASS:35analysis/14commands/23desktop/22document/27formats/
  12renderer. New API contract tests failed before implementation and passed after.
- Exact apply/Undo/Redo arrays, pointer reuse and unchanged source/attribute/image
  sharing; fresh diagnostic changes; instanced snapshot11→9triangles; stale report
  invalidation even after exact Undo; invalid targets/lists/all-face/no-op cases.
- Decoded limits include sourceUV and inactive primitives; exact budget/one-byte
  refusal, revision/state overflow, foreign origin with identical public tags,
  cancellation before/during candidate copying and before Undo/Redo.
- Exact retained byte budget, entry count/no history, impossible allocation,
  full-history direct-plan bypass refusal, interleaved two-mesh full Undo/Redo,
  and byte-limited branch tests. Rejected prepare/direct commit retains redo;
  successful branch fits by reclaiming only its suffix. No-op does not clear redo.
- Source review identified constructor baseline relabelling: test reproduced RED,
  nonzero revision guard fixed GREEN. Review's interleaving and byte-branch coverage
  gaps closed. Final independent source-only review found no remaining blocker;
  reviewer did not rerun native checks.
- fmt check, all-target clippy warnings denied, Windows workspace release build,
  cargo-deny advisories/bans/licenses/sources PASS. Native existing UV/normal-PBR/
  empty regressions: applicable import/rejected replacement and lifecycle,15presents,
  0validationerrors. These are viewer regressions, not command UI acceptance.
  Sync validation requested; effective setting not separately queried. No capture
  or new input automation; test viewers closed. Evidence artifacts/commands-*.

## Boundaries and next step

Low-level document exchanges are for an ordered command/history caller; Session
enforces ordering with a private cursor and no mutable document getter. Numerical
accounting bounds retained payload/metadata, not whole-process peaks or allocator
overhead. Pending proposals are caller-owned; application must bound outstanding
work. Candidate preparation is synchronous CPU work here; no UI-thread invocation
or background command scheduler has been added.

No product repair-set selection, material/extension eligibility, preview/apply UI,
transactional GPU publication, native Undo/Redo shortcuts, reset/eviction, project
history, writer/Save Copy or unsaved-close integration. No saved-file success API.
HEAL/RIG/REPAIR/EXP parity rows remain unimplemented. No user/Legacy/Arc-XRay asset
or source changed; original bytes are never written by these modules.

Next: close ADR-0027source mapping, metadata/extension fidelity, independent static
roundtrip/write verification and desktop candidate-publication behavior before
exposing the first safe Heal workflow. Full roadmap: MASTER_PLAN_AND_STATUS_RU.md.
