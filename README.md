# Asset Doctor RT-PT 2.0

**Asset Doctor RT-PT 2.0** is a native Windows application in development for diagnosing, repairing, editing, and preparing 3D assets. The project is being built **from scratch** in Rust, with a Vulkan-based GPU renderer designed to run on local hardware.

> **Status:** Bootstrap. An independent Rust workspace and Vulkan capability probe now exist. The product features below remain planned. No native viewport/first presented frame, importer, repair tools, RT or PT is claimed yet.

## Planned Features

- **3D Viewer:** GLB/glTF import, PBR materials, lighting, cameras, scene controls, and multi-model comparison.
- **Asset Diagnostics and Repair:** Analysis of geometry, topology, UVs, normals, materials, textures, skeletons, and animations; verifiable repairs with undo support.
- **Rigging and Animation:** Manual editing of bones, bindings, and animation clips, along with model editing and export.
- **Rendering:** Fast Vulkan rasterization and PBR first, followed by hardware-accelerated **Ray Tracing (RT)** and progressive **Path Tracing (PT)**.
- **Future Capabilities:** A Mixamo animation library (subject to licensing terms), automatic rig generation and assignment, manual rig adjustments, and a separate animation retargeting system.

## Technology and Design Principles

- **Rust:** Application core, asset operations, project state, and processing tasks.
- **Vulkan:** Native GPU rendering; `ash` is being considered as the initial integration library.
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

The supplied icon is preserved in `assets/app.png`; Windows ICO variants are
prepared in `assets/app.ico`. Actual window/executable resource integration is
part of the next native slice.

## Build on Windows

Install MSVC C++ Build Tools with Windows SDK, Rust MSVC stable and LunarG Vulkan
SDK (core includes validation layers). This workspace pins Rust 1.100.0. No full
Visual Studio IDE, WebView or Node.js is needed for the new product.

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo run --locked -p ad-render-vulkan -- --probe
cargo run --locked -p ad-render-vulkan -- --probe --require-validation
```

For this machine's isolated portable GNU fallback, use the local wrapper with an
explicit array so PowerShell preserves Cargo's argument separator:

```powershell
.\tools\dev.ps1 -Command @('test','--workspace','--locked')
.\tools\dev.ps1 -Command @('run','--locked','-p','ad-render-vulkan','--','--probe')
```

The probe prints actual GPU/API/heap/queue/extension availability. It does not
enable RT or create a swapchain. Without validation layers the ordinary probe
reports `validation_enabled=false`; `--require-validation` fails explicitly.
Compilation is not GPU/native UI acceptance. CI is prepared but has not been run
remotely; nothing is pushed by bootstrap work. Product licensing remains undecided.
