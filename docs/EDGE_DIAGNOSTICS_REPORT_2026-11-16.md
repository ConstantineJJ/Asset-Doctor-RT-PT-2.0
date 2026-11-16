# Indexed edge diagnostics verification — 2026-11-16

PASS: algorithm3 adds G006 boundary edges (information) and G007 edges with more
than two incident source triangles (warning). Each source primitive is analyzed
once, including inactive data. Coincident positions and separate primitives are
not welded. Cyclic/reversed duplicate source faces retain distinct incidents.
Repeated-index faces are excluded entirely and counted; distinct-index collinear
faces participate. Degree rules do not establish full manifoldness or watertightness.

PASS: evidence distinguishes scalar source vertex/triangle indices from typed
source vertex pairs. Lowest16lexicographic edge samples retained per finding;
fixed per-rule coverage stores completed/omitted primitive counts, first resource
reason, excluded faces and up to16omitted primitive IDs. No failure-path allocation
is needed for coverage. Missing checks are never represented as zero findings.
Core result caps preserve earlier findings. Fatal upfront core admission and
whole-job cancellation still return errors with no partial report publication.

PASS: core sort/reference buffers retire before edge phases. EdgeRecord is12bytes,
with checked3×eligible record admission against existing64MiB scratch. Fallible
reserves and bounded samples. Cancellation probes surround the in-place sort and
run through scans, including dense groups. Sort itself is not internally
interruptible. Scratch limits are not whole-process/peak RAM or VRAM bounds.

PASS:84workspace tests (15analysis/19desktop/14document/24formats/12renderer),
fmt, all-target clippy with warnings denied, Windows release build, cargo-deny
advisories/bans/licenses/sources. No dependency/version/license-policy changes.
Unit oracles include single/shared/tetra/three-incident/reversed/cyclic/repeated
faces, split-but-coincident indices, separate/inactive primitives, instancing,
exact107/108byte edge admission, bounded omissions/samples, zero output budget,
all-excluded coverage and deterministic cancellation within a dense edge group.

PASS: authored Edge_Topology.glb has7source primitives/13triangles/16findings,
21boundary pairs and1high-incidence pair,1excluded face. Instances yield16displayed
triangles. Geometry_Findings now has9findings under7rules (prior6findings under5).
The2million-face Edge_Limit input is generated only in ignored artifacts: three
collinear indexed vertices prevent raster overdraw. Core32000003bytes fits;
edge72000000bytes exceeds64MiB. Real import retains3core findings and5completed
rules, marks G006/G007ResourceLimited, and still displays the model.

PASS: native edge/partial/geometry/emission/empty smoke runs presented15meshframes
where applicable, retained the old model on rejected replacement, resized,
minimized/restored and closed normally,0validationerrors. Import runs requested
child synchronization validation; effective setting not independently queried.

PASS (qualitative native UI): G006 pairs/counts, G007 pair0–1, coverage7/7 and
partial yellow status with5/7 coverage and explicit memory reasons were visible.
Top/bottom scrolling and scope footer inspected at800x540; both windows closed,
0validationerrors. Window capture session reset. No screenshots/user assets committed.
Native visual logs: edge-ui-visual.* and edge-limit-ui-visual.* in ignored artifacts.

PASS (one local observation): heavy Vanguard1559826source triangles/852516vertices,
all7rules complete,1finding in267ms; edge56153736bytes fits existing cap. GPU keeps
4images/223696208mipbytes, completes replacement/lifecycle with0errors; sourceSHA256
unchanged. Finding type/count correctness on arbitrary owner geometry is not a
calibrated oracle; known fixtures establish rule semantics. Timing is not a benchmark.
Logs: vanguard-edge-smoke.*, release-import-smoke*edges*, *edge-limit*.

Source-only independent reviews found no remaining concrete blocker; requested
all-excluded/dense-cancellation cases were added and passed. Reviews did not rerun
native/UI verification. Full connected components, welded topology, orientation,
vertex manifoldness, UV/material/profile/rig checks, rescan/edit invalidation,
repair/export, peak memory and fault injection remain open. No health score, push,
Legacy or supplied Arc-XRay report modification is claimed.
