#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod camera;
mod diagnostic_ui;
mod imports;
mod native;
mod ui_model;
mod viewer_ui;

use ad_document::{DocumentId, ErrorCode};
use ad_render_vulkan::{
    SurfaceSize,
    context::GpuResult,
    renderer::{FrameResult, Renderer},
};
use camera::Camera;
use imports::{Imports, PreparedAsset};
use std::path::PathBuf;
use std::{
    io::Cursor,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Icon, Window, WindowId},
};
use winit::{
    event::{ElementState, MouseButton, MouseScrollDelta},
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
};

fn icon() -> GpuResult<Icon> {
    let decoder = png::Decoder::new(Cursor::new(include_bytes!(
        "../../../assets/window-icon.png"
    )));
    let mut reader = decoder.read_info()?;
    let mut bytes = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or("Icon output size overflow")?
    ];
    let info = reader.next_frame(&mut bytes)?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err("Bundled icon must be 8-bit RGBA PNG".into());
    }
    bytes.truncate(info.buffer_size());
    Ok(Icon::from_rgba(bytes, info.width, info.height)?)
}

#[derive(Debug)]
enum SmokeStage {
    Initial,
    Resizing(u64),
    Minimizing(Instant),
    Restoring(u64),
    Closing,
}

struct App {
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    error: Option<String>,
    validation: bool,
    smoke: Option<SmokeStage>,
    started: Instant,
    resize_events: u32,
    saw_resize: bool,
    saw_minimized: bool,
    saw_restore: bool,
    minimized_frame_baseline: Option<u64>,
    close_requested: bool,
    suspended: bool,
    closed: bool,
    imports: Imports,
    importing: bool,
    current: Option<PreparedAsset>,
    camera: Camera,
    mouse: Option<MouseButton>,
    cursor: Option<(f64, f64)>,
    modifiers: ModifiersState,
    initial_path: Option<PathBuf>,
    smoke_import: bool,
    smoke_bad: Option<PathBuf>,
    expected_tag: Option<(DocumentId, u64)>,
    failed_replacement_preserved: bool,
    ui: viewer_ui::ViewerUi,
}

impl App {
    fn finish(&mut self, event_loop: &ActiveEventLoop) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.imports.cancel();
        if let Some(renderer) = self.renderer.take() {
            let context = renderer.context();
            let frames = renderer.frames;
            let mesh_frames = renderer.mesh_frames;
            let pbr_frames = renderer.pbr_frames;
            let mr_frames = renderer.mr_frames;
            let normal_frames = renderer.normal_frames;
            let ao_frames = renderer.occlusion_frames;
            let emission_frames = renderer.emission_frames;
            drop(renderer); // Retire ALL Vulkan objects while native window is alive.
            let errors = context.validation_errors();
            eprintln!(
                "native_shutdown frames={frames} resize_events={} minimized={} restored={} validation_errors={errors}",
                self.resize_events, self.saw_minimized, self.saw_restore
            );
            if errors > 0 {
                self.error
                    .get_or_insert(format!("{errors} Vulkan validation errors"));
            }
            let pbr_required =
                self.ui.lighting.mode == ad_render_vulkan::lighting::ShadingMode::DirectPbr;
            let mr_required = pbr_required
                && self.ui.mr_maps
                && self.current.as_ref().is_some_and(|asset| {
                    asset
                        .snapshot
                        .draws
                        .iter()
                        .any(|d| d.metallic_roughness_texture.is_some())
                });
            let normal_required = pbr_required
                && self.ui.normal_maps
                && self.current.as_ref().is_some_and(|asset| {
                    asset
                        .snapshot
                        .draws
                        .iter()
                        .any(|d| d.normal_texture.is_some())
                });
            let ao_required = pbr_required
                && self.ui.ao_maps
                && self.current.as_ref().is_some_and(|asset| {
                    asset
                        .snapshot
                        .draws
                        .iter()
                        .any(|d| d.occlusion_texture.is_some())
                });
            let emission_required = pbr_required
                && self.ui.emission
                && self.current.as_ref().is_some_and(|asset| {
                    asset
                        .snapshot
                        .draws
                        .iter()
                        .any(|d| d.emissive_factor.iter().any(|&v| v > 0.))
                });
            if self.smoke.is_some()
                && self.error.is_none()
                && self.smoke_import
                && (self.current.is_none()
                    || !self.failed_replacement_preserved
                    || mesh_frames < 15
                    || (pbr_required && pbr_frames < 15)
                    || (mr_required && mr_frames < 15)
                    || (normal_required && normal_frames < 15)
                    || (ao_required && ao_frames < 15)
                    || (emission_required && emission_frames < 15))
            {
                self.error = Some("Import/rejected-replacement smoke did not complete".into());
            }
            if self.smoke.is_some() && self.error.is_none() {
                if !self.saw_resize
                    || !self.saw_minimized
                    || !self.saw_restore
                    || !self.close_requested
                    || frames < 15
                {
                    self.error = Some(
                        "Native smoke did not observe every required lifecycle transition".into(),
                    );
                } else {
                    eprintln!(
                        "asset_smoke imported={} mesh_frames={mesh_frames} rejected_replacement_preserved={}",
                        self.current.is_some(),
                        self.failed_replacement_preserved
                    );
                    eprintln!(
                        "lighting_smoke mode={:?} pbr_frames={pbr_frames} mr_frames={mr_frames} normal_frames={normal_frames} ao_frames={ao_frames} emission_frames={emission_frames}",
                        self.ui.lighting.mode
                    );
                    eprintln!(
                        "SMOKE_PASS frames={frames} resize=true minimize=true restore=true close_requested=true validation_enabled={} validation_errors=0",
                        context.validation
                    );
                }
            }
        }
        event_loop.exit();
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl std::fmt::Display) {
        self.error = Some(error.to_string());
        self.finish(event_loop);
    }

    fn create(&mut self, event_loop: &ActiveEventLoop) -> GpuResult<()> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Asset Doctor RT-PT 2.0 | Static GLB viewer prototype")
                    .with_inner_size(PhysicalSize::new(1200, 760))
                    .with_min_inner_size(PhysicalSize::new(320, 240))
                    .with_window_icon(Some(icon()?)),
            )?,
        );
        let size = window.inner_size();
        let renderer = Renderer::new(
            window.clone(),
            SurfaceSize {
                width: size.width,
                height: size.height,
            },
            self.validation,
        )?;
        eprintln!(
            "native_window_ready size={}x{} icon=owner_asset",
            size.width, size.height
        );
        self.window = Some(window);
        self.ui
            .attach(self.window.as_ref().expect("Window created"));
        self.renderer = Some(renderer);
        Ok(())
    }

    fn start_import(&mut self, path: PathBuf) {
        let ticket = self.imports.request(path);
        self.importing = true;
        self.ui.error = None;
        eprintln!("import_started ticket={ticket}");
        if let Some(window) = self.window.as_ref() {
            window.set_title("Asset Doctor RT-PT 2.0 — Loading GLB… | Esc: cancel");
        }
    }
    fn title(&self) {
        if let Some(window) = self.window.as_ref() {
            let title = if let Some(asset) = self.current.as_ref() {
                format!(
                    "Asset Doctor RT-PT 2.0 — {} | {} vertices · {} triangles | Ctrl+O: open · F: fit · H: help",
                    asset.document.name(),
                    asset.snapshot.positions.len(),
                    asset.snapshot.indices.len() / 3
                )
            } else {
                "Asset Doctor RT-PT 2.0 — Ctrl+O: open GLB · Drop GLB into window · H: help".into()
            };
            window.set_title(&title);
        }
    }
    fn poll_import(&mut self) -> GpuResult<()> {
        let Some(completion) = self.imports.poll() else {
            return Ok(());
        };
        if completion.ticket != self.imports.ticket() {
            return Ok(());
        }
        self.importing = false;
        match completion.result {
            Ok(asset) => {
                if let Err(error) = self
                    .renderer
                    .as_mut()
                    .ok_or("No renderer for imported asset")?
                    .set_scene(&asset.snapshot)
                {
                    if self.smoke.is_some() {
                        return Err(error);
                    }
                    native::show_error(&format!(
                        "Cannot display model: {error}. The previous model is retained."
                    ));
                    self.title();
                    return Ok(());
                }
                self.camera.fit(asset.snapshot.bounds);
                self.ui.tree.replace(&asset.document);
                self.ui.status = format!("Открыт {}", asset.document.name());
                self.ui.error = None;
                self.current = Some(asset);
                eprintln!("import_published ticket={}", completion.ticket);
            }
            Err(error) => {
                eprintln!(
                    "import_failed ticket={} code={:?}",
                    completion.ticket, error.code
                );
                if let Some(expected) = self.expected_tag {
                    let actual = self.renderer.as_ref().and_then(Renderer::scene_tag);
                    if actual != Some(expected)
                        || self
                            .current
                            .as_ref()
                            .is_none_or(|asset| asset.document.id() != expected.0)
                    {
                        return Err("Failed import replaced the previous document/GPU scene".into());
                    }
                    self.failed_replacement_preserved = true;
                    eprintln!("rejected_replacement_preserved document={}", expected.0.0);
                } else if self.smoke_import {
                    return Err(error.into());
                } else if error.code != ErrorCode::Cancelled {
                    self.ui.error = Some(error.to_string().chars().take(512).collect());
                    self.ui.status = "Загрузка не выполнена; предыдущая модель сохранена".into();
                    self.title();
                    native::show_error(&error.to_string());
                }
            }
        }
        self.title();
        Ok(())
    }
    fn choose_model(&mut self) {
        if let Some(window) = self.window.as_ref() {
            match native::choose_model(window) {
                Ok(Some(path)) => self.start_import(path),
                Ok(None) => (),
                Err(error) => native::show_error(&error.to_string()),
            }
        }
    }

    fn cancel_import(&mut self) {
        self.imports.cancel();
        self.importing = false;
        self.ui.status = "Загрузка отменена".into();
        self.title();
    }

    fn redraw(&mut self) -> GpuResult<()> {
        let Some(window) = self.window.clone() else {
            return Ok(());
        };
        if self.renderer.as_ref().is_none_or(|r| !r.drawable()) {
            return Ok(());
        }
        let (mut frame, actions) = self
            .ui
            .frame(&window, self.current.as_ref(), self.importing);
        if actions.cancel {
            self.cancel_import();
        }
        if actions.fit
            && let Some(asset) = &self.current
        {
            self.camera.fit(asset.snapshot.bounds);
        }
        let renderer = self.renderer.as_mut().expect("Drawable renderer checked");
        let (width, height) = frame.scene.map_or((1, 1), |r| (r.width, r.height));
        renderer.set_camera(
            self.camera.matrix(width, height),
            self.camera.eye(width, height),
        )?;
        renderer.set_textures_enabled(self.ui.textures);
        renderer.set_mr_maps_enabled(self.ui.mr_maps);
        renderer.set_normal_maps_enabled(self.ui.normal_maps);
        renderer.set_occlusion_maps_enabled(self.ui.ao_maps);
        renderer.set_emission_enabled(self.ui.emission);
        renderer.set_lighting(self.ui.lighting)?;
        let result = renderer.draw_ui(&mut frame)?;
        if result == FrameResult::Recreate {
            let size = window.inner_size();
            renderer.recreate(SurfaceSize {
                width: size.width,
                height: size.height,
            })?;
        }
        // Open native dialogs only after the acquired image has been submitted.
        if actions.open {
            self.choose_model();
        }
        Ok(())
    }

    fn tick(&mut self, event_loop: &ActiveEventLoop) -> GpuResult<()> {
        if self.closed {
            return Ok(());
        }
        self.poll_import()?;
        if self.smoke_import
            && matches!(self.smoke, Some(SmokeStage::Initial))
            && self.current.is_some()
            && !self.importing
            && let Some(path) = self.smoke_bad.take()
        {
            self.expected_tag = self.renderer.as_ref().and_then(Renderer::scene_tag);
            self.start_import(path);
            return Ok(());
        }
        let Some(window) = self.window.as_ref() else {
            return Ok(());
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return Ok(());
        };
        let minimized = window.is_minimized().unwrap_or(false);
        if minimized && !self.suspended {
            renderer.recreate(SurfaceSize {
                width: 0,
                height: 0,
            })?;
            self.suspended = true;
            self.saw_minimized = true;
            self.minimized_frame_baseline = Some(renderer.frames);
            eprintln!("native_minimized frames={}", renderer.frames);
        } else if !minimized && self.suspended {
            let size = window.inner_size();
            if size.width > 0 && size.height > 0 {
                renderer.recreate(SurfaceSize {
                    width: size.width,
                    height: size.height,
                })?;
                self.suspended = false;
                self.saw_restore = true;
                eprintln!("native_restored frames={}", renderer.frames);
            }
        }
        if self.smoke_import
            && (self.current.is_none() || self.importing || !self.failed_replacement_preserved)
        {
            if self.started.elapsed() > Duration::from_secs(20) {
                return Err("Import smoke timed out".into());
            }
        } else if let Some(stage) = self.smoke.as_mut() {
            if self.started.elapsed() > Duration::from_secs(20) {
                return Err(format!("Native smoke timed out at {stage:?}").into());
            }
            let frames = if self.smoke_import {
                renderer.mesh_frames
            } else {
                renderer.frames
            };
            match stage {
                SmokeStage::Initial if frames >= 5 => {
                    self.camera.orbit(40., -20.);
                    self.camera.pan(10., 5., window.inner_size().height as f32);
                    self.camera.zoom(1.);
                    eprintln!("camera_smoke orbit=true pan=true zoom=true");
                    let _ = window.request_inner_size(PhysicalSize::new(960, 600));
                    *stage = SmokeStage::Resizing(frames);
                    eprintln!("smoke_request_resize");
                }
                SmokeStage::Resizing(before) if self.saw_resize && frames >= *before + 5 => {
                    window.set_minimized(true);
                    *stage = SmokeStage::Minimizing(Instant::now());
                    eprintln!("smoke_request_minimize");
                }
                SmokeStage::Minimizing(at)
                    if self.saw_minimized && at.elapsed() > Duration::from_millis(500) =>
                {
                    if self.minimized_frame_baseline != Some(renderer.frames) {
                        return Err("Frames were submitted while minimized".into());
                    }
                    window.set_minimized(false);
                    *stage = SmokeStage::Restoring(frames);
                    eprintln!("smoke_request_restore");
                }
                SmokeStage::Restoring(before) if self.saw_restore && frames >= *before + 5 => {
                    native::request_close(window)?;
                    *stage = SmokeStage::Closing;
                    eprintln!("smoke_post_native_close");
                }
                _ => (),
            }
        }
        if !self.suspended {
            window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none()
            && let Err(error) = self.create(event_loop)
        {
            self.fail(event_loop, error);
        }
        if self.renderer.is_some() {
            if let Some(path) = self.initial_path.take() {
                self.start_import(path);
            } else {
                self.title();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let consumed = self
            .window
            .as_ref()
            .is_some_and(|window| self.ui.on_event(window, &event));
        let result: GpuResult<()> = match event {
            WindowEvent::Focused(false) => {
                self.mouse = None;
                self.cursor = None;
                self.modifiers = ModifiersState::default();
                Ok(())
            }
            WindowEvent::DroppedFile(path) => {
                self.start_import(path);
                Ok(())
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
                Ok(())
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && !event.repeat
                    && (!self.ui.context.text_edit_focused()
                        || event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                        || (event.physical_key == PhysicalKey::Code(KeyCode::KeyO)
                            && self.modifiers.control_key())) =>
            {
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::KeyO) if self.modifiers.control_key() => {
                        self.choose_model()
                    }
                    PhysicalKey::Code(KeyCode::KeyF) => {
                        if let Some(asset) = self.current.as_ref() {
                            self.camera.fit(asset.snapshot.bounds);
                        }
                    }
                    PhysicalKey::Code(KeyCode::Escape) => {
                        self.ui.dismiss_help();
                        if self.importing {
                            self.cancel_import();
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyH) => self.ui.help(),
                    _ => (),
                }
                Ok(())
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let inside = self.cursor.is_some_and(|(x, y)| self.ui.camera_at(x, y));
                if state == ElementState::Pressed && !consumed && inside {
                    self.mouse = Some(button);
                } else if self.mouse == Some(button) {
                    self.mouse = None;
                }
                Ok(())
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some((x, y)) = self.cursor {
                    let dx = (position.x - x) as f32;
                    let dy = (position.y - y) as f32;
                    match self.mouse {
                        Some(MouseButton::Left) => self.camera.orbit(dx, dy),
                        Some(MouseButton::Right | MouseButton::Middle) => {
                            self.camera.pan(dx, dy, self.ui.camera_height())
                        }
                        _ => (),
                    }
                }
                self.cursor = Some((position.x, position.y));
                Ok(())
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                Ok(())
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 80.0,
                };
                let inside = self.cursor.is_some_and(|(x, y)| self.ui.camera_at(x, y));
                if !consumed && inside {
                    self.camera.zoom(steps);
                }
                Ok(())
            }
            WindowEvent::CloseRequested => {
                self.close_requested = true;
                eprintln!("native_close_requested");
                self.finish(event_loop);
                Ok(())
            }
            WindowEvent::Resized(size) => {
                self.resize_events += 1;
                self.saw_resize |= size == PhysicalSize::new(960, 600);
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.recreate(SurfaceSize {
                        width: size.width,
                        height: size.height,
                    })
                } else {
                    Ok(())
                }
            }
            WindowEvent::RedrawRequested
                if !self.suspended
                    && self
                        .window
                        .as_ref()
                        .is_some_and(|window| window.is_minimized() != Some(true)) =>
            {
                self.redraw()
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.tick(event_loop) {
            self.fail(event_loop, error);
        }
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.finish(event_loop);
    }
}

fn run() -> GpuResult<()> {
    let mut validation = false;
    let mut smoke = false;
    let mut initial_path = None;
    let mut smoke_import = false;
    let mut smoke_bad = None;
    let mut ui = viewer_ui::ViewerUi::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--require-validation" => validation = true,
            "--pbr" => ui.lighting.mode = ad_render_vulkan::lighting::ShadingMode::DirectPbr,
            "--no-mr-maps" => ui.mr_maps = false,
            "--no-normal-maps" => ui.normal_maps = false,
            "--no-ao-maps" => ui.ao_maps = false,
            "--no-emission" => ui.emission = false,
            "--smoke" => {
                smoke = true;
                validation = true;
            }
            "--open" => {
                initial_path = Some(PathBuf::from(args.next().ok_or("--open needs a GLB path")?))
            }
            "--smoke-import" => {
                initial_path = Some(PathBuf::from(
                    args.next().ok_or("--smoke-import needs a GLB path")?,
                ));
                smoke_import = true;
                smoke = true;
                validation = true;
            }
            "--smoke-bad" => {
                smoke_bad = Some(PathBuf::from(
                    args.next().ok_or("--smoke-bad needs a damaged GLB path")?,
                ))
            }
            "--help" => {
                println!(
                    "AssetDoctorRTPT [--require-validation] [--pbr] [--no-mr-maps] [--no-normal-maps] [--no-ao-maps] [--no-emission] [--open model.glb] [--smoke] [--smoke-import model.glb --smoke-bad bad.glb]"
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    let event_loop = EventLoop::new()?;
    let mut app = App {
        renderer: None,
        window: None,
        error: None,
        validation,
        smoke: smoke.then_some(SmokeStage::Initial),
        started: Instant::now(),
        resize_events: 0,
        saw_resize: false,
        saw_minimized: false,
        saw_restore: false,
        minimized_frame_baseline: None,
        close_requested: false,
        suspended: false,
        closed: false,
        imports: Imports::new(),
        importing: false,
        current: None,
        camera: Camera::default(),
        mouse: None,
        cursor: None,
        modifiers: ModifiersState::default(),
        initial_path,
        smoke_import,
        smoke_bad,
        expected_tag: None,
        failed_replacement_preserved: false,
        ui,
    };
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error.into());
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Asset Doctor startup/runtime error: {error}");
        if !std::env::args().any(|arg| matches!(arg.as_str(), "--smoke" | "--smoke-import")) {
            native::show_error(&error.to_string());
        }
        std::process::exit(1);
    }
}
