# Asset Doctor RT-PT 2.0

**Русская версия: [README.ru.md](README.ru.md)**

Native Windows application for diagnosing, repairing and preparing 3D assets,
written from scratch in Rust with a Vulkan renderer. Engineering prototype.

Current build: **AD RT-PT v2.5.2**. Versions are MAJOR.MINOR.PATCH by
significance: major reworks, notable features or large sets of fixes, and
hotfixes.

## Install and update

1. Download `AssetDoctorRTPT-win-x64.zip` from
   [Releases](https://github.com/ConstantineJJ/Asset-Doctor-RT-PT-2.0/releases).
   Optionally compare it with the `.sha256` file next to it.
2. Unpack it into a folder you can write to (not `Program Files`) and run
   `AssetDoctorRTPT.exe`. A GPU driver with Vulkan 1.1 or newer is required.
3. **Updates (ADR-0049).** The program checks this repository's releases once
   at start-up; turn it off in «Настройки → Обновления». You can also check by
   hand with «Проверить обновления…» there. A new version is installed only
   after you confirm it in the update window:
   - the package is downloaded and its SHA-256 verified;
   - the program closes;
   - `AssetDoctorRTPT-Updater.exe` replaces the files and starts the new
     version;
   - if the new version does not start, the previous one is restored.

   The update check is the program's only network access. It sends no user
   data, and your own panoramas in `assets\360.HDR` are kept.

## What it does today

- **Viewing.** Opens self-contained static **GLB** files and renders them with
  Vulkan: base colour, vertex colours, optional direct PBR with
  metallic/roughness, normal, occlusion and emission maps.
- **Layout (ADR-0045).**
  - Top bar: «Настройки» (help, keys and mouse, outline colour, updates) and
    the file buttons on the left. On the right, the drop-down menus «Вид»,
    «Сетка и помощники», «Освещение», «Лампа», «Экспозиция и сила света»,
    «Сцена», «Окружение», the «SkyBox» button and «Модель», then the outline
    («амёба») and lamp switches.
  - Right block, with tabs:
    - «Здоровье»: finding cards, repair list, queue, history;
    - «Инфо»;
    - «Rigging» (later).
  - A light grey strip under the top bar holds the «Work-Tree» button, which
    opens the left block.
  - Bottom-right corner: Undo/Redo and the views Front, Back, Left, Right, Top,
    Bottom (orthographic) and 3/4.
  - Status bar: the bug icon opens the program log. The window title and
    «Справка» show the version.
- **Mouse (ADR-0043, defaults).**
  - Left button selects and drags objects; right button opens the context
    menu.
  - Orbit: side button Forward or Shift+left drag. Pan: side button Back or
    Shift+right drag. Wheel: zoom.
  - **F** or middle click: frame the model.
  - **Tab**: outline. **X** / **Delete**: delete the object.
  - **Esc**: cancel loading. **H**: help.
  - Every key and mouse button can be rebound in «Клавиши и мышь…».
- **Objects (ADR-0048).**
  - Outline: the object under the cursor and the selected one are outlined.
    The click respects transparent MASK parts.
  - Edits: move with a left drag or the object gizmo, or delete. Each change
    is one edit (Ctrl+Z).
  - «Сохранить» writes the node matrices and removes deleted nodes; the GLB's
    other scenes are kept.
- **Work-Tree.** The node tree, with hide and colour marks; view only.
- **Model gizmo.** Arrows move the model, rings turn it, the centre square
  scales it. Exact values are in «Модель».
- **Light and scene.**
  - Lamp: a point light with azimuth, height and distance handles, and
    shadows on the model and the floor.
  - 20 procedural floors.
  - Render channels: normals, UV, base colour, metal, roughness, AO,
    emission.
  - Wireframe, axes, bounding box and light presets.
- **SkyBox and image-based lighting.**
  - The SkyBox window previews every 2:1 PNG/JPEG or Radiance HDR panorama
    in a folder. By default this is `assets/360.HDR`, with five CC0
    panoramas from Poly Haven.
  - Quality is 2K, 4K or 8K, within what the GPU allows.
  - With «Свет окружения» the panorama lights the model: diffuse from
    spherical harmonics, reflections by roughness.
- **Health (diagnostics G001–G012).** Zero-area and duplicate triangles,
  unused vertices, missing or zero normals, boundary and non-manifold edges,
  connected groups, missing UV0, zero UV area, UV outside 0…1, exact duplicate
  vertices.
- **Repairs.**
  - Each repair goes preview → Apply → Undo/Redo: G001, G002, G003, G012,
    G004, G005.
  - The repair queue runs them one by one, re-analysing after each step.
  - Repair history is kept across runs.
- **Saving.**
  - «Сохранить» (Ctrl+S) replaces the working file after confirmation and
    verification. It never overwrites a file changed by another program, and
    paths over 260 characters work.
  - «Сохранить копию…» writes a new, independently verified GLB.
- **View settings.** Kept between runs in
  `%LOCALAPPDATA%\AssetDoctorRTPT\view-settings.json`.

Skins, animations, morph targets, alpha BLEND, compressed geometry and external
resources are refused at import. Rigging, multi-model comparison, RT and PT are
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

The executables are `target/release/AssetDoctorRTPT.exe` and
`target/release/AssetDoctorRTPT-Updater.exe`. `--require-validation` fails if
Vulkan validation layers are unavailable. CI packages the program folder as
`AssetDoctorRTPT-win-x64.zip` and its `.sha256`. A release is published only
by a manual run with `publish` set.

## Source layout

- `crates/ad-document`: asset document, commands and candidate views
- `crates/ad-formats`: GLB import and verified export
- `crates/ad-analysis`: diagnostics
- `crates/ad-commands`: revision-checked edit history
- `crates/ad-heal`: repair planning and independent verification
- `crates/ad-storage`: verified save and new-file copies
- `crates/ad-render-vulkan`: Vulkan renderer
- `crates/ad-update`: update check, package verification and the updater
- `crates/ad-desktop`: the application
- `tests/fixtures`: small authored models and packages used by the tests
- `assets`: application icon and bundled panoramas

Product licensing has not been decided yet.
