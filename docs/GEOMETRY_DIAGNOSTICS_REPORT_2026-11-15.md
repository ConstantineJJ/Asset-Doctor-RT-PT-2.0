# First source geometry diagnostics — 2026-11-15

PASS: ad-analysis is independent of renderer/UI/formats/files/network. It scans
immutable source primitives once, including inactive data; instance/world/view
transforms do not change counts. Tagged document/revision/algorithm reports carry
stable G001..G005 meanings: zero area, oriented same-index duplicate triangle,
unused vertex, missing normals, zero normals. Reversed winding is distinct;
position-coincident vertices with different indices are not welded.

PASS: adaptive robust1.2.0 orient2d in three projections avoids plain f64 cross
cancellation. Review repro and source f32 extreme ranges pass. Source bytes/indices
remain unchanged. Missing normals is information, other findings warnings. There
is no fitness/health score. Near-zero areas, boundaries/components, UV/material/
rig/profile checks and repairs are outside this first subset.

PASS: 4million total source triangles,64MiB per-primitive scratch,4096findings,
16lowest source indices per finding. Fallible allocation/prospective admission;
periodic cancellation and checks around bounded sort. Sort itself is not
interruptible. A limit error never publishes partial results as complete. Analysis
storage limits are not whole-process peak bounds. Reports use source vertex or
triangle indices, zero-based, not expanded viewport offsets.

PASS: worker ticket/cancel guards cover report publication; valid model may open
with explicit incomplete diagnostic status. Stale report hidden by source tag.
UI offers read-only Diagnostics window, visible scope/omissions and bounded
virtual rows. Native inspection corrected row stride and footer clipping, then
verified top/bottom scrolling at800x540. All test windows closed normally.

PASS:72tests, fmt, clippy(alltargets,warningsdenied), Windows release build,
cargo-deny advisories/bans/licenses/sources. Known fixture:2source primitives,
6source triangles,6findings;3instances produce11displayed triangles. Missing/zero
normals, cyclic/reversed/collinear faces, sample caps, budgets/cancel/source tags,
worker refusal and actual Diagnostics button covered.

PASS: native geometry/emission/empty lifecycle runs, rejected replacement and
0validationerrors. Import runs request child synchronization validation; effective
setting not independently queried. Final qualitative UI inspection3841frames,
0errors, not a benchmark or calibrated pixel test.

PASS (local observation): heavy Vanguard1559826source triangles in1primitive,
0findings under current5rules,77ms analysis;4images223696208mipbytes,
15mesh/PBR/normal/emissive frames, rejected replacement/lifecycle0errors.
Source SHA256 unchanged. No user geometry/images included in repository.
Logs in ignored artifacts: vanguard-analysis-smoke.*, diagnostic-ui-visual-fit.*,
release-import-smoke*geometry*.*. Source-only independent reviews resolved
numerical and row-wrap findings. No Legacy/Arc-XRay modifications or push.

WARN: full topology/profile/format parity, independent rescan/edit invalidation,
calibrated UI accessibility/DPI matrix, peak memory and allocation/device-loss
fault injection remain open. No automatic repair, export or RT/PT is claimed.
