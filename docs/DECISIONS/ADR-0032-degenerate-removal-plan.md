# ADR-0032 — Removing degenerate triangles (REPAIR-01, G001)

Status: ACCEPTED, 2026-11-17. Owner accepted the four recommendations below.
Extends ADR-0030 (plans) and reuses ADR-0031 (publication) unchanged.

## Scope

One explicit source MeshId, all node references (including inactive), as in
G002. A triangle is degenerate exactly when the G001 diagnostic says so: the
robust exact zero-area predicate in all three coordinate projections
(`ad_analysis::zero_area`). This covers repeated-index faces and faces whose
three distinct vertices are collinear. No area tolerance (Legacy used an
epsilon relative to the bounding box; owner decision 1 rejects it, so the plan
always equals the complete G001 set and nearly-thin real faces stay).

Only indices change: survivors keep their original order and winding; vertex
positions, attributes, materials, nodes and source bytes are untouched. Unused
vertices left behind are G003 findings for REPAIR-02, not part of this edit.

## Decisions (owner, 2026-11-17)

1. Exact zero area, same predicate as G001; no epsilon.
2. Shared primitive: edit applies to every referencing node and the preview
   states the reference count (Legacy blocked shared geometry).
3. A primitive whose every triangle is degenerate is refused with an explicit
   message; empty primitives are not supported (ADR-0026).
4. Verification uses the same predicate; `ad-heal` gains a runtime dependency
   on `ad-analysis` for it (boundary change to ADR-0030, which allowed only
   document/commands). No report or renderer dependency.

## Plan and verification

`plan_degenerate` scans the whole primitive (limit 4 M triangles, result
16 MiB, cancellation every 1024 faces) and returns the sorted removal ordinals
in the same `RemovalPlan` shape used by G002, tagged with its repair kind.
Preparation goes through the existing `Session::prepare_removal` admission.
Independent postcheck streams source and candidate once: every source face is
either zero-area and absent, or non-zero and equal to the next candidate face;
the candidate must be fully consumed. This proves deletion-only, order and the
exact removal set without scratch memory. Desktop then runs complete analysis
on the candidate; "G001 gone" is claimed only for complete coverage.

## Desktop

New `EditKind::RemoveDegenerate(MeshId)`; the G001 row gets the same
read-only "Предпросмотр ремонта…" action, the preview window names the repair,
and export preflight, publication, Undo/Redo, Save Copy and guards are reused.

## Acceptance

Collinear and repeated-index faces removed; real thin faces and other
primitives kept; shared references reported; all-degenerate primitive
refused; no-op when none; stale/foreign/cancel guards; hostile candidate
refused by postcheck; desktop preview → Apply → Undo/Redo → Save Copy →
reopen with complete analysis and no G001 on the edited primitive.
