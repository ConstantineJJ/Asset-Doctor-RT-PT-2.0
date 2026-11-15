# ADR-0007 — Resource admission and GPU lifetime

Status: PROPOSED; measurements needed before setting product thresholds.

RAM and VRAM have separate budgets. Track source bytes, decoded geometry/textures,
temporary decode/upload copies, snapshots/history, frame targets and later BVHs.
Use checked arithmetic before allocations and reserve admission tokens before
decode/commit. Legacy's 192 MiB file cap is evidence, not the new RAM limit.

GPU resources are retained until fences prove the last using submission finished.
Minimized/zero-size surface submits nothing. Resize requires safe swapchain
replacement. Exhaustion/device loss cannot mutate the authoritative document.
Peak process RAM and VRAM measurements are required on licensed/synthetic fixtures.
