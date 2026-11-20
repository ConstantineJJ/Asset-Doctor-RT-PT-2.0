# Asset Doctor RT-PT 2.0

Native Windows application for diagnosing, repairing and preparing 3D assets,
written from scratch in Rust with a Vulkan renderer. Engineering prototype.

## What it does today

- Opens self-contained static **GLB** files and renders them with Vulkan:
  base color, vertex colors, optional direct PBR with metallic/roughness,
  normal, occlusion and emission maps.
- Scene tree, summary and camera: left drag orbit, right/middle drag pan,
  wheel zoom, **F** frame, **Esc** cancel loading, **H** help.
- **Пол** (Сводка): 20 procedural floors for contrast — studio grid, matte,
  light, checker, tiles, grass, dry grass, countryside, sand, asphalt,
  cobblestone, red brick, moon, Mars, lake, snow, clay, moss, blue and purple
  studio — or no surface,
- **Канал / Каркас / Оси / Габариты** (Сводка): show normals, UV, base colour,
  metal, roughness, AO or emission instead of lighting; triangle edges over the
  model; world axes and the bounding box with its size; «Пресет света» sets the
  lamp, key, fill and exposure in one choice,
- **Управление…** (Сводка): every key and mouse button, side buttons included,
  can be rebound (two chords per action, conflicts refused or moved, reset);
  saved to `%LOCALAPPDATA%\AssetDoctorRTPT\input-bindings.json`,
- **Гизмо модели**: X/Y/Z arrows move the model, the centre square scales it
  uniformly from the bottom centre; each release is one edit (Ctrl+Z undoes it,
  «Сохранить» writes the new root node transforms; primitives stay byte-exact),
- **Лампа**: a sun icon in the viewport — drag it to turn the light around the
  model, right click for its menu (reset, from above, from the camera, on/off,
  colour, temperature, hide); also in Сводка,
- **Окружение** (Сводка): sky background — dark studio, day, sunset, night,
  overcast, light studio; optional «Свет окружения» lights the model from the
  sky in PBR; «Скайбокс...» shows your own 2:1 PNG/JPEG panorama (read
  only, with yaw); view only, lamps and exposure stay as they are, at the model's lowest point with the step scaled to the model and
  red X / blue Z axes; **G** hides it or brings back the last theme. View only.
- **Диагностика**: geometry and UV checks G001–G012 (zero-area and duplicate
  triangles, unused vertices, missing/zero normals, boundary and non-manifold
  edges, connected groups, missing UV0, zero UV area, UV outside 0…1, exact
  duplicate vertices).
- Repairs with preview → Apply → Undo/Redo (Ctrl+Z / Ctrl+Y) on one source
  primitive: remove zero-area triangles (G001), duplicate triangles (G002) and
  unused vertices (G003); merge exact duplicate vertices (G012), refused if the
  topology would get worse; create missing normals (G004) or replace zero
  normals (G005) from area-weighted faces.
- **Очередь ремонтов**: runs every available repair one by one, re-analysing
  after each step; refused steps are skipped with their reason, a regression
  or failure stops it, and each step can be undone separately.
- **История ремонта** (in the queue window): past repairs, Undo/Redo and saves
  from all runs, stored in `%LOCALAPPDATA%\AssetDoctorRTPT\repair-history.jsonl`;
  clearly separate from the current model's diagnostics.
- **Сохранить** (Ctrl+S) replaces the working file after a one-time
  confirmation and the same verification; a file changed by another program
  is never overwritten.
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
