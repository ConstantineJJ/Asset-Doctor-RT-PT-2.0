# Asset Doctor RT-PT 2.0

**Asset Doctor RT-PT 2.0** is a native Windows application in development for diagnosing, repairing, editing, and preparing 3D assets. The project is being built **from scratch** in Rust, with a Vulkan-based GPU renderer designed to run on local hardware.

> **Status:** Early development. The capabilities described below are planned features, not functionality that has already been implemented.

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

The project is in an early stage. Build instructions, system requirements, and licensing information will be added once the first working configuration is established and verified.
