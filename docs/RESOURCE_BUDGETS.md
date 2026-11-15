# Resource budgets

No production numerical limits have been chosen. Target development hardware:
32 GB system RAM and RTX 5060 Ti with 16 GB VRAM; actual Vulkan heaps must be
queried. These resources must never be summed into a shared 48 GB allowance.

Admission contract: source + decoded CPU + transient CPU + history are checked
separately from GPU decoded/staging/frame-target budgets, with checked arithmetic,
clear rejection and cancellation before commit. Compression expansion, mipmaps,
shared resources and allocations retained by old revisions must be accounted for.
Resource release must respect CPU references and GPU completion fences.

Measure peaks and latency with empty, typical, high-triangle, high-texture,
skinned/morph/animated and 2–8-model fixtures before approving thresholds.
Capability enumeration reports heap size, not free VRAM or a safe allocation cap.
