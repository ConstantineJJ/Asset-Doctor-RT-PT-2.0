# Asset Doctor RT-PT 2.0

Read README.md, Project-pulse.md and tasks/todo.md before changing the project.

- Product/source root is Main. Its parent is the working area, not the repository.
- Legacy E:\MyCreations\Asset-Doctor and installed E:\MyCreations\Asset Doctor are read-only references. Never execute, copy as implementation, modify, reset or clean Legacy.
- Rust + native Vulkan. No React, Three.js, Tauri or WebView in this product.
- Audit observations, owner requests, source verification and runtime verification are distinct. A source control count is not a runtime UI count.
- Do not implement product operations until the relevant audit and contracts are complete. An isolated GPU spike is allowed.
- Domain data cannot depend on window or GPU types. Renderer consumes document snapshots; edits go through revision-checked commands.
- Preserve original assets. No silent repair/export, network resource fetching, overwrite or stale job publication.
- Record dependency purpose/version/license/replacement in DEPENDENCIES.md. Commit Cargo.lock. Product licensing remains the owner's decision.
- Use small checked increments; update Project-pulse.md with checks, limitations and the next concrete step. Never call compilation a native/GPU acceptance pass.
- Verify cargo fmt --all --check, cargo clippy --workspace --all-targets --locked -- -D warnings, cargo test --workspace --locked and Windows build. Report unrun checks.
- Local tools may live in ../.toolchain; never commit tools, reports containing private paths, generated binaries or downloaded SDKs. Use tools/dev.ps1.
- Remote publication is separate from local development; do not push without an explicit request.
