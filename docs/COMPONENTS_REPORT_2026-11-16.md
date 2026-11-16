# Vertex-index connected-component verification — 2026-11-16

PASS: algorithm4 adds G008 information when a source primitive contains multiple
vertex-index connected groups. A shared vertex joins triangles even without a
shared edge. Positions, primitives and instances are not welded. This is neither
edge-connected surface analysis nor a physical-part count. Source data is read only.

PASS: eligibility matches edge analysis (three distinct indices). Collinear faces
participate; repeated-index faces are excluded/count retained. Unused vertices and
vertices referenced only by excluded faces are not components. Duplicate/reversed
eligible rows count as distinct source triangles. Zero/one group produces no G008
finding, without a general health verdict. Prior G001..G007 meanings stay unchanged.

PASS: typed samples retain the smallest source vertex index, unique vertex count
and eligible triangle count, lowest16components ordered by representative. Union
root selection/face order does not change summaries. An independent three-u32-array
forest uses12bytes per source vertex with checked64MiB scratch admission and
fallible reserves, after earlier buffers retire. Union by size/iterative compression,
no recursion or adjacency map. Consuming summary reuses root member counts as
visited markers; all-excluded scope completes without forest allocation.

PASS: initialization/union/summary cancellation discards results; tests interrupt
both union processing and a partly accumulated summary. Scratch/output refusal
marks only G008 incomplete and preserves earlier findings. G008 is attempted after
edge refusal, with inherited fixed coverage and bounded omitted IDs. Scratch
admission is not a whole-process/peak memory bound; earlier sorts remain internally
noninterruptible.

PASS:94workspace tests (24analysis/20desktop/14document/24formats/12renderer),
fmt, all-target clippy warnings denied, Windows release build, cargo-deny all gates.
No dependency/package/policy changes. Tests cover point joins, split coincident
indices, inactive primitives/instances, unused/excluded-only data, collinear and
cyclic/reversed rows, stable minima/sizes,16sample cap, exact239/240byte admission,
output refusal, zero-eligible fast path and continued G008 after edge refusal.

PASS: authored Components_Checker.glb has3source primitives/7triangles/8findings,
4components in primitive0; source instance yields11displayed triangles/33vertices.
Samples are v0/v3/v6/v9 with3vertices/1triangle each. All18fixture hashes/sizes and
manifest coverage verified; generator reproduces bytes. No external/user assets.

PASS: native component/edge/partial/empty lifecycle and rejected replacements,
0validationerrors. The2million-face edge refusal retains3core findings, skips
G006/G007 and completes G008, giving6of8rules complete. Child synchronization
validation requested; effective setting not independently queried.

PASS (qualitative UI): group count/sizes, completed8rule coverage, and expanded
partial coverage with G008complete/G006-G007memory refusal observed at800x540.
Scroll/footer remains visible. Both test apps closed normally with0errors and
capture session reset. Logs components-ui-visual.* and components-partial-ui.*
under ignored artifacts; frame counts are not a benchmark. No screenshots committed.

PASS (local observations): Tire_Quartet402361vertices/702558triangles matches owner
screenshot and124736boundary pairs. All8rules complete in140ms; G0081399index groups.
Vanguard852516vertices/1559826triangles completes in295ms with2219index groups.
These are source-index counts, not independently calibrated physical-part oracles;
timings are single runs, not benchmarks. Both complete15meshframes, rejected
replacement/lifecycle with0errors and unchanged source SHA256. Logs
artifacts/tire-components.*, vanguard-components.*, release-import-smoke*components*.

Source review found no concrete blocker. Its suggested continued-G008-after-edge-
refusal assertion was added and passed, including native/visual evidence. Reviewer
did not rerun GPU/UI. Full UV/material/profile diagnostics, rescan/edit invalidation,
repair/Undo/export, welded topology, orientation/manifoldness/watertightness,
peak memory/fault injection, environment/shadows and RT/PT remain open. Legacy
and supplied Arc-XRay unchanged; local commits only, no push.
