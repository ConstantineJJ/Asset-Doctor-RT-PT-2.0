# Dependencies

| Dependency | Resolved version | License | Purpose / replacement |
|---|---|---|---|
| Rust | 1.100.0 pinned | MIT/Apache-2.0 toolchain | Compiler/build/test; update deliberately with verification |
| ash | 0.38.0+1.3.281 | MIT OR Apache-2.0 | Experimental thin Vulkan bindings; isolated GPU boundary allows replacement |
| libloading | 0.8.8 locked transitive | ISC | ash dynamic loader; replace through ash feature/loader boundary |
| windows-targets / architecture import libs | 0.53.5 / 0.53.1 | MIT OR Apache-2.0 | Transitive Windows loader imports; no domain types |

libloading 0.8.8 is deliberately retained in Cargo.lock for the portable GNU
bootstrap. 0.8.9 needs an import-library assembler not present in the bundled GNU
fallback. Do not carry this constraint forward silently: revisit after verified
MSVC setup and security audit. Lockfile is the full transitive inventory.

No compression/UI/image/GLTF libraries are selected yet. Evaluate actual footprint,
platform behavior, licenses and advisories before adding each. No old npm/Tauri
dependency graph is imported. Product license remains undecided by the owner.
The owner-supplied icon is branding for this application; it does not establish
redistribution rights for unrelated assets. Private machine paths in audit
provenance need review before any later public publication.
