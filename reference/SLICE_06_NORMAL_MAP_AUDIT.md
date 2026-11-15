# Normal texture behavior review — 2026-11-15

Legacy read-only source observations: GeometryAnalyzer.ts:161 counts normal maps;
MaterialAnalyzer.ts records their slots. HealthIssueLocalization.ts:407-409 flags
negative scale/tangent conventions. Locale normal-repair warning refuses a source
normal repair when an accompanying tangent stream cannot be regenerated. This
slice implements preview only, not repair or those diagnostics. No legacy code,
architecture or runtime is used or changed.

Khronos glTF independently specifies linear RGB tangent normals, XY scale, ignored
alpha, authored tangent W/bitangent reconstruction and ignoring tangents without
normals. Missing-tangent reference behavior uses MikkTSpace. The present bounded
preview instead admits a clearly labelled derivative fallback without source edits
or vertex expansion. Authored tangents take priority; full Mikk/baked seam parity
remains an explicit later verification/implementation item, not an implied pass.
