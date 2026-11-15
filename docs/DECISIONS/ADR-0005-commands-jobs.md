# ADR-0005 — Commands, history and jobs

Status: PROPOSED; gate before edits.

Commands carry document ID and expected revision. Validate entirely before atomic
commit; revision always increases, including Undo/Redo. One pointer gesture is one
history entry. New commit after Undo clears redo branch. History admission occurs
before mutation; baseline ownership and eviction need explicit memory policy.

Jobs consume immutable snapshots, carry document/revision/job identity and return
completed/cancelled/failed, structured errors, progress and resource measurements.
Cancelled or stale results cannot publish, including after remove/re-add asset.
Use typed results rather than hidden globals or fallback heavy work on UI thread.

Required tests: stale result, cancel before commit, budget rejection atomicity,
gesture abort, interleaved Heal/rig history and export invalidation.
