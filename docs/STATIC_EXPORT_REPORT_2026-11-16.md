# Verified static GLB export and new-copy storage — 2026-11-16

Feature checkpoint: `7fe1f05e27146a3b4b1d8bc9c9246ba9c9f91108`.
Scope: internal CPU exporter and Windows filesystem API. Desktop still provides
read-only viewing/diagnostics; no Heal planner, Save dialog or dirty-state clearing.

## Implemented behavior

Accepted [ADR-0028](DECISIONS/ADR-0028-verified-static-glb-export.md) and
[ADR-0029](DECISIONS/ADR-0029-safe-new-copy-storage.md) close only the static
writer/storage subset of [ADR-0027](DECISIONS/ADR-0027-first-heal-export-proposal.md).
[Slice14 audit](../reference/SLICE_14_STATIC_EXPORT_AUDIT.md) records read-only
Legacy behavior leads, without executing/copying its implementation.

- The existing importer preflight is shared with export. Source mesh/primitive
  order binds current source primitives, including inactive ones; visible draws
  or node names never select write targets.
- Current indices must be a nonempty, ordered whole-triangle subsequence of source
  indices. U8/U16/U32 and implicit source indices work. Only changed primitives
  receive appended U32 accessor/view storage; the original BIN, encoded images,
  attributes, old accessors and other core metadata stay preserved.
- Unchanged data, including after Undo at a new revision, copies exact original
  GLB bytes. Opaque metadata/chunks can therefore survive without rewriting.
  Changed export refuses opaque extras/extensions, unknown fields/chunks and
  untyped/invalid accessor bounds. This restriction is explicit, not data loss.
- Before returning VerifiedGlb, independently import the candidate and compare
  all current nodes/roots/transforms, source primitives/indices/attributes/UV,
  material factors, sampler/image bytes and texture statistics. Serialize/parse
  metadata equivalence is checked separately. Reimport creates another decoded
  document, so peak memory remains an unprofiled limitation.
- Proofs and saved receipts retain private origin plus document ID/revision.
  Reused public tags cannot authorize a foreign document; older revisions stay
  stale. A verified buffer is distinct from a confirmed published disk file.
- New ad-storage writes only to a new absolute .glb path in an existing directory.
  Its exclusive temporary file allows readers but denies competing writers/delete.
  Chunk write, sync_all and full byte readback precede no-replace hard-link
  publication. It never opens/truncates an existing destination, including an
  existing original path or alias. There is no unsafe overwrite fallback.
- Cleanup marks the owned Windows handle for deletion before closing it; no
  pathname cleanup after releasing protection. Before publication, cancellation
  or failure removes staging. After publication, cleanup failure returns Saved
  with a warning and retained temporary link; it cannot become a failed save.
  Non-Windows storage explicitly refuses operation. Current NTFS volume works;
  unsupported filesystems may refuse. Directory power-loss durability and hostile
  replacement of directory ancestors/junctions are not certified.

## Verification and review

157 passing workspace tests: analysis35 / commands14 / desktop23 / document22 /
formats38 / renderer12 / storage13. The final full workspace run passed all157.
Formatting check,
all-target clippy with denied warnings, locked Windows MSVC release workspace
build and cargo-deny advisories/bans/licenses/sources PASS. No new registry
versions; existing serde/serde_json become explicit production dependencies and
existing windows-sys0.52 supplies Foundation/FileSystem bindings.

Export tests cover exact-copy/Undo, altered indices, original BIN retention,
instances/inactive/multiple scenes, six material fixtures, U8/U16/U32/implicit
indices, shared accessors, opaque metadata/chunks, duplicate/deep/bounded JSON,
budget/cancel/stale/foreign refusal and deliberately mismatched authoring data.
Storage tests use actual owned files on E: NTFS: byte-exact copy/reimport,
existing/original-path preservation, invalid path/proof/cancel, corrupted/truncated
readback, injected partial-write IO failure, racing and simultaneous publication,
Windows sharing, cleanup identity and actual readonly cleanup failure after save.
Injected failure is an exercised error path, not a physical disk-fault simulation.

Initial missing exporter/storage APIs failed their first tests before implementation.
Independent source review found an unchecked accessor min/max metadata case;
source-import-accepted regression reproduced RED, fixed guard passed GREEN. Review
also found pathname-cleanup identity risk; handle-bound deletion removed that path.
Saved receipt origin retention was strengthened. Final focused source review found
no remaining blocker. Reviewer did not independently run tests or GPU/UI checks.

Developer [save_copy_probe](../crates/ad-storage/examples/save_copy_probe.rs)
uses only the authored Geometry_Findings fixture. It removed one triangle from
primitive0 (5→4), produced a1348-byte verified copy and saved it to a fresh owned
artifact path, with no cleanup warning. Fresh native readback reported2source
primitives/5source triangles; instances produce9display triangles. The copy rendered
15mesh frames, preserved itself after damaged replacement and passed native
resize/minimize/restore/close, with validation enabled and0errors.

Existing UV, normal-PBR and empty-window native regressions also passed their
applicable lifecycle/replacement checks with0validationerrors. Child synchronization
validation was requested through the environment; its effective setting was not
queried separately. This is pipeline/lifecycle evidence, not pixel fidelity or
user-facing Heal acceptance. No interactive capture/input session was started;
all probe windows closed. Legacy source, owner models and Arc-XRay were untouched.

Local evidence is ignored under artifacts/static-copy-native.*,
artifacts/static-copy-probe-path.txt, artifacts/Static_Copy_Probe_*.glb,
artifacts/release-import-smoke*uv*/*normal* and normal empty-smoke logs.
Previous command/docs checkpoint14189f0 passed
[Windows CI run43857372910](https://github.com/ConstantineJJ/Asset-Doctor-RT-PT-2.0/actions/runs/43857372910).
New checkpoint CI is separate; local GPU success is not hosted GPU verification.

## Remaining work

First full G002 Heal still needs a complete eligible duplicate-removal plan,
read-only preview, independent postcheck, atomic document/history/analysis/snapshot/
GPU publication, native Undo/Redo and Save Copy/unsaved-close integration. Never
repair using only the16retained diagnostic samples. No full attributes/rig writer,
opaque changed export, skin/animation/BLEND combinations or general repair parity.
EXP-01 remains NOT_STARTED as a user-facing scenario, with its new backend listed
explicitly; all seven repair-operation rows remain unfinished.
