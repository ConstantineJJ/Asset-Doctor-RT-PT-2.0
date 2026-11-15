# ADR-0004 — Native UI choice

Status: OPEN for permanent backend choice. The bounded egui viewer-shell
experiment in ADR-0014 is now implemented and native GPU smoke tested; Win32
controls/accessibility comparison and final backend selection remain open.

Evaluate at least two designs with the same working Vulkan viewport:

| Candidate | Questions to prove |
|---|---|
| winit event loop + egui with explicit ash integration | Input ownership, UI rendering/synchronization, text/DPI, screen-reader integration, docking, native dialogs, dependency/license footprint |
| Native Win32 controls with Vulkan viewport | Resize/DPI behavior, keyboard focus, accessibility, editable tree/inspector complexity, worker progress and viewport input isolation |

These are the permanent-backend evaluation candidates. ADR-0014 records the
currently installed egui dependencies, bounded controls and exact evidence.
No WebView/React/Tauri/Three.js. Use one event loop and explicit pointer capture:
gizmo drag must suppress orbit; side buttons cannot navigate history. Collapsed
tree, automatic 1/2+ Compare and shared settings are product requirements.

Context7 winit v0.30.13 ApplicationHandler examples were reviewed; that alone is
not sufficient to choose winit or prove final UI integration.
