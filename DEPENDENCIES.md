# Dependencies

| Dependency | Resolved version | License | Purpose / replacement |
|---|---|---|---|
| Rust | 1.100.0 pinned | MIT/Apache-2.0 toolchain | Compiler/build/test; update deliberately with verification |
| ash | 0.38.0+1.3.281 | MIT OR Apache-2.0 | Experimental thin Vulkan bindings; isolated GPU boundary allows replacement |
| libloading | 0.8.9 locked transitive | ISC | ash dynamic loader; replace through ash feature/loader boundary |
| ash-window | 0.13.0 | MIT OR Apache-2.0 | Surface bridge using native raw handles; replace inside renderer surface owner |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | Borrowed window/display contract; no UI types enter renderer |
| winit | 0.30.13, rwh_06 only | Apache-2.0 | Native event loop/window experiment; separate desktop crate boundary permits replacement |
| png | 0.18.1 | MIT OR Apache-2.0 | Decode compile-time owner window icon; no user asset parser yet |
| winresource | 0.1.34, build only | MIT | Embed EXE icon/version through Windows SDK resource compiler |
| windows-sys / windows-targets / MSVC import libs | 0.52.0 / 0.52.6 | MIT OR Apache-2.0 | Desktop-only OS close smoke and error message; same version as winit |
| cargo-deny | 0.20.2, development tool | MIT OR Apache-2.0 | License/advisory/source/duplicate gates; not an application runtime dependency |
| glam | 0.34.3, std only | MIT OR Apache-2.0 | Document transforms and desktop camera math; public geometry stays plain arrays, replace at these boundaries |
| gltf / gltf-json / gltf-derive | 1.4.1, default features disabled | MIT OR Apache-2.0 | Maintained GLB container/schema/accessor validation; utils/names/extras only, no image/import/network helpers; replace inside ad-formats |
| serde_json | 1.0.150, direct use in tests only; parser transitive | MIT OR Apache-2.0 | Synthetic GLB metadata fixture construction; gltf owns production metadata parsing |
| naga / naga-types | 30.0.2, BUILD only | MIT OR Apache-2.0 | Authored GLSL to embedded SPIR-V, glsl-in/spv-out only; replace build.rs compiler without changing runtime renderer |
| foldhash / pp-rs / unicode-ident | 0.2.0 / 0.2.1 / 1.0.27, transitive build tools | Zlib / BSD-3-Clause / (MIT OR Apache-2.0) AND Unicode-3.0 | Shader/compiler data structures, GLSL preprocessor, proc-macro identifiers; source license notices reviewed, retain attribution when packaging |

The initial GNU-only libloading 0.8.8 constraint was removed after MSVC readiness;
0.8.9 now builds and passes checks. Cargo.lock is the full transitive inventory,
including target-only packages not compiled into Windows. Runtime dependency
code/byte footprint has not been profiled yet.

GLB parsing is selected for the static subset (ADR-0013); PBR and the final UI
library remain undecided. Naga adds build dependencies, not a runtime GPU backend.
Current release desktop EXE is 1,679,872 bytes; this is an aggregate executable
measurement, not per-library profiling or total runtime memory.
Evaluate actual footprint,
platform behavior, licenses and advisories before adding each. No old npm/Tauri
dependency graph is imported. Product license remains undecided by the owner.
The owner-supplied icon is branding for this application; it does not establish
redistribution rights for unrelated assets. Private machine paths in audit
provenance need review before any later public publication.

cargo deny check passes for the Windows graph. deny.toml has two exact,
documented duplicate exceptions: miniz_oxide 0.8.9 required directly by png while
its current flate2 dependency requires miniz 0.9.1; syn 2.0.119 required by
gltf-derive/zerocopy-derive while serde_derive/thiserror-impl use syn 3.0.10.
The latter are build-time proc macros. Review again on upstream updates.
The allowlist now also includes reviewed permissive Zlib, BSD-3-Clause and
Unicode-3.0 licenses needed by the compiler graph. No advisory exclusions or
global duplicate relaxation. Binary redistribution notices remain a packaging
requirement before public release. Current license/advisory PASS is
not a future vulnerability guarantee or a license for the application itself.
