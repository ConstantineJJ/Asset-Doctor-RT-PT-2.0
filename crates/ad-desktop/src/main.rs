#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod camera;
mod diagnostic_ui;
mod edits;
mod helpers;
mod history;
mod imports;
mod input;
mod lamp;
mod model_gizmo;
mod native;
mod queue;
mod saving;
mod skybox;
mod ui_model;
mod viewer_ui;
mod worker;

use ad_document::{Cancellation, DocumentId, ErrorCode, MeshId};
use ad_render_vulkan::{
    SurfaceSize,
    context::GpuResult,
    renderer::{FrameResult, Renderer, SceneFault},
};
use camera::Camera;
use edits::{
    EditCandidate, EditInput, EditKind, EditOutcome, Edits, PublishError, new_edits, publish_edit,
};
use imports::{Imports, PreparedAsset, new_imports};
use saving::{AfterSave, Confirmed, Guard, SaveInput, SaveTarget, Saves, guard, new_saves};
use std::collections::VecDeque;
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
    /// ADR-0043: drag action of the held mouse button.
    mouse_drag: Option<input::Action>,
    /// Bindings file (None in smoke runs without --bindings: never written).
    bindings_path: Option<PathBuf>,
    /// View settings file (None in smoke runs without --view-settings), the
    /// settings last read or written, and a change waiting to settle.
    view_settings_path: Option<PathBuf>,
    view_saved: serde_json::Value,
    view_pending: Option<(serde_json::Value, Instant)>,
    /// A skybox restored from the settings keeps its saved «on» flag.
    skybox_restore_on: Option<bool>,
    smoke_view_change: bool,
    smoke_input: bool,
    input_smoke_done: bool,
    cursor: Option<(f64, f64)>,
    modifiers: ModifiersState,
    initial_path: Option<PathBuf>,
    smoke_import: bool,
    smoke_bad: Option<PathBuf>,
    expected_tag: Option<(DocumentId, u64)>,
    failed_replacement_preserved: bool,
    /// ADR-0031 B3 smoke: re-import the good model with an injected GPU stage
    /// failure and prove the published scene/document stay current.
    smoke_gpu_fault: Option<SceneFault>,
    smoke_good: Option<PathBuf>,
    gpu_fault_armed: bool,
    gpu_fault_preserved: bool,
    edits: Edits,
    editing: bool,
    edit_busy: String,
    edit_started: Instant,
    /// Close/open/drop requested; resolved with the unsaved guard in tick.
    /// Modal dialogs shown from inside a winit window-event handler stay
    /// invisible while the owner window is disabled (observed as a hang).
    pending_guard: Option<AfterSave>,
    /// Save Copy requested; the native dialog opens from tick, not an event.
    pending_save: bool,
    /// «Сохранить» requested (Ctrl+S or button); runs from tick.
    pending_overwrite: bool,
    /// ADR-0040 skybox: dialog requested, background decode, decoded image
    /// waiting for upload.
    pending_skybox_choose: bool,
    /// Viewport size of the last frame (lamp gizmo projection).
    last_view: (u32, u32),
    /// ADR-0041 gizmo drag preview, kept until its placement edit finishes.
    model_preview: Option<model_gizmo::Delta>,
    model_pending: bool,
    /// Ring being turned (its knob follows the preview).
    model_turning: Option<usize>,
    /// Smoke: place, Undo, Redo, then a verified export of the placement.
    smoke_place: bool,
    place_smoke_done: bool,
    skybox_job: Option<std::sync::mpsc::Receiver<Result<skybox::Loaded, String>>>,
    pending_skybox: Option<skybox::Loaded>,
    /// File the current model was opened from (or last saved and reopened).
    working_path: Option<PathBuf>,
    /// Path of the import in flight; becomes the working path when published.
    pending_path: Option<PathBuf>,
    /// Working file whose overwrite the user already confirmed once.
    overwrite_confirmed: Option<PathBuf>,
    /// Safe repair queue (HEAL-02, ADR-0038); None when not started.
    queue: Option<queue::Queue>,
    pending_queue_start: bool,
    pending_queue_stop: bool,
    smoke_queue: bool,
    queue_smoke_done: bool,
    /// Repair report history (HEAL-03, ADR-0039); None when disabled.
    history: Option<history::History>,
    /// Prepared, verified but unpublished G002 repair shown in Preview.
    pending_preview: Option<EditCandidate>,
    /// ADR-0031 B4 smoke without UI controls: queued edits on the G002 mesh,
    /// the first optionally with an injected GPU failure.
    smoke_edit: bool,
    edit_fault: Option<SceneFault>,
    edit_queue: VecDeque<EditKind>,
    edit_mesh: Option<MeshId>,
    edit_fault_expected: bool,
    edit_smoke_done: bool,
    saves: Saves,
    saving: bool,
    /// Last state known to exist on disk for the current model (decision 2).
    confirmed: Option<Confirmed>,
    after_save: Option<AfterSave>,
    /// Smoke-only Save Copy sequence: refused existing target, new copy, reopen.
    save_smoke: Option<(PathBuf, u8)>,
    /// ADR-0035 smoke: 0 outside edit + refused Save, 1 waiting, 2 Save,
    /// 3 waiting, 4 reopening, 5 done.
    overwrite_smoke: Option<u8>,
    /// Smoke-only close guard: 1 = answer Cancel, 2 = answer No, 3 = done.
    smoke_guard: u8,
    /// Repair kind exercised by --smoke-edit (default G002).
    smoke_repair: ad_heal::RepairKind,
    ui: viewer_ui::ViewerUi,
}

impl App {
    fn finish(&mut self, event_loop: &ActiveEventLoop) {
        if self.closed {
            return;
        }
        self.save_view_settings(true);
        self.closed = true;
        self.imports.cancel();
        if let Some(renderer) = self.renderer.take() {
            let context = renderer.context();
            let frames = renderer.frames;
            let mesh_frames = renderer.mesh_frames;
            let pbr_frames = renderer.pbr_frames;
            let grid_frames = renderer.grid_frames();
            let sky_frames = renderer.sky_frames();
            let environment_light_frames = renderer.environment_light_frames();
            let panorama_frames = renderer.panorama_frames();
            let wire_frames = renderer.wire_frames();
            let shadow_frames = renderer.shadow_frames();
            let wire_supported = renderer.wireframe_supported();
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
                        .snapshot()
                        .draws
                        .iter()
                        .any(|d| d.metallic_roughness_texture.is_some())
                });
            let normal_required = pbr_required
                && self.ui.normal_maps
                && self.current.as_ref().is_some_and(|asset| {
                    asset
                        .snapshot()
                        .draws
                        .iter()
                        .any(|d| d.normal_texture.is_some())
                });
            let ao_required = pbr_required
                && self.ui.ao_maps
                && self.current.as_ref().is_some_and(|asset| {
                    asset
                        .snapshot()
                        .draws
                        .iter()
                        .any(|d| d.occlusion_texture.is_some())
                });
            let emission_required = pbr_required
                && self.ui.emission
                && self.current.as_ref().is_some_and(|asset| {
                    asset
                        .snapshot()
                        .draws
                        .iter()
                        .any(|d| d.emissive_factor.iter().any(|&v| v > 0.))
                });
            if self.smoke.is_some()
                && self.error.is_none()
                && self.smoke_import
                && (self.current.is_none()
                    || !self.failed_replacement_preserved
                    || (self.smoke_gpu_fault.is_some() && !self.gpu_fault_preserved)
                    || (self.smoke_edit && !self.edit_smoke_done)
                    || (self.smoke_queue && !self.queue_smoke_done)
                    || !matches!(self.smoke_guard, 0 | 3)
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
                    eprintln!("grid_smoke grid_frames={}", grid_frames);
                    eprintln!(
                        "view_smoke channel={:?} wireframe={} wire_supported={wire_supported} wire_frames={wire_frames} axes={} bounds={} helper_lines={}",
                        self.ui.channel,
                        self.ui.wireframe,
                        self.ui.show_axes,
                        self.ui.show_bounds,
                        self.ui.helper_overlay.as_ref().map_or(0, |o| o.lines.len())
                    );
                    eprintln!(
                        "shadow_smoke shadows={} lamp={} shadow_frames={shadow_frames}",
                        self.ui.shadows, self.ui.lighting.enabled
                    );
                    let (azimuth, elevation) = lamp::angles(self.ui.lighting.direction);
                    eprintln!(
                        "lamp_smoke azimuth={azimuth:.1} elevation={elevation:.1} color={:?} icon={} enabled={}",
                        self.ui.lighting.color, self.ui.lamp_icon, self.ui.lighting.enabled
                    );
                    eprintln!(
                        "environment_smoke environment={} sky_frames={sky_frames} panorama_frames={panorama_frames} light={} light_frames={environment_light_frames}",
                        self.ui.environment.label(),
                        self.ui.environment_light
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
        // A new model supersedes any edit still being prepared (decision 5).
        self.edits.cancel();
        self.editing = false;
        self.pending_preview = None;
        self.pending_path = Some(path.clone());
        if self.queue.as_ref().is_some_and(queue::Queue::running) {
            eprintln!("queue_stopped reason=model_replaced");
        }
        self.queue = None;
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
                    "{}Asset Doctor RT-PT 2.0 — {} | {} vertices · {} triangles | Ctrl+O: open · F: fit · H: help",
                    if self.is_dirty() { "* " } else { "" },
                    asset.document().name(),
                    asset.snapshot().positions.len(),
                    asset.snapshot().indices.len() / 3
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
                    .set_scene(asset.snapshot())
                {
                    if std::mem::take(&mut self.gpu_fault_armed) {
                        let actual = self.renderer.as_ref().and_then(Renderer::scene_tag);
                        let Some(expected) = self.expected_tag else {
                            return Err("GPU fault smoke has no published scene".into());
                        };
                        if actual != Some(expected)
                            || self.current.as_ref().is_none_or(|current| {
                                (current.document().id(), current.document().revision()) != expected
                            })
                        {
                            return Err("Injected GPU failure replaced the published scene".into());
                        }
                        self.gpu_fault_preserved = true;
                        eprintln!(
                            "gpu_fault_smoke stage={} preserved=true scene_document={} scene_revision={}",
                            match self.smoke_gpu_fault {
                                Some(SceneFault::Upload) => "upload",
                                _ => "wait",
                            },
                            expected.0.0,
                            expected.1
                        );
                        return Ok(());
                    }
                    if self.smoke.is_some() {
                        return Err(error);
                    }
                    native::show_error(&format!(
                        "Cannot display model: {error}. The previous model is retained."
                    ));
                    self.title();
                    return Ok(());
                }
                self.camera.fit(asset.snapshot().bounds);
                self.ui.tree.replace(asset.document());
                self.ui
                    .set_status(format!("Открыт {}", asset.document().name()), false);
                self.ui.error = None;
                self.confirmed = Some(Confirmed::imported(&asset));
                self.current = Some(asset);
                self.working_path = self.pending_path.take();
                eprintln!("import_published ticket={}", completion.ticket);
                if self.overwrite_smoke == Some(4) {
                    let asset = self.current.as_ref().expect("Published above");
                    let report = asset.diagnostics().as_ref().ok();
                    let mesh = self.edit_mesh.unwrap_or(MeshId(0));
                    eprintln!(
                        "reopened_overwrite revision={} dirty={} mesh_vertices={} analysis={} target_in_mesh={}",
                        asset.document().revision(),
                        self.is_dirty(),
                        asset
                            .document()
                            .primitives()
                            .get(mesh.0)
                            .map_or(0, |m| m.positions.len()),
                        if report.is_some_and(|r| r.is_complete()) {
                            "complete"
                        } else {
                            "incomplete"
                        },
                        report.map_or(usize::MAX, |r| edits::findings_in_mesh(
                            r,
                            mesh,
                            self.smoke_repair
                        ))
                    );
                    self.overwrite_smoke = Some(5);
                }
                if let Some((dir, 5)) = self.save_smoke.as_ref() {
                    let dir = dir.clone();
                    let asset = self.current.as_ref().expect("Published above");
                    let report = asset.diagnostics().as_ref().ok();
                    eprintln!(
                        "reopened_saved revision={} primitives={} mesh_triangles={} analysis={} target_in_mesh={} dirty={}",
                        asset.document().revision(),
                        asset.document().primitives().len(),
                        asset
                            .document()
                            .primitives()
                            .get(self.edit_mesh.unwrap_or(MeshId(0)).0)
                            .map_or(0, |m| m.indices.len() / 3),
                        if report.is_some_and(|r| r.is_complete()) {
                            "complete"
                        } else {
                            "incomplete"
                        },
                        // Findings of the repaired rule on the repaired primitive
                        // only; siblings may legitimately keep their own.
                        report.map_or(usize::MAX, |r| edits::findings_in_mesh(
                            r,
                            self.edit_mesh.unwrap_or(MeshId(0)),
                            self.smoke_repair
                        )),
                        self.is_dirty()
                    );
                    self.save_smoke = Some((dir, 6));
                }
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
                            .is_none_or(|asset| asset.document().id() != expected.0)
                    {
                        return Err("Failed import replaced the previous document/GPU scene".into());
                    }
                    self.failed_replacement_preserved = true;
                    eprintln!("rejected_replacement_preserved document={}", expected.0.0);
                } else if self.smoke_import {
                    return Err(error.into());
                } else if error.code != ErrorCode::Cancelled {
                    self.ui.error = Some(error.to_string().chars().take(512).collect());
                    self.ui
                        .set_status("Загрузка не выполнена; предыдущая модель сохранена", true);
                    self.title();
                    native::show_error(&error.to_string());
                }
            }
        }
        self.title();
        Ok(())
    }
    fn request_edit(&mut self, kind: EditKind) -> bool {
        let Some(asset) = self.current.as_ref() else {
            return false;
        };
        if self.importing || self.editing || self.saving {
            return false;
        }
        let ticket = self.edits.request(EditInput {
            session: asset.session.clone(),
            base: asset.generation.stamp.clone(),
            kind,
        });
        self.editing = true;
        self.edit_started = Instant::now();
        self.edit_busy = match kind {
            EditKind::RemoveDuplicates(_) => "Расчёт и проверка ремонта G002…",
            EditKind::RemoveDegenerate(_) => "Расчёт и проверка ремонта G001…",
            EditKind::RemoveUnused(_) => "Расчёт и проверка ремонта G003…",
            EditKind::MergeVertices(_) => "Расчёт и проверка ремонта G012…",
            EditKind::CreateNormals(_) => "Расчёт и проверка нормалей G004…",
            EditKind::FixNormals(_) => "Расчёт и проверка нормалей G005…",
            EditKind::Place(_) => "Перемещение модели…",
            EditKind::Undo => "Подготовка отмены правки…",
            EditKind::Redo => "Подготовка повтора правки…",
        }
        .into();
        eprintln!("edit_started ticket={ticket} kind={kind:?}");
        true
    }
    fn poll_edit(&mut self) -> GpuResult<()> {
        let Some(completion) = self.edits.poll() else {
            return Ok(());
        };
        if completion.ticket != self.edits.ticket() {
            return Ok(());
        }
        self.editing = false;
        // ADR-0041: the drag preview ends with its edit, applied or refused.
        let placing = std::mem::take(&mut self.model_pending);
        if placing {
            self.model_preview = None;
        }
        let prepare_ms = self.edit_started.elapsed().as_millis();
        let queued = self
            .queue
            .as_ref()
            .is_some_and(|q| q.running() && q.current.is_some());
        if queued {
            match &completion.result {
                Ok(EditOutcome::NoChange(_)) => {
                    self.queue_skip("нечего исправлять");
                    return Ok(());
                }
                Err(error)
                    if matches!(
                        error.code,
                        ad_document::ErrorCode::Unsupported
                            | ad_document::ErrorCode::InvalidGeometry
                    ) =>
                {
                    let reason = error.message.clone();
                    self.queue_skip(&reason);
                    return Ok(());
                }
                Err(error) => {
                    eprintln!("edit_failed code={:?}", error.code);
                    let reason = format!("шаг не подготовлен: {error}");
                    self.stop_queue(&reason);
                    return Ok(());
                }
                Ok(EditOutcome::Candidate(_)) => {}
            }
        }
        let candidate = match completion.result {
            Ok(EditOutcome::NoChange(preview)) => {
                eprintln!(
                    "edit_no_change mesh={} triangles={} vertices={}",
                    preview.mesh.0, preview.before_triangles, preview.before_vertices
                );
                self.ui.set_status(
                    "Исправлять нечего: дефект в примитиве не найден; модель не изменена",
                    false,
                );
                return Ok(());
            }
            Ok(EditOutcome::Candidate(candidate)) => *candidate,
            Err(error) => {
                eprintln!("edit_failed code={:?}", error.code);
                if self.smoke_edit {
                    return Err(error.into());
                }
                self.ui.set_status(
                    if placing {
                        format!("Модель не перемещена, положение прежнее: {error}")
                    } else {
                        format!("Правка не подготовлена, модель прежняя: {error}")
                    },
                    true,
                );
                return Ok(());
            }
        };
        if let Some(info) = candidate.info() {
            eprintln!(
                "edit_preview mesh={} before={} after={} references={} complete={} target_left={} vertices_before={} vertices_after={} appended_bytes={} changed_normals={}",
                info.mesh.0,
                info.before,
                info.after,
                info.references,
                info.complete,
                info.target_left.map_or(-1, |n| n as i64),
                info.vertices_before,
                info.vertices_after,
                info.appended_bytes,
                info.changed_normals
            );
            eprintln!("edit_timing stage=prepare kind=remove prepare_ms={prepare_ms}");
            self.ui
                .set_status("Предпросмотр готов; модель не изменена", false);
            self.pending_preview = Some(candidate);
            if self.smoke_edit || queued {
                return self.apply_preview();
            }
            return Ok(());
        }
        eprintln!(
            "edit_timing stage=prepare kind={} prepare_ms={prepare_ms}",
            edit_name(candidate.kind())
        );
        self.publish_candidate(candidate)
    }
    fn apply_preview(&mut self) -> GpuResult<()> {
        match self.pending_preview.take() {
            Some(candidate) => self.publish_candidate(candidate),
            None => Ok(()),
        }
    }
    fn publish_candidate(&mut self, candidate: EditCandidate) -> GpuResult<()> {
        // Any publication attempt invalidates an older preview.
        self.pending_preview = None;
        let kind = candidate.kind();
        let mesh = candidate.mesh();
        let removed = candidate.preview().map_or(0, |p| {
            p.before_triangles - p.after_triangles + p.before_vertices - p.after_vertices
        });
        let (Some(asset), Some(renderer)) = (self.current.as_mut(), self.renderer.as_mut()) else {
            return Ok(());
        };
        let started = Instant::now();
        let queued = self.queue.as_ref().is_some_and(|q| q.current.is_some());
        let mut queue_failure = None;
        let step = candidate.preview().map(|p| {
            (
                (p.before_triangles, p.after_triangles),
                (p.before_vertices, p.after_vertices),
                p.changed_normals,
            )
        });
        let rule = kind.repair().map(|(_, k)| k);
        let published = publish_edit(asset, renderer, candidate, &Cancellation::default());
        eprintln!(
            "edit_timing stage=publish kind={} publish_ms={}",
            edit_name(kind),
            started.elapsed().as_millis()
        );
        match published {
            Ok(tag) => {
                let report = asset.diagnostics().as_ref().ok();
                let count =
                    |kind| report.map_or(usize::MAX, |r| edits::findings_in_mesh(r, mesh, kind));
                eprintln!(
                    "edit_published kind={} removed={removed} mesh={} revision={} scene_revision={} mesh_triangles={} analysis={} g001_in_mesh={} g002_in_mesh={} undo={} redo={} consistent={} mesh_vertices={} g003_in_mesh={} g012_in_mesh={} g004_in_mesh={} g005_in_mesh={}",
                    edit_name(kind),
                    mesh.0,
                    tag.revision,
                    renderer.scene_tag().map_or(u64::MAX, |t| t.1),
                    asset
                        .document()
                        .primitives()
                        .get(mesh.0)
                        .map_or(0, |m| m.indices.len() / 3),
                    match report {
                        Some(r) if r.is_complete() => "complete",
                        Some(_) => "partial",
                        None => "incomplete",
                    },
                    count(ad_heal::RepairKind::DegenerateTriangles),
                    count(ad_heal::RepairKind::DuplicateTriangles),
                    asset.session.history_usage().undo,
                    asset.session.history_usage().redo,
                    asset.is_consistent(),
                    asset
                        .document()
                        .primitives()
                        .get(mesh.0)
                        .map_or(0, |m| m.positions.len()),
                    count(ad_heal::RepairKind::UnusedVertices),
                    count(ad_heal::RepairKind::DuplicateVertices),
                    count(ad_heal::RepairKind::MissingNormals),
                    count(ad_heal::RepairKind::ZeroNormals)
                );
                self.ui
                    .set_status(format!("Правка применена; ревизия {}", tag.revision), false);
                let primitive = asset.document().primitives().get(mesh.0);
                let (t, v) = (
                    primitive.map_or(0, |m| m.indices.len() / 3),
                    primitive.map_or(0, |m| m.positions.len()),
                );
                let (triangles, vertices, normals) = step.unwrap_or(((t, t), (v, v), 0));
                let record = history::Record {
                    time: history::now(),
                    session: 0,
                    model: asset.document().name().to_owned(),
                    source_bytes: asset.document().source().len() as u64,
                    action: if queued { "queue" } else { edit_name(kind) }.into(),
                    rule: rule.map_or("-", edits::repair_label).into(),
                    mesh: Some(mesh.0),
                    triangles,
                    vertices,
                    normals,
                    left: rule.and_then(|k| report.map(|r| edits::findings_in_mesh(r, mesh, k))),
                    complete: report.is_some_and(|r| r.is_complete()),
                    revision: tag.revision,
                };
                if let Some(q) = self.queue.as_mut().filter(|q| q.current.is_some()) {
                    q.completed();
                }
                self.record_history(record);
            }
            Err(error)
                if !self.smoke_edit
                    && matches!(error, PublishError::Stale | PublishError::Busy) =>
            {
                eprintln!("edit_refused kind={} reason={error:?}", edit_name(kind));
                self.ui.set_status(
                    format!("Правка не применена: {error}. Повторите предпросмотр."),
                    true,
                );
                if queued {
                    queue_failure = Some(format!("шаг не применён: {error}"));
                }
            }
            Err(error) => {
                let revision = asset.document().revision();
                eprintln!(
                    "edit_refused kind={} reason={error:?} revision={revision}",
                    edit_name(kind)
                );
                if queued {
                    queue_failure = Some(format!("шаг не применён: {error}"));
                }
                if std::mem::take(&mut self.edit_fault_expected)
                    && matches!(error, PublishError::Gpu(_))
                {
                    let usage = asset.session.history_usage();
                    let scene = renderer.scene_tag();
                    if scene != Some((asset.document().id(), revision)) || !asset.is_consistent() {
                        return Err("Injected GPU failure changed the published edit state".into());
                    }
                    eprintln!(
                        "edit_gpu_fault preserved=true revision={revision} scene_revision={} undo={} redo={}",
                        scene.map_or(u64::MAX, |t| t.1),
                        usage.undo,
                        usage.redo
                    );
                    // Retry the same edit without the injected failure.
                    self.edit_queue.push_front(kind);
                    return Ok(());
                }
                if self.smoke_edit {
                    return Err(format!("Smoke edit refused: {error}").into());
                }
                self.ui.set_status(
                    format!("Правка не применена, модель прежняя: {error}"),
                    true,
                );
            }
        }
        if let Some(reason) = queue_failure {
            self.stop_queue(&reason);
        }
        self.title();
        Ok(())
    }
    /// Smoke sequence on the first finding of the smoke repair kind:
    /// [fault Apply,] Apply, Undo, Redo.
    fn advance_edit_smoke(&mut self) -> GpuResult<()> {
        if !self.smoke_edit || self.importing || self.editing || !self.failed_replacement_preserved
        {
            return Ok(());
        }
        if self.edit_mesh.is_none() && self.smoke_place {
            let asset = self.current.as_ref().ok_or("Place smoke has no model")?;
            let bounds = asset.snapshot().bounds;
            let mut delta = model_gizmo::Delta::new(model_gizmo::pivot(bounds));
            delta.factor = 1.5;
            delta.offset = [model_gizmo::handle_length(bounds), 0., 0.];
            // A quarter turn about Y keeps the placed bounds exact below.
            delta.turned = std::f32::consts::FRAC_PI_2;
            delta.rotation = glam::Quat::from_rotation_y(delta.turned).to_array();
            let placement = delta.placement(asset.document().placement());
            eprintln!(
                "place_smoke_target scale={} rotation={:?} translation={:?}",
                placement.scale, placement.rotation, placement.translation
            );
            self.edit_mesh = Some(MeshId(0));
            // Then «В центр» + «На пол» on the placed bounds, as the buttons do.
            let mut placed = ad_document::Bounds {
                min: [f32::INFINITY; 3],
                max: [f32::NEG_INFINITY; 3],
            };
            for i in 0..8 {
                let corner = [
                    if i & 1 == 0 {
                        bounds.min[0]
                    } else {
                        bounds.max[0]
                    },
                    if i & 2 == 0 {
                        bounds.min[1]
                    } else {
                        bounds.max[1]
                    },
                    if i & 4 == 0 {
                        bounds.min[2]
                    } else {
                        bounds.max[2]
                    },
                ];
                let p = placement.apply(corner);
                for a in 0..3 {
                    placed.min[a] = placed.min[a].min(p[a]);
                    placed.max[a] = placed.max[a].max(p[a]);
                }
            }
            let grounded = model_gizmo::on_floor(model_gizmo::centred(placement, placed), placed);
            self.edit_queue = VecDeque::from([
                EditKind::Place(placement),
                EditKind::Undo,
                EditKind::Redo,
                EditKind::Place(grounded),
            ]);
        }
        if self.edit_mesh.is_none() {
            let mesh = self
                .current
                .as_ref()
                .and_then(|asset| asset.diagnostics().as_ref().ok())
                .and_then(|report| {
                    report
                        .findings
                        .iter()
                        .find(|f| f.code == edits::target_code(self.smoke_repair))
                })
                .map(|finding| finding.mesh)
                .ok_or("Edit smoke model has no finding for the smoke repair")?;
            self.edit_mesh = Some(mesh);
            self.edit_queue = VecDeque::from([
                EditKind::from_repair(mesh, self.smoke_repair),
                EditKind::Undo,
                EditKind::Redo,
            ]);
        }
        let Some(kind) = self.edit_queue.pop_front() else {
            if self.saving || !self.advance_overwrite_smoke() || !self.advance_save_smoke() {
                return Ok(());
            }
            if self.smoke_place && !self.place_smoke_done {
                self.place_smoke_done = true;
                self.verify_place_export()?;
            }
            if !self.edit_smoke_done {
                self.edit_smoke_done = true;
                eprintln!("edit_smoke_done=true");
            }
            return Ok(());
        };
        if let (Some(fault), Some(renderer)) = (self.edit_fault.take(), self.renderer.as_mut()) {
            renderer.inject_scene_fault(Some(fault));
            self.edit_fault_expected = true;
        }
        self.request_edit(kind);
        Ok(())
    }
    /// Place smoke: the current placement must export, reimport and compare
    /// exactly (the same proof Save Copy uses), without writing a file.
    fn verify_place_export(&mut self) -> GpuResult<()> {
        let asset = self.current.as_ref().ok_or("Place smoke has no model")?;
        let document = asset.document();
        let cancel = Cancellation::default();
        let verified = ad_formats::prepare_glb(
            document,
            document.tag(),
            ad_formats::ExportLimits::default(),
            ad_document::ImportLimits::default(),
            &cancel,
        )?;
        let reimported = ad_formats::import_glb(
            verified.bytes().clone(),
            "place-smoke".into(),
            ad_document::DocumentId(u64::MAX),
            ad_document::ImportLimits::default(),
            &cancel,
        )?;
        ad_formats::compare_reimported(document, &reimported, &cancel)?;
        let placement = document.placement();
        let b = asset.snapshot().bounds;
        eprintln!(
            "place_grounded min_y={:.5} centre_x={:.5} centre_z={:.5}",
            b.min[1],
            (b.min[0] + b.max[0]) * 0.5,
            (b.min[2] + b.max[2]) * 0.5
        );
        eprintln!(
            "place_export_verified revision={} scale={} rotation={:?} translation={:?} bytes={} source_bytes={}",
            document.revision(),
            placement.scale,
            placement.rotation,
            placement.translation,
            verified.bytes().len(),
            document.source().len()
        );
        Ok(())
    }
    /// HEAL-03: appends one record; a failure is a warning, never an edit error.
    fn record_history(&mut self, mut record: history::Record) {
        let Some(history) = self.history.as_ref() else {
            return;
        };
        record.session = history.session;
        match history.append(&record) {
            Ok(()) => eprintln!(
                "history_append action={} rule={} ok=true",
                record.action, record.rule
            ),
            Err(error) => {
                eprintln!("history_append action={} ok=false", record.action);
                self.ui.set_status(error, true);
            }
        }
        if self.ui.history.open {
            self.load_history();
        }
    }
    fn load_history(&mut self) {
        let view = &mut self.ui.history;
        view.lines.clear();
        view.corrupt = 0;
        view.error = None;
        let Some(history) = self.history.as_ref() else {
            view.error = Some("История ремонта отключена в этом запуске".into());
            return;
        };
        view.path = history.path().display().to_string();
        match history.load() {
            Ok((records, corrupt)) => {
                view.corrupt = corrupt;
                view.lines = records
                    .iter()
                    .rev()
                    .map(|r| (r.session == history.session, viewer_ui::history_line(r)))
                    .collect();
            }
            Err(error) => view.error = Some(error),
        }
    }
    /// HEAL-02: starts a queue on the current model (ADR-0038).
    fn start_queue(&mut self) {
        if self.current.is_none() || self.importing || self.saving {
            return;
        }
        if self.queue.as_ref().is_some_and(queue::Queue::running) {
            return;
        }
        // A pending manual preview would be stale once the queue edits.
        self.pending_preview = None;
        self.queue = Some(queue::Queue::default());
        eprintln!("queue_started");
        self.ui.set_status("Очередь ремонтов запущена", false);
    }
    fn stop_queue(&mut self, reason: &str) {
        let Some(q) = self.queue.as_mut().filter(|q| q.running()) else {
            return;
        };
        q.fail(reason);
        if self.editing {
            self.edits.cancel();
            self.editing = false;
        }
        self.pending_preview = None;
        self.finish_queue();
    }
    fn queue_skip(&mut self, reason: &str) {
        if let Some(q) = self.queue.as_mut() {
            if let Some((mesh, kind)) = q.current {
                eprintln!(
                    "queue_skip step={} mesh={} reason={reason}",
                    edits::repair_label(kind),
                    mesh.0
                );
            }
            q.skip(reason);
        }
    }
    /// Chooses and requests the next step when nothing else is running.
    fn advance_queue(&mut self) {
        if self.editing || self.importing || self.saving {
            return;
        }
        let Some(mut q) = self.queue.take() else {
            return;
        };
        if !q.running() || q.current.is_some() {
            self.queue = Some(q);
            return;
        }
        let next = match self.current.as_ref().map(|a| a.diagnostics().as_ref()) {
            Some(Ok(report)) => q.next(report),
            _ => {
                q.fail("анализ модели недоступен");
                queue::Next::Finished
            }
        };
        self.queue = Some(q);
        match next {
            queue::Next::Step(mesh, kind) => {
                eprintln!(
                    "queue_step step={} mesh={}",
                    edits::repair_label(kind),
                    mesh.0
                );
                if !self.request_edit(EditKind::from_repair(mesh, kind)) {
                    self.stop_queue("шаг не запущен");
                }
            }
            queue::Next::Finished => self.finish_queue(),
        }
    }
    fn finish_queue(&mut self) {
        let Some(q) = self.queue.as_ref() else {
            return;
        };
        let remaining = self
            .current
            .as_ref()
            .and_then(|a| a.diagnostics().as_ref().ok())
            .map_or(usize::MAX, queue::Queue::remaining);
        eprintln!(
            "queue_done done={} skipped={} remaining={remaining} stopped={} reason={}",
            q.done.len(),
            q.skipped.len(),
            q.stop.is_some(),
            q.stop.as_deref().unwrap_or("-")
        );
        let summary = format!(
            "Очередь ремонтов: выполнено {}, пропущено {}",
            q.done.len(),
            q.skipped.len()
        );
        match &q.stop {
            Some(reason) => self
                .ui
                .set_status(format!("{summary}; остановлена: {reason}"), true),
            None => self.ui.set_status(summary, false),
        }
        if self.smoke_queue {
            self.queue_smoke_done = true;
        }
    }
    fn is_dirty(&self) -> bool {
        match (self.current.as_ref(), self.confirmed.as_ref()) {
            (Some(asset), Some(confirmed)) => confirmed.is_dirty(asset),
            _ => false,
        }
    }
    /// Smoke-only «Сохранить» sequence (ADR-0035); true when finished or absent.
    fn advance_overwrite_smoke(&mut self) -> bool {
        let Some(stage) = self.overwrite_smoke else {
            return true;
        };
        let Some(path) = self.working_path.clone() else {
            return false;
        };
        match stage {
            0 => {
                // Another program edits the working file: Save must refuse.
                use std::io::Write;
                let appended = std::fs::OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .and_then(|mut f| f.write_all(b"outside edit"));
                eprintln!("overwrite_smoke outside_edit={}", appended.is_ok());
                self.overwrite_smoke = Some(1);
                self.start_overwrite();
            }
            2 => {
                self.overwrite_smoke = Some(3);
                self.start_overwrite();
            }
            5 => return true,
            _ => (),
        }
        false
    }
    /// Smoke-only Save Copy sequence; returns true when finished or absent.
    /// Stages: 0 start refused save, 1 waiting, 2 start new copy, 3 waiting,
    /// 4 reopen saved copy, 5 waiting for import, 6 done.
    fn advance_save_smoke(&mut self) -> bool {
        let Some((dir, stage)) = self.save_smoke.clone() else {
            return true;
        };
        match stage {
            0 => {
                eprintln!(
                    "unsaved_guard dirty={} decision={:?}",
                    self.is_dirty(),
                    guard(self.is_dirty(), None)
                );
                self.save_smoke = Some((dir.clone(), 1));
                self.start_save_to(dir.join("existing.glb"), None);
            }
            2 => {
                self.save_smoke = Some((dir.clone(), 3));
                self.start_save_to(dir.join("healed.glb"), None);
            }
            4 => {
                eprintln!(
                    "unsaved_guard dirty={} decision={:?}",
                    self.is_dirty(),
                    guard(self.is_dirty(), None)
                );
                self.save_smoke = Some((dir.clone(), 5));
                self.start_import(dir.join("healed.glb"));
            }
            6 => return true,
            _ => (),
        }
        false
    }
    /// User-facing Save Copy: asks for a new path unless a smoke target exists.
    fn start_save(&mut self, after: Option<AfterSave>) {
        if self.saving || self.editing || self.importing || self.current.is_none() {
            return;
        }
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let suggested = self.current.as_ref().map_or(String::new(), |asset| {
            let name = asset.document().name();
            let stem = name.strip_suffix(".glb").unwrap_or(name);
            format!("{stem}_copy.glb")
        });
        match native::choose_save_path(window, &suggested) {
            Ok(Some(path)) => self.start_save_to(path, after),
            Ok(None) => {
                eprintln!("save_cancelled dialog=true");
                self.ui
                    .set_status("Сохранение отменено; правки не сохранены", false);
            }
            Err(error) => native::show_error(&error.to_string()),
        }
    }
    fn start_save_to(&mut self, path: PathBuf, after: Option<AfterSave>) {
        let Some(asset) = self.current.as_ref() else {
            return;
        };
        if self.saving || self.editing || self.importing {
            return;
        }
        let ticket = self.saves.request(SaveInput {
            session: asset.session.clone(),
            base: asset.generation.stamp.clone(),
            target: SaveTarget::Copy(path),
        });
        self.saving = true;
        self.after_save = after;
        eprintln!(
            "save_started ticket={ticket} revision={}",
            asset.document().revision()
        );
    }
    /// «Сохранить» (ADR-0035): replaces the working file after verification.
    /// The first overwrite of each working file is confirmed in a native dialog.
    fn start_overwrite(&mut self) {
        if self.saving || self.editing || self.importing {
            return;
        }
        let (Some(asset), Some(path)) = (self.current.as_ref(), self.working_path.clone()) else {
            return;
        };
        if !self.is_dirty() {
            self.ui
                .set_status("Изменений нет; рабочий файл уже совпадает с моделью", false);
            return;
        }
        if self.overwrite_confirmed.as_ref() != Some(&path) {
            let Some(window) = self.window.as_ref() else {
                return;
            };
            if self.overwrite_smoke.is_some() {
                native::answer_next_dialog(saving::UnsavedChoice::SaveCopy);
            }
            eprintln!("save_overwrite_confirm asked=true");
            if !native::ask_overwrite(window, &path.display().to_string()) {
                eprintln!("save_overwrite_declined");
                self.ui
                    .set_status("Сохранение отменено; правки не сохранены", false);
                return;
            }
            self.overwrite_confirmed = Some(path.clone());
        }
        let ticket = self.saves.request(SaveInput {
            session: asset.session.clone(),
            base: asset.generation.stamp.clone(),
            target: SaveTarget::Replace {
                path,
                on_disk: asset.document().source().clone(),
            },
        });
        self.saving = true;
        self.after_save = None;
        eprintln!(
            "save_started ticket={ticket} revision={} replace=true",
            asset.document().revision()
        );
    }
    fn poll_save(&mut self, event_loop: &ActiveEventLoop) -> GpuResult<()> {
        let Some(completion) = self.saves.poll() else {
            return Ok(());
        };
        if completion.ticket != self.saves.ticket() {
            return Ok(());
        }
        self.saving = false;
        let after = self.after_save.take();
        let stage = self.save_smoke.as_ref().map(|(_, stage)| *stage);
        match completion.result {
            Ok(outcome) => {
                let path = outcome.receipt.path().display().to_string();
                let tag = outcome.receipt.tag();
                let accepted = match (self.confirmed.as_mut(), self.current.as_ref()) {
                    (Some(confirmed), Some(asset)) => confirmed.accept(outcome.receipt, asset),
                    _ => false,
                };
                eprintln!(
                    "save_published revision={} bytes={} exact_copy={} reimported_triangles={} cleanup_warning={} accepted={accepted} dirty={} replaced={} backup_kept={}",
                    tag.revision,
                    outcome.bytes,
                    outcome.exact_copy,
                    outcome.reimported_triangles,
                    outcome.cleanup_warning,
                    self.is_dirty(),
                    outcome.replaced,
                    outcome.backup_kept
                );
                let saved = history::Record {
                    time: history::now(),
                    session: 0,
                    model: self
                        .current
                        .as_ref()
                        .map_or(String::new(), |a| a.document().name().to_owned()),
                    source_bytes: outcome.bytes as u64,
                    action: if outcome.replaced {
                        "save"
                    } else {
                        "save-copy"
                    }
                    .into(),
                    rule: "-".into(),
                    mesh: None,
                    triangles: (outcome.reimported_triangles, outcome.reimported_triangles),
                    vertices: (0, 0),
                    normals: 0,
                    left: None,
                    complete: true,
                    revision: tag.revision,
                };
                self.record_history(saved);
                if outcome.replaced && self.overwrite_smoke == Some(3) {
                    self.overwrite_smoke = Some(4);
                }
                if outcome.replaced {
                    // The working file now holds the saved bytes: reopen it as
                    // a fresh revision-0 document (ADR-0035 safe default).
                    self.ui.set_status(
                        if outcome.backup_kept {
                            format!(
                                "Сохранено и проверено: {path}. Резервная копия рядом не удалена"
                            )
                        } else {
                            format!("Сохранено и проверено: {path}")
                        },
                        outcome.backup_kept,
                    );
                    self.start_import(PathBuf::from(&path));
                    self.title();
                    return Ok(());
                }
                if stage == Some(1) {
                    return Err("Smoke Save Copy replaced an existing destination".into());
                }
                if let (Some(3), Some((dir, _))) = (stage, self.save_smoke.clone()) {
                    self.save_smoke = Some((dir, 4));
                }
                if accepted {
                    self.ui
                        .set_status(format!("Копия сохранена и проверена: {path}"), false);
                    if let Some(after) = after {
                        self.perform(after, event_loop);
                    } else if self.smoke.is_none()
                        && self
                            .window
                            .as_ref()
                            .is_some_and(|w| native::ask_rebind(w, &path))
                    {
                        // ADR-0035: the verified copy becomes the working file.
                        eprintln!("save_rebind path_changed=true");
                        self.start_import(PathBuf::from(&path));
                    }
                } else {
                    self.ui.set_status(format!(
                        "Копия {path} сохранена для более ранней ревизии; текущие правки не сохранены"
                    ), true);
                }
            }
            Err(error) => {
                eprintln!(
                    "save_failed code={:?} dirty={} reason={}",
                    error.code,
                    self.is_dirty(),
                    error.message
                );
                if self.overwrite_smoke == Some(1) && error.code == ad_document::ErrorCode::Conflict
                {
                    // The outside edit must survive untouched; then restore the
                    // bytes the program opened and try «Сохранить» again.
                    let path = self.working_path.clone().ok_or("No working path")?;
                    let source = self
                        .current
                        .as_ref()
                        .ok_or("No model")?
                        .document()
                        .source()
                        .clone();
                    let on_disk = std::fs::read(&path)?;
                    let untouched = on_disk.len() == source.len() + 12
                        && on_disk.starts_with(&source)
                        && on_disk.ends_with(b"outside edit");
                    eprintln!("overwrite_conflict refused=true outside_edit_kept={untouched}");
                    if !untouched {
                        return Err("Refused Save changed the working file".into());
                    }
                    std::fs::write(&path, &source)?;
                    self.overwrite_smoke = Some(2);
                } else if let (Some(1), Some((dir, _))) = (stage, self.save_smoke.clone()) {
                    self.save_smoke = Some((dir, 2));
                } else if self.smoke.is_some() {
                    return Err(error.into());
                } else {
                    let what = if error.code == ad_document::ErrorCode::Conflict {
                        "Рабочий файл изменён на диске другой программой и не перезаписан. Сохраните копию"
                    } else {
                        "Файл не сохранён"
                    };
                    self.ui
                        .set_status(format!("{what}; правки остаются несохранёнными"), true);
                    native::show_error(&format!("{what}: {error}"));
                }
            }
        }
        self.title();
        Ok(())
    }
    /// Close/replace guard (decision 2): ask only when the model is dirty.
    fn guarded(&mut self, after: AfterSave, event_loop: &ActiveEventLoop) {
        if self.saving {
            self.ui
                .set_status("Дождитесь окончания сохранения копии", false);
            return;
        }
        let dirty = self.is_dirty();
        let mut decision = guard(dirty, None);
        if decision == Guard::Ask {
            let Some(window) = self.window.as_ref() else {
                return;
            };
            match self.smoke_guard {
                1 => native::answer_next_dialog(saving::UnsavedChoice::Cancel),
                2 => native::answer_next_dialog(saving::UnsavedChoice::Discard),
                _ => (),
            }
            let choice = native::ask_unsaved(window, after.verb());
            if self.smoke_guard == 2 {
                self.smoke_guard = 3;
            }
            decision = guard(dirty, Some(choice));
            eprintln!("unsaved_guard dirty=true choice={choice:?} decision={decision:?}");
        }
        match decision {
            Guard::Proceed => self.perform(after, event_loop),
            Guard::SaveThenProceed => self.start_save(Some(after)),
            Guard::Stay | Guard::Ask => {
                self.ui
                    .set_status("Действие отменено; модель и правки сохранены в окне", false);
                if self.smoke_guard == 1 && after == AfterSave::Close {
                    eprintln!(
                        "unsaved_guard_stayed window_open={} dirty={}",
                        !self.closed,
                        self.is_dirty()
                    );
                    self.smoke_guard = 2;
                    if let Some(window) = self.window.as_ref() {
                        let _ = native::request_close(window);
                    }
                }
            }
        }
    }
    fn perform(&mut self, after: AfterSave, event_loop: &ActiveEventLoop) {
        match after {
            AfterSave::Close => {
                self.close_requested = true;
                self.finish(event_loop);
            }
            AfterSave::Choose => self.choose_model(),
            AfterSave::Open(path) => self.start_import(path),
        }
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

    /// ADR-0040 skybox: dialog, background decode and upload. Failures leave
    /// the preset sky and say why in the status line.
    fn poll_skybox(&mut self) {
        if std::mem::take(&mut self.pending_skybox_choose)
            && let Some(window) = self.window.as_ref()
        {
            match native::choose_image(window) {
                Ok(Some(path)) => {
                    self.skybox_job = Some(skybox::spawn(path));
                    self.ui.set_status("Загрузка скайбокса...", false);
                }
                Ok(None) => (),
                Err(error) => self.ui.set_status(error.to_string(), true),
            }
        }
        if let Some(job) = self.skybox_job.as_ref() {
            match job.try_recv() {
                Ok(Ok(loaded)) => {
                    self.skybox_job = None;
                    self.pending_skybox = Some(loaded);
                }
                Ok(Err(error)) => {
                    self.skybox_job = None;
                    if self.skybox_restore_on.take().is_some() {
                        self.ui.set_status(
                            format!("Скайбокс прошлого запуска не загружен: {error}"),
                            true,
                        );
                    } else {
                        self.ui.set_status(error, true);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => (),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.skybox_job = None;
                    self.ui.set_status("Скайбокс не загружен", true);
                }
            }
        }
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        if let Some(loaded) = self.pending_skybox.take() {
            match renderer.set_skybox_image(Some(&loaded.image)) {
                Ok(()) => {
                    eprintln!(
                        "skybox_loaded name={} size={}x{}",
                        loaded.name, loaded.image.width, loaded.image.height
                    );
                    if loaded.not_panorama {
                        self.ui.set_status(
                            format!(
                                "Скайбокс {}: не 2:1 ({}x{}), картинка будет растянута",
                                loaded.name, loaded.image.width, loaded.image.height
                            ),
                            true,
                        );
                    } else {
                        self.ui
                            .set_status(format!("Скайбокс: {}", loaded.name), false);
                    }
                    self.ui.skybox_name = Some(loaded.name);
                    self.ui.skybox_path = Some(loaded.path);
                    self.ui.skybox_light = Some(loaded.light);
                    self.ui.skybox_on = self.skybox_restore_on.take().unwrap_or(true);
                }
                Err(error) => {
                    self.ui.skybox_name = None;
                    self.ui.skybox_path = None;
                    self.ui.skybox_light = None;
                    self.skybox_restore_on = None;
                    self.ui
                        .set_status(format!("Скайбокс не загружен: {error}"), true);
                }
            }
        }
    }

    fn mods(&self) -> input::Mods {
        input::Mods {
            ctrl: self.modifiers.control_key(),
            shift: self.modifiers.shift_key(),
            alt: self.modifiers.alt_key(),
        }
    }
    /// ADR-0043: the next key or mouse button goes to the slot being
    /// captured. A modifier alone waits; plain Esc cancels; a chord owned by
    /// another action asks the owner first.
    fn capture_chord(&mut self, chord: input::Chord, escape: bool, modifier: bool) {
        let Some((action, slot)) = self.ui.input_capture else {
            return;
        };
        match input::capture(&mut self.ui.bindings, action, slot, chord, escape, modifier) {
            input::Captured::Wait => return,
            input::Captured::Cancelled => (),
            input::Captured::Assigned => self.ui.bindings_changed = true,
            input::Captured::Conflict(owner) => {
                self.ui.input_conflict = Some((action, slot, chord, owner))
            }
        }
        self.ui.input_capture = None;
    }
    fn run_command(&mut self, action: input::Action) {
        use input::Action as A;
        match action {
            A::Open => self.pending_guard = Some(AfterSave::Choose),
            A::Save => self.pending_overwrite = true,
            A::SaveCopy => self.pending_save = true,
            A::Undo => self.request_history(EditKind::Undo),
            A::Redo => self.request_history(EditKind::Redo),
            A::Fit => {
                if let Some(asset) = self.current.as_ref() {
                    self.camera.fit(asset.snapshot().bounds);
                }
            }
            A::ToggleFloor => {
                let on = self.ui.toggle_floor();
                eprintln!("floor_toggle on={on}");
            }
            A::ToggleWireframe => {
                self.ui.wireframe = !self.ui.wireframe && self.ui.wireframe_supported;
            }
            A::ToggleAxes => self.ui.show_axes = !self.ui.show_axes,
            A::ToggleBounds => self.ui.show_bounds = !self.ui.show_bounds,
            A::ToggleLampIcon => self.ui.lamp_icon = !self.ui.lamp_icon,
            A::ToggleModelGizmo => self.ui.model_gizmo_on = !self.ui.model_gizmo_on,
            A::Help => self.ui.help(),
            A::Cancel => {
                self.ui.dismiss_help();
                if self.importing {
                    self.cancel_import();
                }
                self.cancel_edit();
            }
            A::Orbit | A::Pan => (),
        }
    }
    /// Writes the view settings once a change has settled for a second
    /// (`now`: at exit, at once). A failure is a warning and is not retried
    /// until the next change.
    fn save_view_settings(&mut self, now: bool) {
        let Some(path) = self.view_settings_path.clone() else {
            return;
        };
        let current = self.ui.view_settings();
        if current == self.view_saved {
            self.view_pending = None;
            return;
        }
        let due = match &self.view_pending {
            Some((pending, since)) if *pending == current => {
                now || since.elapsed() >= std::time::Duration::from_secs(1)
            }
            _ => {
                self.view_pending = Some((current.clone(), Instant::now()));
                now
            }
        };
        if !due {
            return;
        }
        match viewer_ui::view_settings::save(&path, &current) {
            Ok(()) => eprintln!("view_settings_saved ok=true"),
            Err(error) => {
                eprintln!("view_settings_saved ok=false");
                self.ui
                    .set_status(format!("Настройки вида не сохранены: {error}"), true);
            }
        }
        self.view_saved = current;
        self.view_pending = None;
    }
    /// Writes changed bindings; a failure is a warning, the change stays.
    fn save_bindings(&mut self) {
        if !std::mem::take(&mut self.ui.bindings_changed) {
            return;
        }
        let Some(path) = self.bindings_path.as_ref() else {
            return;
        };
        match input::save(path, &self.ui.bindings) {
            Ok(()) => eprintln!("bindings_saved ok=true"),
            Err(error) => {
                eprintln!("bindings_saved ok=false");
                self.ui
                    .set_status(format!("Управление не сохранено: {error}"), true);
            }
        }
    }
    /// Smoke: a new binding, a refused conflict and a dispatched chord.
    fn advance_input_smoke(&mut self) {
        if !self.smoke_input || self.input_smoke_done || self.current.is_none() {
            return;
        }
        self.input_smoke_done = true;
        let w = input::Chord::key(KeyCode::KeyW, input::Mods::NONE);
        let added = self
            .ui
            .bindings
            .assign(input::Action::ToggleWireframe, 0, w, false)
            .is_ok();
        self.ui.bindings_changed |= added;
        let floor = self.ui.bindings.slots(input::Action::ToggleFloor)[0];
        let refused =
            floor.map(|chord| self.ui.bindings.assign(input::Action::Fit, 0, chord, false));
        eprintln!(
            "input_conflict refused={} owner={}",
            matches!(refused, Some(Err(_))),
            match refused {
                Some(Err(owner)) => owner.action.key(),
                _ => "-",
            }
        );
        // Press the rebound floor key twice: off, then back on.
        for _ in 0..2 {
            if let Some(chord) = floor
                && let Some(action) = self.ui.bindings.command(chord)
            {
                self.run_command(action);
                eprintln!(
                    "input_dispatch chord={} action={} floor_on={}",
                    chord.name(),
                    action.key(),
                    self.ui.floor.is_some()
                );
            }
        }
        eprintln!(
            "input_assigned toggle_wireframe={}",
            self.ui.bindings.describe(input::Action::ToggleWireframe)
        );
    }
    /// ADR-0041: gizmo drags update the preview; release or reset requests
    /// one placement edit.
    fn model_gizmo_actions(&mut self, actions: &viewer_ui::Actions, width: u32, height: u32) {
        let Some(asset) = self.current.as_ref() else {
            return;
        };
        let bounds = asset.snapshot().bounds;
        let current = asset.document().placement();
        if let Some((handle, [nx, ny])) = actions.model_drag
            && !self.model_pending
        {
            let delta = self
                .model_preview
                .get_or_insert_with(|| model_gizmo::Delta::new(model_gizmo::pivot(bounds)));
            let drag = [nx * width as f32, ny * height as f32];
            let camera = &self.camera;
            let px = |p: [f32; 3]| {
                camera
                    .project(p, width, height)
                    .map(|[x, y]| [x * width as f32, y * height as f32])
            };
            match handle {
                model_gizmo::Handle::Rotate(axis) => {
                    self.model_turning = Some(axis);
                    let origin = delta
                        .matrix()
                        .transform_point3(glam::Vec3::from_array(delta.pivot));
                    let origin = origin.to_array();
                    let radius = model_gizmo::ring_radius(bounds) * delta.factor;
                    let base = model_gizmo::knob_angle(axis, origin, camera.eye(width, height));
                    let knob = model_gizmo::knob(axis, origin, radius, base, delta.turned);
                    let tangent = model_gizmo::knob_tangent(axis, origin, knob);
                    // Screen motion per radian, from a small step along the tangent.
                    let step = 1e-3;
                    let ahead = [0, 1, 2].map(|a| knob[a] + tangent[a] * step);
                    if let (Some(a), Some(b)) = (px(knob), px(ahead)) {
                        let tangent_px = [(b[0] - a[0]) / step, (b[1] - a[1]) / step];
                        model_gizmo::drag_turn(delta, axis, drag, tangent_px);
                    }
                }
                model_gizmo::Handle::Axis(axis) => {
                    if let Some(g) =
                        model_gizmo::screen(&self.camera, bounds, None, None, width, height)
                        && let Some(end) = g.ends[axis]
                    {
                        let axis_px = [
                            (end[0] - g.pivot[0]) * width as f32,
                            (end[1] - g.pivot[1]) * height as f32,
                        ];
                        let len = model_gizmo::handle_length(bounds);
                        model_gizmo::drag_axis(delta, axis, len, drag, axis_px);
                    }
                }
                model_gizmo::Handle::Scale => model_gizmo::drag_scale(delta, current, drag[1]),
            }
        }
        if actions.model_drag_end {
            self.model_turning = None;
        }
        if actions.model_drag_end && !self.model_pending {
            match self.model_preview {
                Some(delta) if !delta.is_noop() => {
                    let placement = delta.placement(current);
                    if placement.valid() && self.request_edit(EditKind::Place(placement)) {
                        self.model_pending = true;
                    } else {
                        self.model_preview = None;
                        self.ui.set_status(
                            "Модель сейчас нельзя переместить: идёт другая работа",
                            true,
                        );
                    }
                }
                _ => self.model_preview = None,
            }
        }
        // Numeric fields: preview while editing, one edit on release.
        if let Some(target) = actions.place_preview
            && !self.model_pending
        {
            self.model_preview = Some(model_gizmo::between(current, target));
        }
        if let Some(target) = actions.place
            && !self.model_pending
        {
            if target == current || !target.valid() {
                self.model_preview = None;
            } else if self.request_edit(EditKind::Place(target)) {
                self.model_pending = true;
                self.model_preview = Some(model_gizmo::between(current, target));
            } else {
                self.model_preview = None;
                self.ui
                    .set_status("Положение не изменено: идёт другая работа", true);
            }
        }
        if actions.model_reset
            && !current.is_identity()
            && !self.model_pending
            && !self.request_edit(EditKind::Place(ad_document::Placement::IDENTITY))
        {
            self.ui
                .set_status("Положение не сброшено: идёт другая работа", true);
        }
    }

    fn cancel_import(&mut self) {
        self.imports.cancel();
        self.importing = false;
        self.ui.set_status("Загрузка отменена", false);
        self.title();
    }

    fn redraw(&mut self) -> GpuResult<()> {
        let Some(window) = self.window.clone() else {
            return Ok(());
        };
        if self.renderer.as_ref().is_none_or(|r| !r.drawable()) {
            return Ok(());
        }
        self.refresh_edit_ui();
        if self.smoke_view_change && self.current.is_some() {
            self.smoke_view_change = false;
            self.ui.show_axes = !self.ui.show_axes;
            eprintln!("view_settings_changed axes={}", self.ui.show_axes);
        }
        self.save_view_settings(false);
        // ADR-0040 lamp gizmo: lamp 1.25 model radii from the centre.
        let (view_w, view_h) = self.last_view;
        // ADR-0041 model gizmo at the (previewed) bottom centre.
        self.ui.model_gizmo = self.current.as_ref().and_then(|asset| {
            model_gizmo::screen(
                &self.camera,
                asset.snapshot().bounds,
                self.model_preview.as_ref(),
                self.model_turning,
                view_w,
                view_h,
            )
        });
        self.ui.helper_overlay = self.current.as_ref().and_then(|asset| {
            (self.ui.show_axes || self.ui.show_bounds).then(|| {
                helpers::build(
                    &self.camera,
                    asset.snapshot().bounds,
                    self.ui.show_axes,
                    self.ui.show_bounds,
                    view_w,
                    view_h,
                )
            })
        });
        self.ui.placement = self
            .current
            .as_ref()
            .map_or(ad_document::Placement::IDENTITY, |a| {
                a.document().placement()
            });
        self.ui.lamp_gizmo = self.current.as_ref().and_then(|_| {
            let camera = &self.camera;
            let guides = lamp::guides(self.ui.lighting.direction, |d| {
                camera.project_direction(d, LAMP_DISTANCE, view_w, view_h)
            });
            Some(viewer_ui::LampGizmo {
                lamp: camera.project_direction(
                    self.ui.lighting.direction,
                    LAMP_DISTANCE,
                    view_w,
                    view_h,
                )?,
                center: camera.project_direction([0., 1., 0.], 0., view_w, view_h)?,
                guides: Some(std::sync::Arc::new(guides)),
            })
        });
        let (mut frame, actions) = self
            .ui
            .frame(&window, self.current.as_ref(), self.importing);
        if actions.cancel {
            self.cancel_import();
        }
        if actions.fit
            && let Some(asset) = &self.current
        {
            self.camera.fit(asset.snapshot().bounds);
        }
        {
            let (width, height) = frame.scene.map_or((1, 1), |r| (r.width, r.height));
            self.model_gizmo_actions(&actions, width, height);
        }
        let renderer = self.renderer.as_mut().expect("Drawable renderer checked");
        let (width, height) = frame.scene.map_or((1, 1), |r| (r.width, r.height));
        self.last_view = (width, height);
        if let Some([dx, dy]) = actions.lamp_drag {
            let camera = &self.camera;
            self.ui.lighting.direction = lamp::drag(self.ui.lighting.direction, dx, dy, |d| {
                camera.project_direction(d, LAMP_DISTANCE, width, height)
            });
        }
        if let Some([turn_azimuth, turn_elevation]) = actions.lamp_turn {
            let (azimuth, elevation) = lamp::angles(self.ui.lighting.direction);
            self.ui.lighting.direction =
                lamp::from_angles(azimuth + turn_azimuth, elevation + turn_elevation);
        }
        if actions.lamp_from_camera {
            self.ui.lighting.direction = self.camera.view_direction();
        }
        let floor_grid = self.ui.floor.and_then(|theme| {
            self.current.as_ref().and_then(|asset| {
                // ADR-0041: the floor stays at the imported model's bottom,
                // so a moved or turned model is seen moving against it.
                let bounds = asset.snapshot().source_bounds;
                ad_render_vulkan::grid::grid_for_bounds(bounds.min, bounds.max)
                    .map(|grid| ad_render_vulkan::grid::GridSettings { theme, ..grid })
            })
        });
        renderer.set_grid(floor_grid);
        // ADR-0044: lamp shadow fitted to the model and its shadow on the floor.
        renderer.set_shadow(
            self.current
                .as_ref()
                .filter(|_| self.ui.shadows)
                .and_then(|asset| {
                    lamp::shadow_matrix(
                        self.ui.lighting.direction,
                        asset.snapshot().bounds,
                        floor_grid.map(|g| g.floor_y),
                        ad_render_vulkan::shadow::SIZE,
                    )
                })
                .map(|(matrix, texel)| ad_render_vulkan::shadow::ShadowSettings {
                    light_view_projection: matrix.to_cols_array_2d(),
                    strength: self.ui.shadow_strength,
                    bias: 0.0015,
                    normal_offset: texel * 1.5,
                }),
        );
        let view_projection = self.camera.matrix(width, height);
        // ADR-0040 environment: sky only, lamps/exposure stay as they are.
        renderer.set_sky(Some(ad_render_vulkan::sky::SkySettings {
            inverse_view_projection: glam::Mat4::from_cols_array_2d(&view_projection)
                .inverse()
                .to_cols_array_2d(),
            environment: self.ui.environment,
            panorama: self.ui.skybox_on.then(|| self.ui.skybox_yaw.to_radians()),
        }));
        let panorama_light = self.ui.skybox_light.filter(|_| self.ui.skybox_on);
        renderer.set_environment_light(self.ui.environment_light.then(|| {
            ad_render_vulkan::sky::EnvironmentLight {
                colors: panorama_light.unwrap_or_else(|| self.ui.environment.light_colors()),
                strength: self.ui.environment_strength,
            }
        }));
        renderer.set_camera(view_projection, self.camera.eye(width, height))?;
        let lamp_direction = self.ui.lighting.direction;
        renderer.set_model_preview(self.model_preview.map(|delta| {
            let m = delta.matrix();
            let inverse = m.inverse();
            (
                (glam::Mat4::from_cols_array_2d(&view_projection) * m).to_cols_array_2d(),
                inverse
                    .transform_point3(glam::Vec3::from_array(self.camera.eye(width, height)))
                    .to_array(),
                // The lamp in the previewed model's frame: it does not turn.
                inverse
                    .transform_vector3(glam::Vec3::from_array(lamp_direction))
                    .normalize_or_zero()
                    .to_array(),
            )
        }));
        renderer.set_channel(self.ui.channel);
        renderer.set_wireframe(self.ui.wireframe);
        self.ui.wireframe_supported = renderer.wireframe_supported();
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
        if actions.skybox_load {
            self.pending_skybox_choose = true;
        }
        if actions.skybox_remove {
            self.ui.skybox_name = None;
            self.ui.skybox_path = None;
            self.ui.skybox_light = None;
            self.ui.skybox_on = false;
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.set_skybox_image(None)?;
            }
        }
        // Open native dialogs only after the acquired image has been submitted.
        if actions.open {
            self.pending_guard = Some(AfterSave::Choose);
        }
        if actions.save {
            self.pending_overwrite = true;
        }
        if actions.history_open {
            self.ui.history.open = true;
            self.load_history();
        }
        if actions.history_clear {
            if let Some(history) = self.history.as_ref()
                && let Err(error) = history.clear()
            {
                self.ui.set_status(error, true);
            }
            self.load_history();
        }
        if actions.queue_start {
            self.pending_queue_start = true;
        }
        if actions.queue_stop {
            self.pending_queue_stop = true;
        }
        if actions.save_copy {
            self.pending_save = true;
        }
        if let Some((mesh, kind)) = actions.preview {
            self.request_edit(EditKind::from_repair(mesh, kind));
        }
        if actions.discard_preview && self.pending_preview.take().is_some() {
            self.ui
                .set_status("Предпросмотр закрыт; модель не изменена", false);
        }
        if actions.apply {
            self.apply_preview()?;
        }
        if actions.undo {
            self.request_history(EditKind::Undo);
        }
        if actions.redo {
            self.request_history(EditKind::Redo);
        }
        if actions.cancel_edit {
            self.cancel_edit();
        }
        Ok(())
    }
    fn request_history(&mut self, kind: EditKind) {
        let allowed = self.current.as_ref().is_some_and(|asset| match kind {
            EditKind::Undo => asset.session.can_undo(),
            _ => asset.session.can_redo(),
        });
        if allowed {
            self.request_edit(kind);
        }
    }
    fn cancel_edit(&mut self) {
        if self.saving {
            self.saves.interrupt();
            self.ui.set_status("Отмена сохранения…", false);
        }
        if self.editing {
            self.edits.cancel();
            self.editing = false;
            self.ui
                .set_status("Подготовка правки отменена; модель не изменена", false);
        }
    }
    fn refresh_edit_ui(&mut self) {
        let history = self
            .current
            .as_ref()
            .map(|asset| (asset.session.can_undo(), asset.session.can_redo()));
        self.ui.edit = viewer_ui::EditUi {
            busy: self.editing.then(|| self.edit_busy.clone()),
            preview: self.pending_preview.as_ref().and_then(EditCandidate::info),
            can_undo: history.is_some_and(|h| h.0),
            can_redo: history.is_some_and(|h| h.1),
            can_save: self.current.is_some(),
            can_overwrite: self.current.is_some() && self.working_path.is_some(),
            queue: self.queue.as_ref().map(|q| viewer_ui::QueueUi {
                running: q.running(),
                done: q
                    .done
                    .iter()
                    .map(|&(m, k)| queue::step_name(m, k))
                    .collect(),
                skipped: q
                    .skipped
                    .iter()
                    .map(|(m, k, r)| format!("{} — {r}", queue::step_name(*m, *k)))
                    .collect(),
                current: q.current.map(|(m, k)| queue::step_name(m, k)),
                stop: q.stop.clone(),
            }),
            dirty: self.is_dirty(),
        };
        if self.saving {
            self.ui.edit.busy = Some("Сохранение и проверка копии…".into());
        }
    }

    fn tick(&mut self, event_loop: &ActiveEventLoop) -> GpuResult<()> {
        if self.closed {
            return Ok(());
        }
        self.poll_import()?;
        self.poll_edit()?;
        self.poll_save(event_loop)?;
        if std::mem::take(&mut self.pending_overwrite) {
            self.start_overwrite();
        }
        self.poll_skybox();
        self.advance_input_smoke();
        self.save_bindings();
        if std::mem::take(&mut self.pending_save) {
            self.start_save(None);
        }
        if let Some(after) = self.pending_guard.take() {
            self.guarded(after, event_loop);
            if self.closed {
                return Ok(());
            }
        }
        self.advance_edit_smoke()?;
        if self.smoke_queue
            && self.queue.is_none()
            && self.failed_replacement_preserved
            && self.current.is_some()
            && !self.importing
        {
            self.pending_queue_start = true;
        }
        if std::mem::take(&mut self.pending_queue_stop) {
            self.stop_queue("остановлено пользователем");
        }
        if std::mem::take(&mut self.pending_queue_start) {
            self.start_queue();
        }
        self.advance_queue();
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
        if self.failed_replacement_preserved
            && !self.importing
            && let Some(fault) = self.smoke_gpu_fault
            && let Some(path) = self.smoke_good.take()
            && let Some(renderer) = self.renderer.as_mut()
        {
            self.expected_tag = renderer.scene_tag();
            renderer.inject_scene_fault(Some(fault));
            self.gpu_fault_armed = true;
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
            && (self.current.is_none()
                || self.importing
                || !self.failed_replacement_preserved
                || (self.smoke_gpu_fault.is_some() && !self.gpu_fault_preserved)
                || (self.smoke_edit && !self.edit_smoke_done)
                || (self.smoke_queue && !self.queue_smoke_done))
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
                self.mouse_drag = None;
                self.cursor = None;
                self.modifiers = ModifiersState::default();
                Ok(())
            }
            WindowEvent::DroppedFile(path) => {
                self.pending_guard = Some(AfterSave::Open(path));
                Ok(())
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
                Ok(())
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                // ADR-0043: every key goes through the owner's bindings.
                if let PhysicalKey::Code(code) = event.physical_key {
                    let chord = input::Chord::key(code, self.mods());
                    if self.ui.input_capture.is_some() {
                        self.capture_chord(
                            chord,
                            code == KeyCode::Escape,
                            input::is_modifier(code),
                        );
                    } else if let Some(action) = self.ui.bindings.command(chord)
                        && (!self.ui.context.text_edit_focused()
                            || chord.mods.ctrl
                            || chord.mods.alt
                            || action == input::Action::Cancel)
                    {
                        self.run_command(action);
                    }
                }
                Ok(())
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let chord = input::Chord {
                    input: input::Input::Mouse(button),
                    mods: self.mods(),
                };
                let inside = self.cursor.is_some_and(|(x, y)| self.ui.camera_at(x, y));
                if state == ElementState::Pressed && self.ui.input_capture.is_some() {
                    self.capture_chord(chord, false, false);
                } else if state == ElementState::Pressed && !consumed {
                    if let Some(action) = self.ui.bindings.command(chord) {
                        self.run_command(action);
                    } else if inside && let Some(drag) = self.ui.bindings.drag(button, chord.mods) {
                        self.mouse = Some(button);
                        self.mouse_drag = Some(drag);
                    }
                } else if state == ElementState::Released && self.mouse == Some(button) {
                    self.mouse = None;
                    self.mouse_drag = None;
                }
                Ok(())
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some((x, y)) = self.cursor {
                    let dx = (position.x - x) as f32;
                    let dy = (position.y - y) as f32;
                    match self.mouse_drag {
                        Some(input::Action::Orbit) => self.camera.orbit(dx, dy),
                        Some(input::Action::Pan) => {
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
                    self.camera.zoom(if self.ui.bindings.invert_zoom {
                        -steps
                    } else {
                        steps
                    });
                }
                Ok(())
            }
            WindowEvent::CloseRequested => {
                eprintln!("native_close_requested");
                if self.smoke.is_some() && self.smoke_guard == 0 {
                    self.close_requested = true;
                    self.finish(event_loop);
                } else {
                    self.pending_guard = Some(AfterSave::Close);
                }
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

/// ADR-0040: the lamp gizmo sits this many model radii from the centre.
const LAMP_DISTANCE: f32 = 1.25;

fn edit_name(kind: EditKind) -> &'static str {
    match kind {
        EditKind::RemoveDuplicates(_)
        | EditKind::RemoveDegenerate(_)
        | EditKind::RemoveUnused(_)
        | EditKind::MergeVertices(_)
        | EditKind::CreateNormals(_)
        | EditKind::FixNormals(_) => "remove",
        EditKind::Place(_) => "place",
        EditKind::Undo => "undo",
        EditKind::Redo => "redo",
    }
}

fn run() -> GpuResult<()> {
    let mut validation = false;
    let mut smoke = false;
    let mut initial_path = None;
    let mut smoke_import = false;
    let mut smoke_bad = None;
    let mut smoke_gpu_fault = None;
    let mut smoke_edit = false;
    let mut smoke_save = None;
    let mut smoke_guard = 0u8;
    let mut smoke_overwrite = false;
    let mut smoke_queue = false;
    let mut smoke_place = false;
    let mut smoke_input = false;
    let mut bindings_arg: Option<PathBuf> = None;
    let mut pending_skybox = None;
    let mut history_path = None;
    let mut smoke_repair = ad_heal::RepairKind::DuplicateTriangles;
    let mut ui = viewer_ui::ViewerUi::default();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // View settings first, so command-line options still win. Smoke runs
    // never read or write the owner's file (only --view-settings).
    let smoke_run = argv.iter().any(|a| a.starts_with("--smoke"));
    let view_settings_path = argv
        .iter()
        .position(|a| a == "--view-settings")
        .map(|i| {
            argv.get(i + 1)
                .map(PathBuf::from)
                .ok_or("--view-settings needs a file path")
        })
        .transpose()?
        .or_else(|| {
            if smoke_run {
                None
            } else {
                viewer_ui::view_settings::default_path()
            }
        });
    let mut view_warnings = Vec::new();
    let mut restore_skybox = None;
    if let Some(path) = view_settings_path.as_ref() {
        let (value, warnings) = viewer_ui::view_settings::load(path);
        view_warnings.extend(warnings);
        if let Some(value) = value {
            restore_skybox = ui.apply_view_settings(&value, &mut view_warnings);
        }
        eprintln!(
            "view_settings_loaded warnings={} floor={:?} environment={} wireframe={} tab={:?} skybox={}",
            view_warnings.len(),
            ui.floor.map(|f| f.label()),
            ui.environment.label(),
            ui.wireframe,
            ui.tab,
            restore_skybox.is_some()
        );
    }
    let mut smoke_view_change = false;
    let mut args = argv.into_iter();
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
            "--smoke-edit" => smoke_edit = true,
            "--smoke-place" => {
                smoke_edit = true;
                smoke_place = true;
            }
            "--smoke-guard" => smoke_guard = 1,
            "--smoke-overwrite" => smoke_overwrite = true,
            "--smoke-queue" => smoke_queue = true,
            "--environment-light" => {
                ui.environment_light = true;
                ui.environment_strength = args
                    .next()
                    .and_then(|n| n.parse::<f32>().ok())
                    .filter(|s| {
                        ad_render_vulkan::sky::EnvironmentLight::preset(ui.environment, *s).valid()
                    })
                    .ok_or("--environment-light needs a strength from 0 to 2")?;
            }
            "--lamp-off" => ui.lighting.enabled = false,
            "--bindings" => {
                bindings_arg = Some(PathBuf::from(
                    args.next().ok_or("--bindings needs a file path")?,
                ))
            }
            "--smoke-input" => smoke_input = true,
            "--view-settings" => {
                args.next();
            }
            "--smoke-view-change" => smoke_view_change = true,
            "--channel" => {
                ui.channel = args
                    .next()
                    .and_then(|n| n.parse::<usize>().ok())
                    .and_then(|i| ad_render_vulkan::lighting::Channel::ALL.get(i).copied())
                    .ok_or("--channel needs a channel number from 0")?;
            }
            "--wireframe" => ui.wireframe = true,
            "--no-shadows" => ui.shadows = false,
            "--helpers" => {
                ui.show_axes = true;
                ui.show_bounds = true;
            }
            "--light-preset" => {
                let preset = args
                    .next()
                    .and_then(|n| n.parse::<usize>().ok())
                    .and_then(|i| lamp::LightPreset::ALL.get(i).copied())
                    .ok_or("--light-preset needs a preset number from 0")?;
                preset.apply(&mut ui.lighting);
            }
            "--lamp" => {
                // AZIMUTH,ELEVATION[,R,G,B] in degrees and 0..1 colour.
                let value = args
                    .next()
                    .ok_or("--lamp needs AZIMUTH,ELEVATION[,R,G,B]")?;
                let parts: Vec<f32> = value
                    .split(',')
                    .map(|p| p.trim().parse::<f32>())
                    .collect::<Result<_, _>>()
                    .map_err(|_| "--lamp needs numbers")?;
                let (Some(&azimuth), Some(&elevation)) = (parts.first(), parts.get(1)) else {
                    return Err("--lamp needs AZIMUTH,ELEVATION[,R,G,B]".into());
                };
                ui.lighting.direction = lamp::from_angles(azimuth, elevation);
                if let [_, _, r, g, b] = parts[..] {
                    ui.lighting.color = [r, g, b];
                }
                if !ui.lighting.valid() {
                    return Err("--lamp values out of range".into());
                }
            }
            "--skybox" => {
                let path = args
                    .next()
                    .ok_or("--skybox needs a PNG/JPEG panorama path")?;
                pending_skybox = Some(skybox::load(std::path::Path::new(&path))?);
            }
            "--environment" => {
                ui.environment = args
                    .next()
                    .and_then(|n| n.parse::<usize>().ok())
                    .and_then(|i| ad_render_vulkan::sky::Environment::ALL.get(i).copied())
                    .ok_or("--environment needs a preset number from 0")?;
            }
            "--floor-theme" => {
                let value = args
                    .next()
                    .ok_or("--floor-theme needs a theme number or none")?;
                ui.floor = match value.as_str() {
                    "none" => None,
                    n => Some(
                        n.parse::<usize>()
                            .ok()
                            .and_then(|i| ad_render_vulkan::grid::FloorTheme::ALL.get(i).copied())
                            .ok_or("--floor-theme needs a theme number from 0 or none")?,
                    ),
                };
            }
            "--history" => {
                history_path = Some(PathBuf::from(
                    args.next().ok_or("--history needs a file path")?,
                ))
            }
            "--smoke-repair" => {
                smoke_repair = match args.next().as_deref() {
                    Some("g001") => ad_heal::RepairKind::DegenerateTriangles,
                    Some("g002") => ad_heal::RepairKind::DuplicateTriangles,
                    Some("g003") => ad_heal::RepairKind::UnusedVertices,
                    Some("g012") => ad_heal::RepairKind::DuplicateVertices,
                    Some("g004") => ad_heal::RepairKind::MissingNormals,
                    Some("g005") => ad_heal::RepairKind::ZeroNormals,
                    _ => {
                        return Err(
                            "--smoke-repair needs g001, g002, g003, g004, g005 or g012".into()
                        );
                    }
                }
            }
            "--smoke-save" => {
                smoke_save = Some(PathBuf::from(
                    args.next()
                        .ok_or("--smoke-save needs an owned output directory")?,
                ))
            }
            "--smoke-gpu-fault" => {
                smoke_gpu_fault = Some(
                    args.next()
                        .ok_or("--smoke-gpu-fault needs upload or wait")?
                        .parse::<SceneFault>()?,
                )
            }
            "--help" => {
                println!(
                    "AssetDoctorRTPT [--require-validation] [--pbr] [--no-mr-maps] [--no-normal-maps] [--no-ao-maps] [--no-emission] [--open model.glb] [--smoke] [--smoke-import model.glb --smoke-bad bad.glb [--smoke-edit [--smoke-save dir]] [--smoke-gpu-fault upload|wait]]"
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    if smoke_guard > 0 && (!smoke_edit || smoke_save.is_some()) {
        return Err("--smoke-guard requires --smoke-edit without --smoke-save".into());
    }
    if smoke_save.is_some() && !smoke_edit {
        return Err("--smoke-save requires --smoke-edit".into());
    }
    if (smoke_gpu_fault.is_some() || smoke_edit) && (!smoke_import || smoke_bad.is_none()) {
        return Err("--smoke-gpu-fault/--smoke-edit require --smoke-import and --smoke-bad".into());
    }
    // With --smoke-edit the injected GPU failure targets the first edit, not a re-import.
    let (smoke_gpu_fault, edit_fault) = if smoke_edit {
        (None, smoke_gpu_fault)
    } else {
        (smoke_gpu_fault, None)
    };
    let smoke_good = smoke_gpu_fault.and(initial_path.clone());
    let event_loop = EventLoop::new()?;
    // ADR-0043: smoke runs never read or write the owner's bindings file.
    let bindings_path = bindings_arg.or_else(|| if smoke { None } else { input::default_path() });
    if let Some(path) = bindings_path.as_ref() {
        let (bindings, warnings) = input::load(path);
        eprintln!(
            "input_loaded warnings={} toggle_floor={} orbit={}",
            warnings.len(),
            bindings.describe(input::Action::ToggleFloor),
            bindings.describe(input::Action::Orbit)
        );
        if !warnings.is_empty() {
            ui.set_status(format!("Управление: {}", warnings.join("; ")), true);
        }
        ui.bindings = bindings;
    }
    if !view_warnings.is_empty() {
        ui.set_status(
            format!("Настройки вида: {}", view_warnings.join("; ")),
            true,
        );
    }
    let view_saved = ui.view_settings();
    let (skybox_job, skybox_restore_on) = match restore_skybox {
        Some((path, on)) if pending_skybox.is_none() => (Some(skybox::spawn(path)), Some(on)),
        _ => (None, None),
    };
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
        imports: new_imports(),
        importing: false,
        current: None,
        camera: Camera::default(),
        mouse: None,
        mouse_drag: None,
        bindings_path,
        view_settings_path,
        view_saved,
        view_pending: None,
        skybox_restore_on,
        smoke_view_change,
        smoke_input,
        input_smoke_done: false,
        cursor: None,
        modifiers: ModifiersState::default(),
        initial_path,
        smoke_import,
        smoke_bad,
        expected_tag: None,
        failed_replacement_preserved: false,
        smoke_gpu_fault,
        smoke_good,
        gpu_fault_armed: false,
        gpu_fault_preserved: false,
        edits: new_edits(),
        editing: false,
        edit_busy: String::new(),
        edit_started: Instant::now(),
        pending_guard: None,
        pending_save: false,
        pending_overwrite: false,
        pending_skybox_choose: false,
        last_view: (1, 1),
        model_preview: None,
        model_pending: false,
        model_turning: None,
        smoke_place,
        place_smoke_done: false,
        skybox_job,
        pending_skybox,
        working_path: None,
        pending_path: None,
        overwrite_confirmed: None,
        queue: None,
        pending_queue_start: false,
        pending_queue_stop: false,
        smoke_queue,
        queue_smoke_done: false,
        history: history_path
            .or_else(|| {
                if smoke {
                    None
                } else {
                    history::History::default_path()
                }
            })
            .map(|path| {
                let session = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos() as u64);
                history::History::at(path, session)
            }),
        pending_preview: None,
        smoke_edit,
        edit_fault,
        edit_queue: VecDeque::new(),
        edit_mesh: None,
        edit_fault_expected: false,
        edit_smoke_done: false,
        saves: new_saves(),
        saving: false,
        confirmed: None,
        after_save: None,
        save_smoke: smoke_save.map(|dir| (dir, 0)),
        overwrite_smoke: smoke_overwrite.then_some(0),
        smoke_guard,
        smoke_repair,
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
