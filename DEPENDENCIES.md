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

The initial GNU-only libloading 0.8.8 constraint was removed after MSVC readiness;
0.8.9 now builds and passes checks. Cargo.lock is the full transitive inventory,
including target-only packages not compiled into Windows. Runtime dependency
code/byte footprint has not been profiled yet.

No GLTF/PBR or final product UI library is selected yet. Evaluate actual footprint,
platform behavior, licenses and advisories before adding each. No old npm/Tauri
dependency graph is imported. Product license remains undecided by the owner.
The owner-supplied icon is branding for this application; it does not establish
redistribution rights for unrelated assets. Private machine paths in audit
provenance need review before any later public publication.

cargo deny check passes for the Windows graph. deny.toml has one exact,
documented duplicate exception: miniz_oxide 0.8.9 required directly by png while
its current flate2 dependency requires miniz 0.9.1. No global duplicate or advisory
gate was weakened. Revisit on next png update. Current license/advisory PASS is
not a future vulnerability guarantee or a license for the application itself.
