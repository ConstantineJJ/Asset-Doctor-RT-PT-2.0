# Asset Doctor RT-PT 2.0

Native Windows application for diagnosing, repairing and preparing 3D assets,
written from scratch in Rust with a Vulkan renderer. Engineering prototype.

## What it does today

- Opens self-contained static **GLB** files and renders them with Vulkan:
  base color, vertex colors, optional direct PBR with metallic/roughness,
  normal, occlusion and emission maps.
- Scene tree, summary and camera: left drag orbit, right/middle drag pan,
  wheel zoom, **F** frame, **Esc** cancel loading, **H** help.
- **Диагностика**: geometry and UV checks G001–G011 (zero-area and duplicate
  triangles, unused vertices, missing/zero normals, boundary and non-manifold
  edges, connected groups, missing UV0, zero UV area, UV outside 0…1).
- Repairs with preview → Apply → Undo/Redo (Ctrl+Z / Ctrl+Y):
  remove zero-area triangles (G001), duplicate triangles (G002) and unused
  vertices (G003) of one source primitive.
- **Сохранить копию…** writes a new GLB only after an independent re-import
  check; the original file is never overwritten.

Skins, animations, morph targets, alpha BLEND, compressed geometry and external
resources are refused at import. Rigging, environment lighting, RT and PT are
planned.

## Build

Requires MSVC Build Tools with the Windows SDK, Rust 1.100.0 (pinned in
`rust-toolchain.toml`) and the LunarG Vulkan SDK for validation layers.

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
cargo run --locked -p ad-desktop -- --open model.glb
```

The executable is `target/release/AssetDoctorRTPT.exe`. `--require-validation`
fails if Vulkan validation layers are unavailable.

## Layout

- `crates/ad-document` — asset document, commands and candidate views
- `crates/ad-formats` — GLB import and verified export
- `crates/ad-analysis` — diagnostics
- `crates/ad-commands` — revision-checked edit history
- `crates/ad-heal` — repair planning and independent verification
- `crates/ad-storage` — safe new-file copies
- `crates/ad-render-vulkan` — Vulkan renderer
- `crates/ad-desktop` — the application
- `tests/fixtures` — small authored models used by the tests
- `assets` — application icon

Product licensing has not been decided yet.
