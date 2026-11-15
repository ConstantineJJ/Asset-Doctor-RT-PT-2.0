# Asset Doctor RT-PT 2.0

**Asset Doctor RT-PT 2.0** is a native Windows application in development for diagnosing, repairing, editing, and preparing 3D assets. The project is being built **from scratch** in Rust, with a Vulkan-based GPU renderer designed to run on local hardware.

> **Status:** Native Vulkan GLB viewer with Open/Fit, scene tree, summary, embedded PNG/JPEG base-color textures and optional direct PBR preview with authored normals, material factors and metallic/roughness maps. Editing, normal/other material maps, rig/animation, RT and PT remain planned. This is an engineering prototype.

## Planned Features

- **3D Viewer:** GLB/glTF import, PBR materials, lighting, cameras, scene controls, and multi-model comparison.
- **Asset Diagnostics and Repair:** Analysis of geometry, topology, UVs, normals, materials, textures, skeletons, and animations; verifiable repairs with undo support.
- **Rigging and Animation:** Manual editing of bones, bindings, and animation clips, along with model editing and export.
- **Rendering:** Fast Vulkan rasterization and PBR first, followed by hardware-accelerated **Ray Tracing (RT)** and progressive **Path Tracing (PT)**.
- **Future Capabilities:** A Mixamo animation library (subject to licensing terms), automatic rig generation and assignment, manual rig adjustments, and a separate animation retargeting system.

## Technology and Design Principles

- **Rust:** Application core, asset operations, project state, and processing tasks.
- **Vulkan:** Native GPU rendering through `ash`, isolated from the asset document.
- **Native UI:** No React, Three.js, or WebView.
- **Modular Architecture:** Asset data and processing logic are independent of the GPU renderer and user interface.
- **Non-Destructive Workflow:** Original assets are preserved, with repair and export operations validated through checks and re-import tests.

## Development Roadmap

This is a new application, **not a continuation of the legacy Asset Doctor codebase**. The legacy application serves only as a local reference for existing features and behavior. Before recreating its functionality, the team will document its UI, settings, tools, and testable workflows.

Planned sequence: **Legacy feature audit → Rust core → Basic Vulkan renderer → Diagnostics and editing tools → RT → PT → Mixamo and automated rigging**. Initial project setup and the legacy audit may proceed in parallel.

## Repository

[ConstantineJJ / Asset-Doctor-RT-PT-2.0](https://github.com/ConstantineJJ/Asset-Doctor-RT-PT-2.0)

## Source layout and current evidence

`Main` is the source repository. Its parent is the local working area. Legacy
`E:\MyCreations\Asset-Doctor` remains a read-only behavior reference; its
architecture and implementation are not carried into this product.

- [Product specification](docs/PRODUCT_SPEC.md)
- [Independent architecture](docs/ARCHITECTURE.md) and [advisory proposal review](docs/ARCHITECTURE_PROPOSAL_REVIEW.md)
- [Functional inventory](reference/LEGACY_FEATURE_CATALOG.md): 66 initial functions/scenarios, 60 settings/control rows, 10 owner requests.
- [Parity matrix](docs/FEATURE_PARITY_MATRIX.csv) and [open audit gaps](reference/LEGACY_GAP_REGISTER.md).
- [Project pulse](Project-pulse.md), [tasks](tasks/todo.md), [dependencies](DEPENDENCIES.md), [test plan](docs/TEST_PLAN.md).

The supplied icon is preserved in `assets/app.png`; `assets/app.ico` is embedded
in the EXE and `assets/window-icon.png` is applied to the window.

Current runnable prototype: `target/release/AssetDoctorRTPT.exe` after release
build. Before loading a model it shows an opening hint. Click **Открыть…**, press **Ctrl+O** or
drop a self-contained static **.glb** into the window. For a known supported model,
use [Twin_Cubes.glb](tests/fixtures/Twin_Cubes.glb).

- Left drag: orbit; right/middle drag: pan; wheel: zoom.
- **F**: frame the model; **Esc**: cancel loading; **H**: controls and compatibility.
- **Дерево** opens the initially hidden scene list. Selecting a node shows its
  read-only details; geometry highlighting/editing is not implemented yet.
- **Сводка** shows source size, document nodes, expanded display counts and bounds.
  Optional panels hide in compact windows. Help includes embedded font notices.
- Loading failure/cancellation retains the previous model. Sources are read only.
- This increment supports opaque static triangles and embedded BIN geometry.
  It displays base-color factors × linear vertex colors × embedded PNG/JPEG maps
  with face lighting and OPAQUE/MASK coverage. **Базовые текстуры** in Summary
  toggles image sampling. Choose **PBR · направленный свет** in Summary for authored
  normals, metallic/roughness factors × maps, exposure and light intensity.
  **Карты металла/шероховатости** toggles data-map sampling independently of base
  color. This preview has approximate fill, no environment/shadows or normal maps.
  Default **Базовый вид** retains the previously accepted face-light appearance.
  External resources, referenced data-URI images, compressed geometry/images,
  texture-transform extensions, sparse accessors, alpha BLEND, skins, animations
  and morph targets are explicitly rejected. See the [compatibility and verification
  report](docs/STATIC_VIEWER_REPORT_2026-11-15.md) for exact scope.

The [viewer UI report](docs/VIEWER_UI_REPORT_2026-11-15.md) records current native
smoke results, headless input/layout tests and remaining visual/accessibility checks.
The [base-color report](docs/BASE_COLOR_REPORT_2026-11-15.md) adds image admission,
sampler/mipmap behavior and current textured-asset evidence.
The [direct PBR report](docs/DIRECT_PBR_REPORT_2026-11-15.md) records the normal,
material-factor and lighting increment. [Pbr_Factors.glb](tests/fixtures/Pbr_Factors.glb)
provides six smooth spheres for comparing metal and roughness settings.
The [MR texture report](docs/MR_TEXTURE_REPORT_2026-11-15.md) records independent UVs,
linear data sampling, shared-source image admission and native model checks.
Referenced MR resources now use the same strict admission as base-color maps,
including in the default view. Malformed/unsupported MR images retain the old model.

## Build on Windows

Install MSVC C++ Build Tools with Windows SDK, Rust MSVC stable and LunarG Vulkan
SDK (core includes validation layers). This workspace pins Rust 1.100.0. No full
Visual Studio IDE, WebView or Node.js is needed for the new product.

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo build --workspace --release --locked
cargo run --locked -p ad-desktop
cargo run --locked -p ad-render-vulkan -- --probe
cargo run --locked -p ad-render-vulkan -- --probe --require-validation
```

The local wrapper uses the installed MSVC toolchain and preserves the caller's
environment. Its explicit array preserves Cargo's argument separator:

```powershell
.\tools\dev.ps1 -Command @('test','--workspace','--locked')
.\tools\dev.ps1 -Command @('run','--locked','-p','ad-desktop','--','--require-validation')
.\tools\smoke.ps1
.\tools\import-smoke.ps1
.\tools\import-smoke.ps1 -SynchronizationValidation
.\tools\import-smoke.ps1 -Scene color -SynchronizationValidation
.\tools\import-smoke.ps1 -Scene pbr -Pbr -SynchronizationValidation
.\tools\import-smoke.ps1 -Scene mr -Pbr -SynchronizationValidation
.\tools\import-smoke.ps1 -Scene mr -Pbr -NoMrMaps -SynchronizationValidation
```

The separate probe prints actual GPU/API/heap/queue/extension availability. It does not
enable RT or create a swapchain. Without validation layers the ordinary probe
reports `validation_enabled=false`; `--require-validation` fails explicitly.
The native experiment requires swapchain-maintenance presentation fences; broader
device fallback is not implemented. It uses one in-flight frame and an
overlay-compatible UNORM target with explicit output transfer. PBR exposure and
Reinhard tone mapping are separate from the default base-color mode.
tools/smoke.ps1 requires validation, checks resize/minimize/restore/native close
and returns failure if any validation error occurs. It does not change overlay
settings. Compilation alone is not native acceptance. CI is prepared but has not
run remotely; nothing is pushed. Product licensing remains undecided.
