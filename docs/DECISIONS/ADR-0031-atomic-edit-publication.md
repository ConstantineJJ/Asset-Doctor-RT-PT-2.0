# ADR-0031 — Atomic desktop publication of index edits

Status: ACCEPTED, 2026-11-17, protocol and owner decisions only. Owner approved
the stage plan A–D and decisions 1–7 below. No code is changed by this ADR;
implementation follows in stages B1–B4 (publication), C (eligibility/G002 UI)
and D (Save Copy/dirty/close). Closes the publication part of ADR-0027 once
B1–B4 are verified; source/material eligibility and Save UI stay open until C/D.

## Evidence from code (audit at 24f9fc6)

- `Session::commit` truncates the redo suffix after a successful exchange
  (`ad-commands/src/lib.rs:183`). Every Undo/Redo exchange increases revision
  (`ad-document/src/indices.rs:305`). Therefore "commit → GPU failure → Undo"
  is not a rollback: revision changes and redo may already be lost. Rejected.
- `Renderer::set_scene` already performs all fallible work (`GpuScene::new`,
  `wait_idle`) before `self.scene = Some(candidate)`; old scene drop is
  infallible. Its only defect is composition: `Session::commit` is itself
  fallible (tag/cancel, history admission, decoded budget, revision overflow).
  Neither "set_scene → commit" nor "commit → set_scene" is atomic.
- Desktop owns `current: Option<PreparedAsset>` with a bare `AssetDocument`;
  it has no `Session` and no dependency on commands/heal/storage.
- `GeometrySnapshot` and `ad_analysis::analyze` read only current
  `mesh.indices`; there is no way to derive a candidate without mutation.
- `GpuScene::new` re-uploads every texture (`scene.rs:202`). Candidate upload
  costs the full scene, not only changed indices.
- `Report::matches` compares public ID/revision/algorithm only.

## State owners

| State | Owner after this ADR |
|---|---|
| Document, indices, revision, origin | `Session` (unchanged) |
| Undo/Redo cursor and payload | `Session` (single history, unchanged) |
| Snapshot + analysis report + stamp | one desktop `Generation` value |
| GPU scene and its tag | `Renderer` |
| Background edit job, ticket, cancel | desktop edit worker (like `Imports`) |
| Saved receipt / dirty | desktop, from `ad-storage::SavedCopy` (stage D) |

Desktop holds `Arc<Session>`. Workers receive a clone for read-only work and
drop it before posting a result. Only the UI thread mutates, through
`Arc::get_mut`. `Session` contains no `Rc`/`Cell`/raw pointers; B4 adds a
compile-time `Send + Sync` assertion.

## Protocol

Preparation (may fail, mutates nothing). On the edit worker:
1. `plan_duplicates` over the full source primitive (no report samples).
2. Changed-export eligibility preflight (stage C).
3. `DuplicatePlan::prepare` → verified `PreparedRemoval`.
4. Candidate view: `(MeshId, indices_after, revision + 1)`.
5. Candidate snapshot and full analysis from that view, tagged revision + 1.
On the UI thread, when the result arrives:
6. Latest ticket only; plan stamp must still be current.
7. `Arc::get_mut(&mut session)`; refuse as busy if another clone exists.
8. `session.stage_commit(prepared, cancel)`: every Session/document check,
   no mutation. Returns `StagedEdit<'_>` holding `&mut Session`. Its
   resulting tag must equal the candidate snapshot/report tag, else refuse.
9. `renderer.stage_scene(&snapshot)`: `GpuScene::new` + `wait_idle`.
   Returns `StagedScene<'_>` holding `&mut Renderer`.

Publication point (no `Result`, no allocation, no event processing between):
10. `StagedEdit::publish()` — index swap, revision + 1, redo truncate, history
    push into reserved capacity.
11. `StagedScene::activate()` — scene assignment; old scene dropped after the
    idle wait of step 9.
12. Replace the whole `Generation` (snapshot, report, new `document.stamp()`)
    with one move; update UI state. Logs only after these assignments.

The two `&mut` borrows make it impossible to draw a frame, submit GPU work or
touch the Session between check and publish. Cancellation is consulted up to
step 9; after step 10 a late cancel cannot turn success into failure. Any
failure in 1–9 drops candidates; document, revision, history/redo, generation,
GPU scene and dirty state stay exactly as before.

Undo/Redo use the same steps with `Session::undo_target()/redo_target()`
(read-only view of the alternate index Vec already stored in history) instead
of 1–4, and `stage_undo/stage_redo` in step 8. No compensation sequence.

No-op: an empty plan is reported as "no duplicates" after step 1; no staging,
revision, history, redo, dirty or GPU change.

## Minimal API extensions

- ad-document: `CandidateIndices<'a> { mesh, indices, revision }`;
  `snapshot_with(Option<CandidateIndices>)`; split `apply_triangle_removal`
  and `exchange_indices` into check + infallible swap; read-only target of an
  `IndexExchange`. Existing functions become wrappers with identical behavior.
- ad-analysis: `analyze_with(document, Option<CandidateIndices>, …)`;
  `analyze` delegates with `None`.
- ad-commands: `stage_commit/stage_undo/stage_redo`, `StagedEdit::publish`,
  `undo_target/redo_target`. `commit/undo/redo` become stage + publish.
- ad-render-vulkan: `stage_scene` + `StagedScene::activate`; `set_scene`
  becomes their composition (import path keeps its behavior).
- Fault seam: desktop publication runs against a small scene trait, real
  `Renderer` in the app and a fake in unit tests; a smoke-only flag injects
  upload/wait failure natively. Inert outside smoke mode.
No new crate, no second history, no renderer dependency in domain crates.

## Generation and stale results

Every active snapshot/report belongs to one `Generation` whose stamp is current
for the Session document and whose tag equals the GPU scene tag. A report from
another generation is hidden; the panel shows "recomputing" (decision 1). It is
never used for a plan. Late worker results are dropped by ticket and stamp.

## Threads and resources

Planning, verification, candidate snapshot/analysis, export and file IO run on
workers. GPU upload stays on the UI thread (single queue, no external queue
synchronization) and blocks it for the full upload, including all textures.
Peak VRAM ≈ old scene + candidate scene + staging; budget refusal before step
10 keeps the old scene. Old scene release relies on `wait_idle`, not on frame
counters. There is no VK_EXT_memory_budget; measurements report accounted
allocation bytes and phase timings, not driver usage. Texture reuse across
generations is a later optimization.

DEVICE_LOST: CPU document/history/redo are preserved; the GPU scene is not.
Device recovery is out of scope; the app reports the error.

## Owner decisions (2026-11-17)

1. Stale diagnostics hidden behind a "recomputing" banner; no Apply from them.
2. Dirty = current revision/origin differs from the last confirmed state
   (import, or last `SavedCopy` receipt). Conservative: Apply→Undo stays dirty.
   Save Copy does not change the working path.
3. Unsupported changed export refuses Apply before the edit, with exact reason.
4. `Arc<Session>` with background preparation.
5. New import cancels edit preparation; Apply/Undo/Redo disabled while an
   import or save holds the Session.
6. Work on branch `claude/g002-heal` in Main; no push without owner command.
7. Native checks use self-closing smoke scripts; interactive input/screen
   capture only with separate owner permission.

## Test invariants (RED before GREEN)

- Candidate snapshot/report equal post-commit ones exactly (B1).
- Failure at each check of stage_* leaves revision, cursor, redo, indices (B2).
- Injected `GpuScene::new`/`wait_idle` failure keeps old scene tag, document,
  history, redo, generation, dirty (B3/B4, AT-08).
- Stale ticket/stamp, foreign plan, busy Session, cancel at each stage (AT-05/06).
- Full history, revision overflow, invalid scope refuse before change (AT-07).
- Failed Apply after Undo keeps redo; successful one drops it (AT-09).
- Apply→Undo→Redo: indices, revisions, report and scene tags agree (AT-10).
- No-op keeps everything (AT-04). Existing viewer/import tests unchanged.

Stop condition: if a step proves impossible without changing Session ownership
or history semantics beyond the list above, stop and ask the owner.
