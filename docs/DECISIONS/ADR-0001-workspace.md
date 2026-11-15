# ADR-0001 — Independent workspace

Status: ACCEPTED for bootstrap, 2026-11-15.

Use one Cargo workspace rooted in Main, with separate Git history continuing the
owner's README commit. Start with one experimental Vulkan crate; introduce core,
formats and desktop crates only with real independently testable responsibilities.
No Legacy implementation/dependencies are copied. Parent directory holds local
tools and work artifacts. Commit lockfile, pinned Rust toolchain and Windows CI.

Product ID proposal: com.constantinejj.assetdoctor.rtpt. Do not register file
associations/install paths until packaging work; never reuse Legacy preferences.
Package 0.1.0 is experimental; displayed product name remains RT-PT 2.0.
License is unset pending owner selection; do not invent a public license.
