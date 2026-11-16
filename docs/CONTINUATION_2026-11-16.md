# Development checkpoint — 2026-11-16

Owner authorizes autonomous development and computer use without manual acceptance.
No push or release is authorized. Models, Legacy and supplied Arc-XRay are read-only.

New local commits:

- `757a963`: per-rule coverage preserves completed findings on result limits.
- `412dd52`: indexed edge incidence, typed evidence, fixtures and native verification.

Current analysis algorithm is3. Seven rules: zero area, oriented same-index duplicate
faces, unused vertices, missing/zero normals, boundary edges and edges with more
than two incident source triangles. Boundaries are information, not automatic
defects. Source primitives, including inactive ones, are checked once. Repeated-
index triangles are excluded from edge checks and counted; distinct-index collinear
faces remain eligible. Coincident positions and separate primitives are not welded.

Report coverage is fixed per rule: completed/omitted source primitive counts,
excluded faces, first resource reason and at most16omitted primitive IDs. Evidence
is scalar vertex/triangle indices or typed vertex pairs, bounded to16samples.
Existing source/material/UI/GPU behavior stays read-only. Whole-job cancellation
discards publication; upfront core admission remains fatal. Edge scratch/output
refusal preserves completed core findings with explicit partial status.

Checks:84workspace tests (15analysis/19desktop/14document/24formats/12renderer),
fmt, all-target clippy with warnings denied, Windows release build and cargo-deny
advisories/bans/licenses/sources PASS. No dependency or license-policy changes.
All17fixture hashes/sizes/manifest entries verified; edge generator reproduces bytes.

Native evidence:

- Edge fixture:7source primitives/13triangles/16findings,21boundary pairs and
  1high-incidence pair,1excluded face; instances yield16displayed triangles.
- Generated transient2million-collinear-face input:3core findings retained,
  5rules complete,2edge rules explicitly resource-limited. Core32000003bytes fits,
  edge72000000bytes exceeds64MiB. Input stays in ignored artifacts, not repository.
- Edge/partial/geometry/emission/empty lifecycle and failed replacement passed with
  0validationerrors. Synchronization validation requested in child environment;
  effective setting not independently queried.
- Heavy Vanguard1559826triangles:7rules complete,1finding,267ms local observation,
  source hash unchanged. No benchmark or arbitrary-model correctness oracle claimed.
- Actual native UI showed edge pairs/counts, coverage and partial yellow status
  with memory reasons. Scroll/footer inspected at800x540. Both windows closed with
  0errors and capture session reset. No test app remains open or screenshot committed.

Durable report: EDGE_DIAGNOSTICS_REPORT_2026-11-16.md. Ignored logs under artifacts:
release-import-smoke*edges*, *edge-limit*, edge-ui-visual.*, edge-limit-ui-visual.*,
vanguard-edge-smoke.*. Release EXE: target/release/AssetDoctorRTPT.exe.

Next: settle source-index connected-component semantics and admission before
implementation. Specify vertex-connected versus edge-connected groups, treatment
of unused/degenerate data and primitive boundaries. Preserve per-rule coverage,
bounded samples, immutable sources and cancellation. UV/material/profile analysis,
rescan/edit invalidation, verified repair/Undo/export, environment/shadows and
RT/PT remain future work. Sort phases are not internally interruptible; memory
admission is not a whole-process peak bound. Source reviews found no remaining
blocker after dense-cancellation/all-excluded acceptance tests were added.
