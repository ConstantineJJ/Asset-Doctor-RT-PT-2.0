# Format compatibility

Source evidence: GLBLoaderService, GltfSourceAudit, RepairedExportService, RigGlbPatch, FileIO (indexed in catalog).

| Data | Current source observation | New implementation |
|---|---|---|
| GLB 2 / self-contained JSON glTF | Accepted through loader; original bytes retained | Partial static GLB/BIN implemented; JSON glTF not yet supported |
| External buffers/images/URLs | Single-file importer rejects | Global rejection implemented; no fetching |
| Embedded base-color PNG/JPEG, UV/COLOR_0, OPAQUE/MASK | Authored source captured before presentation overrides | Bounded implementation/native smoke in ADR-0015 and BASE_COLOR_REPORT; not full PBR parity |
| Draco | Bundled view decoder; modified rig/skinning combinations can block export | NOT_STARTED, do not claim parity |
| Meshopt | View decoder; buffer view patch rules exist | NOT_STARTED |
| KTX2/BasisU | Capability detection, bundled transcoder | NOT_STARTED |
| Skins, clips, morphs | Source metadata and export verification have dedicated rules | NOT_STARTED |
| Unknown extras/extensions | Guarded source patching, not universal certification | Preserve opaque provenance; reject unsupported edit explicitly |
| Repaired export | GLB only; source single scene/reachable-node restrictions | NOT_STARTED |
| Rig + Heal export | Separate source patch path; Draco/static baking/index/accessor extension restrictions | NOT_STARTED |
| FBX/OBJ/USD | No product loader found in inspected paths | NOT_FOUND; no implied support |

All entries are source observations, not a fresh file roundtrip/native PASS. Export support must be tested per extension × operation, not inferred from viewing support.
