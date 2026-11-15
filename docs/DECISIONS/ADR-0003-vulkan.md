# ADR-0003 — Vulkan binding experiment

Status: EXPERIMENTAL; ash is not a final renderer commitment.

Use ash 0.38 for a minimal local capability probe, then window/clear and lifecycle
smoke. Dynamically load Vulkan runtime; report GPU, driver raw version, API,
queues and extension availability. Extension presence is not enabled RT support.
Base instance API 1.1; final renderer feature baseline remains undecided.

Require validation for validation-certified runs; absence is explicit failure of
--require-validation rather than a false PASS. Ordinary exploratory probe may run
without layers and prints that fact. Context owns loader/instance/debug callbacks
with reverse-order teardown. Window/device/frame resources must precede context
teardown. Device loss must surface as a controlled error preserving document data.

References consulted: Context7 /ash-rs/ash/0.38.0, upstream ash README.
