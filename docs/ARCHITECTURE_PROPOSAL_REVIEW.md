# Review of owner's advisory architecture v0.1

2026-11-15. Source: attached Initial Architecture Proposal — v0.1. It is advisory,
not an approved source tree. The current user authorizes reasoned selection.

## Accepted

- Independent Rust asset domain, separate application operations, native UI and
  Vulkan resources. No Legacy implementation or architecture is carried forward.
- One authoritative asset document; stable IDs and revisions; commands validate
  their source revision; stale workers cannot publish into newer documents.
- Renderer owns GPU resources only; swapchain recreation cannot destroy assets.
- Original source provenance, export roundtrip verification and safe Save Copy.
- Independent diagnostics/Heal/manual rig/animation responsibilities, common Undo.
- Resource admission distinguishes RAM from VRAM; heavy work avoids the UI loop.
- Raster/PBR → RT → PT → late autorig/retargeting/Mixamo, with rights review.
- Unified single/Compare session, owner mouse/light/gizmo/floor/autoplay requests.

## Adjusted to avoid creating a new monolith

1. Do not put document data, session state, all commands, all jobs and operation
   history into an undifferentiated ad-core. The initial domain crate will have
   data/invariants; application owns sessions, command history and scheduling.
   Physical crate boundaries are introduced with implemented slices.
2. History is application session state with references/deltas to document data;
   it is not automatically serialized into every asset file. Diagnostic reports
   are revision-keyed derived results, not authoritative geometry or permanent
   fields that can survive unnoticed after edits.
3. ad-tools is a possible folder umbrella, never an orchestration manager.
   Diagnostics returns findings; Heal returns plans/commands; rig/animation edit
   domain data. Compare belongs to session/layout and comparison queries, not a
   second set of independent scene/edit/Undo implementations.
4. Camera input belongs to desktop/application presentation state; renderer
   receives camera snapshots. Authored lights and model transforms use document
   commands, with transient gizmo preview kept separate from committed data.
5. No universal event bus or generic multi-backend graphics framework now. Start
   with explicit typed calls/results and one Vulkan backend.

## Deferred pending evidence

Seven proposed crates are a logical map, not seven empty packages. winit/egui are
candidates, not selected product UI. Dedicated render thread, worker count,
snapshot copy strategy and storage format need first-prototype measurements.
Exact decoder/parser choices, extension fidelity and production memory limits
remain open. ash remains experimental until window/frame/validation evidence.

## Present implementation boundary

Only an isolated ash capability probe and a surface-size test exist. No domain
data, tools, desktop UI or first presented frame is implied by this review. The
next native slice is window + Vulkan clear + resize/minimize/close + icon. Tool
installation verification will wait for the owner's explicit «Готово» message.
