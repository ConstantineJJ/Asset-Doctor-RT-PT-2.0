# ADR-0008 — Verification and release gates

Status: ACCEPTED for bootstrap; expand with implemented slices.

Pinned toolchain and Cargo.lock; fmt, clippy with denied warnings, unit/integration
tests and Windows MSVC build in CI. Dependency licenses/advisories checked with
cargo-deny. GPU-less CI builds cannot certify rendering or installed-app behavior.
Local portable GNU results are recorded separately from MSVC verification.

GPU smoke must state actual device/driver/layers and assert presented frames,
resize/minimize/restore, orderly shutdown and zero validation errors. A missing
validation layer is UNVERIFIED, never validation PASS. Unsupported GPU/loader has
an actionable failure. Product native smoke later adds import/edit/cancel/export
and unsaved close. Only original/synthetic fixtures with known redistribution
rights enter repository. No release/installer acceptance at bootstrap.
