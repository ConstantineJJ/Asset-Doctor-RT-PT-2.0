# Development checkpoint — 2026-11-16

Owner authorizes autonomous development and computer use without manual acceptance.
No push/release authorized. Models, Legacy and supplied Arc-XRay remain read-only.

Current feature commit:2c12381 (vertex-index connected components). Previous
757a963/412dd52 added per-rule coverage/indexed edge incidence.

Analysis algorithm4, eight rules: zero area, oriented same-index duplicate faces,
unused vertices, missing/zero normals, boundary edges, high-incidence edges and
multiple vertex-index connected groups. G006/G008 are information. Connectivity
uses shared source indices (a point join suffices), never positional welding,
instance/material merging or physical-part counting. Repeated-index triangles
are excluded/count retained for topology phases; distinct-index collinear rows
participate. Unused/excluded-only vertices are not components.

Typed evidence includes scalar indices, vertex pairs and component summaries
(minimum source vertex, unique vertex count, eligible source triangle count),
maximum16samples. Component forest uses3u32arrays/12bytes per source vertex after
other phase scratch retires; consuming summary reuses member counts as root markers.
Coverage retains completed/omitted primitives, first resource reason, excluded
faces and16omitted IDs. G008 refusal preserves earlier results; G008 can complete
after edge refusal. Whole-job cancellation discards publication; upfront core
admission remains fatal. Sources and authoritative revision are unchanged.

Gates:94tests (24analysis/20desktop/14document/24formats/12renderer), fmt,
all-target clippy warnings denied, Windows release build and cargo-deny all PASS.
No new dependency or policy exception. All18fixture hashes/sizes/manifest entries
verified, component generator reproducible. Source review found no blocker; added
explicit continued-G008-after-edge-refusal test and native/visual checks.

Native: Components_Checker3source primitives/7triangles/8findings,4groups in
primitive0,11display triangles with instance.2Mface edge refusal retains3findings,
6of8rules complete (G006/G007scratch limited,G008complete). Component/edge/partial/
empty lifecycle/rejected replacements0validationerrors. Actual full/partial8rule
UI and group counts/sizes/scroll/footer inspected800x540; normal closes0errors,
capture reset, no test app left open. Sync validation requested, effective setting
not independently queried. No user assets or screenshots copied into repository.

Tire_Quartet402361vertices/702558triangles/124736boundary pairs,1399index groups,
all8rules in140ms. Vanguard852516vertices/1559826triangles/2219index groups,295ms.
One-run local observations, not benchmarks or physical-part oracles. Both source
hashes unchanged, runtime/replacement/lifecycle0errors. Durable report:
COMPONENTS_REPORT_2026-11-16.md; ignored logs artifacts/*components*, components-
ui-visual.*, components-partial-ui.*, release-import-smoke*components*/*edge-limit*.
Runnable EXE:target/release/AssetDoctorRTPT.exe.

Next: source UV-channel inventory/diagnostics contracts before loader/domain edits.
Current MeshPrimitive UV arrays are selected per material role; they are NOT a
complete source TEXCOORD_n inventory and must not be labelled UV0 by assumption.
Decide preservation/budgets/optional non-textured UVs and exact diagnostic rules.
Do not infer arbitrary UV values outside0..1 are invalid with repeating samplers.
Repair/history/export, full topology/profile/rig, environment/shadows and RT/PT
remain future work. Existing sorts are not internally interruptible and scratch
limits do not cap whole-process memory peaks.
