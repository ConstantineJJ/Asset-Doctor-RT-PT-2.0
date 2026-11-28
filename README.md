# Asset Doctor RT-PT 2.0

**Русская версия: [README.ru.md](README.ru.md)**

Native Windows application for diagnosing, repairing and preparing 3D assets,
written from scratch in Rust with a Vulkan renderer. Engineering prototype.

Current build: **AD RT-PT v2.11.0**. Versions are MAJOR.MINOR.PATCH by
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
  - Top bar: «Настройки» (help, keys and mouse, outline colour, window
    mode, updates, Exit) and the file buttons on the left. On the right, the drop-down menus «Вид»,
    «Сетка и помощники», «Освещение», «Лампа», «Экспозиция и сила света»,
    «Сцена», «Окружение», the «SkyBox» button and «Модель», then the outline
    («амёба») and lamp switches.
  - Right block, with tabs:
    - «Здоровье»: finding cards, repair list, queue, history;
    - «Инфо»;
    - «Rigging»: skins, joints and animation clips (see below).
  - A light grey strip under the top bar holds the «Work-Tree» button, which
    opens the left block. **Q** / **E** hide or show the left / right block.
  - Interface scale 75–200 % («Настройки → Окно»). The mouse wheel over a
    field, slider or list steps its value.
  - Window: a normal window or full screen (**F11**; **Esc** returns to the
    window). **F1** removes the window's frame and title bar (and brings
    them back).
  - Bottom-right corner: Undo/Redo and the views Front, Back, Left, Right, Top,
    Bottom (orthographic) and 3/4.
  - Status bar: the bug icon opens the program log. The window title and
    «Справка» show the version.
- **Mouse (ADR-0043, defaults).**
  - Left button selects and drags objects; right button opens the context
    menu.
  - Multi-selection: drag on empty space for a selection box, **Shift**+click
    adds an object, **Ctrl**+click (or Ctrl + box) removes it.
  - Orbit: side button Forward or Shift+left drag. Pan: side button Back or
    Shift+right drag. Wheel: zoom.
  - **F**: frame the model.
  - Middle click: **grab**. The selection (or the object under the pointer)
    follows the pointer; a left or middle click puts it down, a right click or
    **Esc** cancels.
  - **Tab**: outline (width 1–32 px, dynamic contrast or 18 colours incl.
    bright neon ones, in «Настройки» and the right-click menu). **X** / **Delete**:
    delete the selected objects. **Z**: show or hide the rig.
  - **Esc**: cancel loading. **H**: help.
  - Every key and mouse button can be rebound in «Клавиши и мышь…».
- **Objects (ADR-0048).**
  - Outline: the object under the cursor and the selected ones are outlined.
    The click respects transparent MASK parts and has a few pixels of
    tolerance for small or far objects.
  - The gizmo appears on the selection by itself and moves, turns and scales
    the whole selection about its centre. With nothing selected it works on
    the whole model.
  - Edits: move with the gizmo, a left drag or a grab (the whole selection
    moves together), or delete. Each change, also for a group, is one edit
    (Ctrl+Z).
  - «Сохранить» writes the node matrices and removes deleted nodes; the GLB's
    other scenes are kept.
- **Work-Tree.** The node tree, with hide and colour marks; view only.
- **Rigging (P7).**
  - Opens GLB files with skins (JOINTS_0/WEIGHTS_0) and TRS animation clips
    (STEP, LINEAR, CUBICSPLINE).
  - The skeleton is drawn over the model in the current pose («Сетка и
    помощники → Скелет»); click a joint to select it. The Rigging tab shows
    the joint tree.
  - Playback bar at the bottom of the viewport: clip, previous / play-pause
    / next animation, stop, loop, speed and a timeline with the keys
    (click or drag to scrub). **Space** plays and pauses, **,** / **.** step
    one frame (1/30 s); speed 0.1–3× in steps of 0.1. The clip list has a
    search field. A model with clips starts its first clip when it opens
    («Настройки → Анимация»).
  - Joints: rename the selected joint; add a child joint (along the parent's
    bone or a local axis, chosen length, no weights); change a bone's length
    or a joint's rest position — the skeleton moves, the mesh stays (rebind);
    delete a joint with
    everything under it — its weights go to the nearest joint above, its
    channels leave the clips. Rigging card «Сустав» or the joint's
    right-click menu; one Undo step each, saved and verified. «Сбросить риг
    к файлу» brings the file's skeleton back in one step (clips keep their
    keys).
  - Root motion: how far the skeleton's root (or the selected joint) moves
    during the clip — displacement, distance on the floor, path, speed,
    turn and height — with its path drawn in the viewport.
  - The Rigging tab: creating clips (yellow), the chosen clip (blue) and
    its keys (pink) in the top third, the joint tree below. Edit a joint's
    keys; create, copy, rename, delete and retime clips; all with Undo/Redo.
  - Right click on a joint (tree or viewport), the timeline or the Rigging
    tab: select, key the current pose, delete a key, clip actions.
  - Joint gizmo (clip paused): rings turn the selected joint, arrows move
    it, local or world axes; releasing writes a key at the current time.
  - Saving writes the animation back and verifies it by reimporting.
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

Transparent (alpha BLEND) materials are drawn blended, back to front. A
required `KHR_lights_punctual` is left out (the program's lamp lights the
scene), and common export slips (`texCoord: -1`, a texture without its UV set)
open with a note instead of a refusal. Morph targets, compressed geometry and
external resources are
refused at import. Geometry and object edits are not yet available for skinned
or animated models. GPU skinning, multi-model comparison, RT and PT are
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
`AssetDoctorRTPT-win-x64.zip` and its `.sha256`. A release is published by
pushing a tag `vX.Y.Z` that equals the Cargo version and points at a commit of
`main`; the annotated tag's message becomes the release notes. A manual run
with `publish` set is the fallback. Memory budgets are generous: GLB up to
1 GiB, textures up to 2 GiB and 16384 px.

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

Licensed under the MIT License (see [LICENSE](LICENSE)).
