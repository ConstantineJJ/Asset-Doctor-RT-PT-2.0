# Viewer UI behavior audit — bounded gate

2026-11-15. Legacy HEAD 17ca3549767aac78bf4ad51fa1c254c90dd15113 rechecked,
working tree clean. Read-only review, no Legacy implementation reused/executed.

| Behavior | Legacy evidence | New slice contract |
|---|---|---|
| File open | TopToolbar.tsx:129,167, previous import audit | Existing native picker/worker from a visible button |
| Tree visibility | App.tsx:213; UI-02 inventory | Hidden at startup, explicit toolbar toggle (REQ-06 supersedes Legacy expanded default) |
| Selected-node details | InspectorPanel.tsx:1095-1135; TREE-01 | Read-only name/local transform/reference counts; stable document-scoped selection |
| Geometry summary | InspectorPanel.tsx:1038-1080; DIAG-02 | Source size/node count and explicitly labelled expanded snapshot geometry/bounds |
| Camera | CameraController.ts:11,25; static import audit | Existing fit/orbit/pan/zoom; panel input never moves camera |
| Loading/cancel/error | App.tsx:700-731; static import audit | Visible status/cancel, old document and selection survive failure |

This closes only the display/open/selection shell contract. Tree visibility/isolate,
viewport picking/highlighting, material/PBR editing, settings persistence, full
inspector tabs, diagnostics, command history, Compare and native accessibility
remain open. Source review is not Legacy native acceptance. See ADR-0014.
