# Slice15 — Duplicate-removal behavior audit

2026-11-17; read-only source inspection of Legacy, no execution or implementation
reuse. Existing feature inventory: REPAIR-06 / HEAL-01; native Legacy behavior is
still unverified.

- src/heal/DuplicateTriangleRepair.ts:7,58–160: source behavior distinguishes
  oriented exact-index duplicates, keeps first occurrences, blocks unsupported
  draw/material layouts and checks a proposed index result before committing.
- src/heal/HealVerification.ts:366–398: source behavior checks unchanged vertex
  count, expected triangle delta and target absence separately from metric changes.
- New document primitives already delimit material/draw scope; source MeshIds
  differ from visible instances. Generic index commands retain all attributes and
  immutable source bytes. Diagnostic G002 keeps only16examples, so its samples
  cannot be the repair input.

ADR-0030 accepts a fresh bounded CPU plan for one explicit source primitive and
independent precommit postcheck, integrated with existing command history. It does
not transfer Legacy's THREE probes, UI controller, queue, source-patch writer or
heuristic metric-regression policy. Native material/export eligibility and atomic
publication must still be closed before a user-facing Heal button. Source and
owner assets remain protected; no Legacy tests/builds/EXE were run.
