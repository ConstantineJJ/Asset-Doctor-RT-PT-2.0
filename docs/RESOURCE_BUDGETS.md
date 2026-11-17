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

## Prototype duplicate-plan phases (ADR-0030)

These are engineering caps, not approved production thresholds. One source
primitive admits at most4million triangles,64MiB planning scratch and16MiB retained
removal storage. Source key records retire before source-node reference descriptors
reuse the scratch allowance. Shared node primitive-ID lists are grouped and scanned
once. Candidate verification admits128MiB for both rebuilt key arrays together;
the prepared index replacement and retained removal list have separate ownership
and budgets. Fallible reservation checks capacities and arithmetic before work.
An unchanged no-op aliases current indices and needs no verification scratch.

Cancellation is cooperative in scans and around bounded in-place sorts; sorting
itself is not interruptible. No whole-process memory peak or cancellation-latency
claim. The application must bound outstanding plans and future background jobs.
