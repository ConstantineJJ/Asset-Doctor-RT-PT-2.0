#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod camera;
mod diagnostic_ui;
mod edits;
mod imports;
mod native;
mod saving;
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
    /// File the current model was opened from (or last saved and reopened).
    working_path: Option<PathBuf>,
    /// Path of the import in flight; becomes the working path when published.
    pending_path: Option<PathBuf>,
    /// Working file whose overwrite the user already confirmed once.
    overwrite_confirmed: Option<PathBuf>,
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
        let prepare_ms = self.edit_started.elapsed().as_millis();
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
                    format!("Правка не подготовлена, модель прежняя: {error}"),
                    true,
                );
                return Ok(());
            }
        };
        if let Some(info) = candidate.info() {
            eprintln!(
                "edit_preview mesh={} before={} after={} references={} complete={} target_left={} vertices_before={} vertices_after={} appended_bytes={}",
                info.mesh.0,
                info.before,
                info.after,
                info.references,
                info.complete,
                info.target_left.map_or(-1, |n| n as i64),
                info.vertices_before,
                info.vertices_after,
                info.appended_bytes
            );
            eprintln!("edit_timing stage=prepare kind=remove prepare_ms={prepare_ms}");
            self.ui
                .set_status("Предпросмотр готов; модель не изменена", false);
            self.pending_preview = Some(candidate);
            if self.smoke_edit {
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
                    "edit_published kind={} removed={removed} mesh={} revision={} scene_revision={} mesh_triangles={} analysis={} g001_in_mesh={} g002_in_mesh={} undo={} redo={} consistent={} mesh_vertices={} g003_in_mesh={} g012_in_mesh={}",
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
                    count(ad_heal::RepairKind::DuplicateVertices)
                );
                self.ui
                    .set_status(format!("Правка применена; ревизия {}", tag.revision), false);
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
            }
            Err(error) => {
                let revision = asset.document().revision();
                eprintln!(
                    "edit_refused kind={} reason={error:?} revision={revision}",
                    edit_name(kind)
                );
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
            self.pending_guard = Some(AfterSave::Choose);
        }
        if actions.save {
            self.pending_overwrite = true;
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
                || (self.smoke_edit && !self.edit_smoke_done))
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
                self.pending_guard = Some(AfterSave::Open(path));
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
                        self.pending_guard = Some(AfterSave::Choose)
                    }
                    PhysicalKey::Code(KeyCode::KeyS)
                        if self.modifiers.control_key() && self.modifiers.shift_key() =>
                    {
                        self.pending_save = true
                    }
                    PhysicalKey::Code(KeyCode::KeyS) if self.modifiers.control_key() => {
                        self.pending_overwrite = true
                    }
                    PhysicalKey::Code(KeyCode::KeyF) => {
                        if let Some(asset) = self.current.as_ref() {
                            self.camera.fit(asset.snapshot().bounds);
                        }
                    }
                    PhysicalKey::Code(KeyCode::Escape) => {
                        self.ui.dismiss_help();
                        if self.importing {
                            self.cancel_import();
                        }
                        self.cancel_edit();
                    }
                    PhysicalKey::Code(KeyCode::KeyZ)
                        if self.modifiers.control_key() && self.modifiers.shift_key() =>
                    {
                        self.request_history(EditKind::Redo)
                    }
                    PhysicalKey::Code(KeyCode::KeyZ) if self.modifiers.control_key() => {
                        self.request_history(EditKind::Undo)
                    }
                    PhysicalKey::Code(KeyCode::KeyY) if self.modifiers.control_key() => {
                        self.request_history(EditKind::Redo)
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

fn edit_name(kind: EditKind) -> &'static str {
    match kind {
        EditKind::RemoveDuplicates(_)
        | EditKind::RemoveDegenerate(_)
        | EditKind::RemoveUnused(_)
        | EditKind::MergeVertices(_) => "remove",
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
    let mut smoke_repair = ad_heal::RepairKind::DuplicateTriangles;
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
            "--smoke-edit" => smoke_edit = true,
            "--smoke-guard" => smoke_guard = 1,
            "--smoke-overwrite" => smoke_overwrite = true,
            "--smoke-repair" => {
                smoke_repair = match args.next().as_deref() {
                    Some("g001") => ad_heal::RepairKind::DegenerateTriangles,
                    Some("g002") => ad_heal::RepairKind::DuplicateTriangles,
                    Some("g003") => ad_heal::RepairKind::UnusedVertices,
                    Some("g012") => ad_heal::RepairKind::DuplicateVertices,
                    _ => return Err("--smoke-repair needs g001, g002, g003 or g012".into()),
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
        working_path: None,
        pending_path: None,
        overwrite_confirmed: None,
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
