//! Background edit preparation and atomic desktop publication (ADR-0031).
//!
//! Preparation (worker, read-only Session clone): complete plan, verified
//! command, candidate snapshot and analysis at the future revision.
//! Publication (UI thread): stale/busy checks, Session stage, GPU stage, then
//! infallible publish of history, scene and one generation.
use crate::imports::{Generation, PreparedAsset};
use crate::worker::{Request, Worker};
use ad_commands::Session;
use ad_document::{
    AssetError, AssetResult, Cancellation, CandidateIndices, DocumentStamp, DocumentTag,
    GeometrySnapshot, ImportLimits, MeshId, PreparedRemoval,
};
use ad_heal::{DuplicatePreview, PlanLimits, RepairKind, VerificationLimits};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EditKind {
    /// G002 for one explicit source primitive and all its references.
    RemoveDuplicates(MeshId),
    /// G001 for one explicit source primitive and all its references.
    RemoveDegenerate(MeshId),
    /// G003 (ADR-0033): compact the primitive's vertex streams.
    RemoveUnused(MeshId),
    /// G012 (ADR-0034): merge exact duplicate vertices.
    MergeVertices(MeshId),
    /// G004 (ADR-0037): create normals for a primitive without them.
    CreateNormals(MeshId),
    /// G005 (ADR-0037): replace zero normals, keep authored ones.
    FixNormals(MeshId),
    /// ADR-0041: replace the model placement (move / uniform scale).
    Place(ad_document::Placement),
    /// ADR-0048: one object edit (node transform or delete).
    Node(ad_document::NodeOp),
    Animation(AnimationEdit, Option<(usize, f32)>),
    Undo,
    Redo,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimationEdit {
    Key(ad_document::AnimationKey),
    Clip,
    Undo,
    Redo,
}

pub struct AnimationFrame {
    pub clip: usize,
    pub time: f32,
    pub rest: Arc<GeometrySnapshot>,
}

impl EditKind {
    /// The source primitive and repair kind of a repair edit.
    pub fn repair(&self) -> Option<(MeshId, RepairKind)> {
        match *self {
            Self::RemoveDuplicates(mesh) => Some((mesh, RepairKind::DuplicateTriangles)),
            Self::RemoveDegenerate(mesh) => Some((mesh, RepairKind::DegenerateTriangles)),
            Self::RemoveUnused(mesh) => Some((mesh, RepairKind::UnusedVertices)),
            Self::MergeVertices(mesh) => Some((mesh, RepairKind::DuplicateVertices)),
            Self::CreateNormals(mesh) => Some((mesh, RepairKind::MissingNormals)),
            Self::FixNormals(mesh) => Some((mesh, RepairKind::ZeroNormals)),
            Self::Undo | Self::Redo | Self::Place(_) | Self::Node(_) | Self::Animation(..) => None,
        }
    }
    pub fn from_repair(mesh: MeshId, kind: RepairKind) -> Self {
        match kind {
            RepairKind::DuplicateTriangles => Self::RemoveDuplicates(mesh),
            RepairKind::DegenerateTriangles => Self::RemoveDegenerate(mesh),
            RepairKind::UnusedVertices => Self::RemoveUnused(mesh),
            RepairKind::DuplicateVertices => Self::MergeVertices(mesh),
            RepairKind::MissingNormals => Self::CreateNormals(mesh),
            RepairKind::ZeroNormals => Self::FixNormals(mesh),
        }
    }
}
/// Diagnostic rule a repair kind removes.
pub fn target_code(kind: RepairKind) -> ad_analysis::Code {
    match kind {
        RepairKind::DuplicateTriangles => ad_analysis::Code::DuplicateTriangle,
        RepairKind::DegenerateTriangles => ad_analysis::Code::ZeroArea,
        RepairKind::UnusedVertices => ad_analysis::Code::UnusedVertex,
        RepairKind::DuplicateVertices => ad_analysis::Code::DuplicateVertex,
        RepairKind::MissingNormals => ad_analysis::Code::MissingNormals,
        RepairKind::ZeroNormals => ad_analysis::Code::ZeroNormal,
    }
}
/// Findings of the rule a repair kind removes, on one source primitive.
pub fn findings_in_mesh(report: &ad_analysis::Report, mesh: MeshId, kind: RepairKind) -> usize {
    let code = target_code(kind);
    report
        .findings
        .iter()
        .filter(|f| f.mesh == mesh && f.code == code)
        .count()
}
pub fn repair_label(kind: RepairKind) -> &'static str {
    match kind {
        RepairKind::DuplicateTriangles => "G002",
        RepairKind::DegenerateTriangles => "G001",
        RepairKind::UnusedVertices => "G003",
        RepairKind::DuplicateVertices => "G012",
        RepairKind::MissingNormals => "G004",
        RepairKind::ZeroNormals => "G005",
    }
}
pub struct EditInput {
    pub session: Arc<Session>,
    pub base: DocumentStamp,
    pub kind: EditKind,
}
enum Change {
    Remove(PreparedRemoval),
    Undo,
    Redo,
}
/// Fully prepared, not yet visible. Single use; bound to its base stamp.
pub struct EditCandidate {
    kind: EditKind,
    /// Source primitive whose indices this edit changes (also for Undo/Redo).
    mesh: MeshId,
    base: DocumentStamp,
    change: Change,
    preview: Option<DuplicatePreview>,
    /// BIN bytes a saved copy appends to the original (export preflight).
    appended: usize,
    /// G006 boundary edges of the primitive before/after a merge.
    boundary: Option<(usize, usize)>,
    snapshot: Arc<GeometrySnapshot>,
    pub animation_frame: Option<AnimationFrame>,
    diagnostics: AssetResult<ad_analysis::Report>,
}
impl EditCandidate {
    pub fn kind(&self) -> EditKind {
        self.kind
    }
    pub fn mesh(&self) -> MeshId {
        self.mesh
    }
    pub fn preview(&self) -> Option<DuplicatePreview> {
        self.preview
    }
    /// Read-only facts for the Preview window; None for Undo/Redo.
    pub fn info(&self) -> Option<crate::viewer_ui::PreviewInfo> {
        let preview = self.preview?;
        let (_, kind) = self.kind.repair()?;
        let code = target_code(kind);
        let report = self.diagnostics.as_ref().ok();
        Some(crate::viewer_ui::PreviewInfo {
            kind,
            mesh: preview.mesh,
            before: preview.before_triangles,
            after: preview.after_triangles,
            vertices_before: preview.before_vertices,
            vertices_after: preview.after_vertices,
            appended_bytes: self.appended,
            boundary_edges: self.boundary,
            changed_normals: preview.changed_normals,
            references: preview.source_node_references,
            complete: report.is_some_and(|r| r.is_complete()),
            target_left: report.map(|r| {
                r.findings
                    .iter()
                    .filter(|f| f.mesh == preview.mesh && f.code == code)
                    .map(|f| f.count)
                    .sum()
            }),
            geometry_bytes: ad_document::geometry_bytes(
                self.snapshot.positions.len(),
                self.snapshot.indices.len(),
            )
            .unwrap_or(usize::MAX),
        })
    }
    pub fn tag_after(&self) -> DocumentTag {
        DocumentTag {
            document: self.snapshot.document_id,
            revision: self.snapshot.revision,
        }
    }
}
pub enum EditOutcome {
    /// Complete scan found nothing to remove: no revision, history or GPU work.
    NoChange(DuplicatePreview),
    Candidate(Box<EditCandidate>),
}

fn stale() -> AssetError {
    AssetError::new(
        ad_document::ErrorCode::Conflict,
        "Edit was prepared for another document or revision",
    )
}

#[cfg(test)]
pub fn prepare_edit(
    session: &Session,
    base: &DocumentStamp,
    kind: EditKind,
    cancel: &Cancellation,
) -> AssetResult<EditOutcome> {
    prepare_edit_with_clip(session, base, kind, None, cancel)
}

/// Read-only preparation. Never mutates the Session.
fn prepare_edit_with_clip(
    session: &Session,
    base: &DocumentStamp,
    kind: EditKind,
    clip_edit: Option<&ad_document::ClipEdit>,
    cancel: &Cancellation,
) -> AssetResult<EditOutcome> {
    cancel.check()?;
    let document = session.document();
    if !base.is_current(document) {
        return Err(stale());
    }
    let (change, preview, removal);
    match kind {
        EditKind::RemoveDuplicates(mesh)
        | EditKind::RemoveDegenerate(mesh)
        | EditKind::RemoveUnused(mesh)
        | EditKind::MergeVertices(mesh)
        | EditKind::CreateNormals(mesh)
        | EditKind::FixNormals(mesh) => {
            let plan = match kind {
                EditKind::CreateNormals(_) | EditKind::FixNormals(_) => ad_heal::plan_normals(
                    document,
                    base.tag(),
                    mesh,
                    kind.repair().expect("repair edit").1,
                    PlanLimits::default(),
                    cancel,
                )?,
                EditKind::MergeVertices(_) => ad_heal::plan_vertex_merge(
                    document,
                    base.tag(),
                    mesh,
                    PlanLimits::default(),
                    cancel,
                )?,
                EditKind::RemoveUnused(_) => {
                    ad_heal::plan_unused(document, base.tag(), mesh, PlanLimits::default(), cancel)?
                }
                EditKind::RemoveDegenerate(_) => ad_heal::plan_degenerate(
                    document,
                    base.tag(),
                    mesh,
                    PlanLimits::default(),
                    cancel,
                )?,
                _ => ad_heal::plan_duplicates(
                    document,
                    base.tag(),
                    mesh,
                    PlanLimits::default(),
                    cancel,
                )?,
            };
            if plan.is_empty() {
                return Ok(EditOutcome::NoChange(plan.preview()));
            }
            removal = plan.prepare(session, VerificationLimits::default(), cancel)?;
            preview = Some(plan.preview());
            change = Change::Remove(removal);
        }
        EditKind::Place(placement) => {
            let plan = session.prepare_placement(base.tag(), placement, cancel)?;
            if !plan.changes() {
                return Err(AssetError::new(
                    ad_document::ErrorCode::Unsupported,
                    "положение модели не изменилось",
                ));
            }
            preview = None;
            change = Change::Remove(plan);
        }
        EditKind::Node(op) => {
            let plan = session.prepare_node_op(base.tag(), op, cancel)?;
            if !plan.changes() {
                return Err(AssetError::new(
                    ad_document::ErrorCode::Unsupported,
                    "объект не изменился",
                ));
            }
            preview = None;
            change = Change::Remove(plan);
        }
        EditKind::Animation(AnimationEdit::Key(key), _) => {
            let plan = session.prepare_animation_key(base.tag(), key, cancel)?;
            if !plan.changes() {
                return Err(AssetError::new(
                    ad_document::ErrorCode::Unsupported,
                    "Ключ не изменился",
                ));
            }
            preview = None;
            change = Change::Remove(plan);
        }
        EditKind::Animation(AnimationEdit::Clip, _) => {
            let plan = session.prepare_clip_edit(base.tag(), clip_edit.ok_or_else(stale)?, cancel)?;
            if !plan.changes() {
                return Err(AssetError::new(
                    ad_document::ErrorCode::Unsupported,
                    "Клип не изменился",
                ));
            }
            preview = None;
            change = Change::Remove(plan);
        }
        EditKind::Undo | EditKind::Animation(AnimationEdit::Undo, _) => {
            preview = None;
            change = Change::Undo;
        }
        EditKind::Redo | EditKind::Animation(AnimationEdit::Redo, _) => {
            preview = None;
            change = Change::Redo;
        }
    }
    let candidate: CandidateIndices<'_> = match &change {
        Change::Remove(removal) => removal.candidate(document, cancel)?,
        Change::Undo => session.undo_candidate()?,
        Change::Redo => session.redo_candidate()?,
    };
    let mesh = candidate.mesh();
    let view = document.candidate_view(candidate, cancel)?;
    let mut appended = 0;
    if kind.repair().is_some() || matches!(kind, EditKind::Place(_) | EditKind::Node(_)) {
        // G002-06: refuse before Apply if the result could not be saved by
        // the verified static exporter (ADR-0028), with its exact reason.
        appended = ad_formats::preflight_changed_export(
            &view,
            ad_formats::ExportLimits::default(),
            ImportLimits::default(),
            cancel,
        )?;
    }
    if candidate.changes_animation()
        && matches!(
            kind,
            EditKind::Animation(AnimationEdit::Key(_) | AnimationEdit::Clip, _)
        )
    {
        ad_formats::verify_animation_export(
            &view,
            ad_formats::ExportLimits::default(),
            ImportLimits::default(),
            cancel,
        )?;
    }
    let rest = Arc::new(view.snapshot(ImportLimits::default(), cancel)?);
    let position = animation_position(kind, clip_edit, document, &view);
    let (snapshot, animation_frame) = if let Some((clip, time)) = position {
        let pose = view.animated_snapshot(clip, time, None, ImportLimits::default(), cancel)?;
        (
            Arc::new(pose),
            Some(AnimationFrame { clip, time, rest }),
        )
    } else {
        (rest, None)
    };
    // Like import, an analysis resource refusal keeps a viewable result with
    // an incomplete report; whole-job cancellation still discards everything.
    let diagnostics = ad_analysis::analyze_view(&view, ad_analysis::Limits::default(), cancel);
    let boundary = match kind {
        EditKind::MergeVertices(mesh) => {
            Some(merge_regression(document, &diagnostics, mesh, cancel)?)
        }
        _ => None,
    };
    cancel.check()?;
    Ok(EditOutcome::Candidate(Box::new(EditCandidate {
        kind,
        mesh,
        base: base.clone(),
        change,
        preview,
        appended,
        boundary,
        snapshot,
        animation_frame,
        diagnostics,
    })))
}

fn animation_position(
    kind: EditKind,
    clip_edit: Option<&ad_document::ClipEdit>,
    document: &ad_document::AssetDocument,
    view: &ad_document::DocumentView<'_>,
) -> Option<(usize, f32)> {
    use ad_document::ClipEdit;
    let EditKind::Animation(_, current) = kind else {
        return None;
    };
    if matches!(
        clip_edit,
        Some(ClipEdit::Create { .. } | ClipEdit::Duplicate { .. })
    ) {
        return view.animation_clips().len().checked_sub(1).map(|last| (last, 0.));
    }
    let (index, time) = current?;
    let previous = document.animation_clips().get(index)?;
    let (next_index, next) = view
        .animation_clips()
        .enumerate()
        .find(|(_, clip)| clip.id() == previous.id())?;
    let time = if previous.duration > 0.
        && previous.duration != next.duration
        && matches!(clip_edit, Some(ClipEdit::Retime { .. }))
    {
        (time.clamp(0., previous.duration) / previous.duration) * next.duration
    } else {
        time.clamp(0., next.duration)
    };
    Some((next_index, time))
}

/// Per-primitive counts a merge must not increase (ADR-0034 decision 6):
/// G001, G002, G006, G007 findings and G008 groups.
const TOPOLOGY: [ad_analysis::Code; 5] = [
    ad_analysis::Code::ZeroArea,
    ad_analysis::Code::DuplicateTriangle,
    ad_analysis::Code::BoundaryEdge,
    ad_analysis::Code::HighIncidenceEdge,
    ad_analysis::Code::DisconnectedComponents,
];
fn topology(report: &ad_analysis::Report, mesh: MeshId) -> [usize; 5] {
    TOPOLOGY.map(|code| {
        report
            .findings
            .iter()
            .filter(|f| f.mesh == mesh && f.code == code)
            .map(|f| f.count)
            .sum()
    })
}
/// Refuses a merge whose complete candidate analysis shows any topology count
/// above the current complete analysis; returns G006 before/after.
fn merge_regression(
    document: &ad_document::AssetDocument,
    after: &AssetResult<ad_analysis::Report>,
    mesh: MeshId,
    cancel: &Cancellation,
) -> AssetResult<(usize, usize)> {
    let refused = |message: String| AssetError::new(ad_document::ErrorCode::Unsupported, message);
    let before = ad_analysis::analyze(document, ad_analysis::Limits::default(), cancel)?;
    let after = match after {
        Ok(report) if report.is_complete() && before.is_complete() => report,
        _ => {
            return Err(refused(
                "слияние не проверено: анализ топологии неполный".into(),
            ));
        }
    };
    let (b, a) = (topology(&before, mesh), topology(after, mesh));
    if a.iter().zip(&b).any(|(a, b)| a > b) {
        let changes = ["G001", "G002", "G006", "G007", "G008"]
            .iter()
            .zip(b.iter().zip(&a))
            .filter(|(_, (b, a))| a > b)
            .map(|(code, (b, a))| format!("{code} {b} -> {a}"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(refused(format!(
            "слияние ухудшит топологию примитива #{}: {changes}",
            mesh.0
        )));
    }
    Ok((b[2], a[2]))
}

struct EditRequest {
    edit: EditInput,
    clip: Option<ad_document::ClipEdit>,
}

pub struct Edits(Worker<EditRequest, EditOutcome>);
impl Edits {
    #[cfg(test)]
    pub fn request(&mut self, edit: EditInput) -> u64 {
        self.request_with_clip(edit, None)
    }
    pub fn request_with_clip(&mut self, edit: EditInput, clip: Option<ad_document::ClipEdit>) -> u64 {
        self.0.request(EditRequest { edit, clip })
    }
    pub fn cancel(&mut self) {
        self.0.cancel();
    }
    pub fn ticket(&self) -> u64 {
        self.0.ticket()
    }
    pub fn poll(&mut self) -> Option<crate::worker::Completion<EditOutcome>> {
        self.0.poll()
    }
}

pub fn new_edits() -> Edits {
    Edits(Worker::new(
        |request: &Request<EditRequest>| {
            let input = &request.input.edit;
            prepare_edit_with_clip(
                &input.session,
                &input.base,
                input.kind,
                request.input.clip.as_ref(),
                &request.cancel,
            )
        },
        "Edit preparation stopped unexpectedly; the model was not changed",
    ))
}

/// GPU side of publication, implemented by the Vulkan renderer and by test fakes.
pub trait SceneStage {
    /// All fallible GPU work. The returned stage blocks further GPU use.
    fn stage_scene<'a>(
        &'a mut self,
        snapshot: &GeometrySnapshot,
    ) -> Result<Box<dyn StagedSceneSink + 'a>, String>;
}
pub trait StagedSceneSink {
    /// Infallible scene replacement.
    fn activate(self: Box<Self>);
}
impl SceneStage for ad_render_vulkan::renderer::Renderer {
    fn stage_scene<'a>(
        &'a mut self,
        snapshot: &GeometrySnapshot,
    ) -> Result<Box<dyn StagedSceneSink + 'a>, String> {
        ad_render_vulkan::renderer::Renderer::stage_scene(self, snapshot)
            .map(|staged| Box::new(staged) as Box<dyn StagedSceneSink + 'a>)
            .map_err(|error| error.to_string())
    }
}
impl StagedSceneSink for ad_render_vulkan::renderer::StagedScene<'_> {
    fn activate(self: Box<Self>) {
        ad_render_vulkan::renderer::StagedScene::activate(*self);
    }
}

#[derive(Debug)]
pub enum PublishError {
    /// Document or revision changed since preparation.
    Stale,
    /// Another holder (e.g. a running worker) still reads the Session.
    Busy,
    /// A Session/document check refused the change before any GPU work.
    Session(AssetError),
    /// GPU upload or idle wait failed; the current scene is still shown.
    Gpu(String),
}
impl std::fmt::Display for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stale => f.write_str("the model changed while the edit was prepared"),
            Self::Busy => f.write_str("the model is still in use by a background task"),
            Self::Session(error) => write!(f, "{error}"),
            Self::Gpu(error) => write!(f, "GPU: {error}"),
        }
    }
}

/// Publishes a prepared edit or changes nothing. Order: stale → exclusive
/// Session → Session stage → GPU stage → infallible publish (ADR-0031).
pub fn publish_edit(
    asset: &mut PreparedAsset,
    scene: &mut dyn SceneStage,
    candidate: EditCandidate,
    cancel: &Cancellation,
) -> Result<DocumentTag, PublishError> {
    let PreparedAsset {
        session,
        generation,
    } = asset;
    if !candidate.base.is_current(session.document()) {
        return Err(PublishError::Stale);
    }
    let session = Arc::get_mut(session).ok_or(PublishError::Busy)?;
    let expected = candidate.tag_after();
    let base = candidate.base.tag();
    let staged = match candidate.change {
        Change::Remove(removal) => session.stage_commit(removal, cancel),
        Change::Undo => session.stage_undo(base, cancel),
        Change::Redo => session.stage_redo(base, cancel),
    }
    .map_err(PublishError::Session)?;
    if staged.tag_after() != expected
        || candidate
            .diagnostics
            .as_ref()
            .is_ok_and(|r| (r.document_id, r.revision) != (expected.document, expected.revision))
    {
        return Err(PublishError::Stale);
    }
    let staged_scene = scene
        .stage_scene(&candidate.snapshot)
        .map_err(PublishError::Gpu)?;
    // Publication point: nothing below can fail or yield to other UI work.
    let tag = staged.publish();
    staged_scene.activate();
    *generation = Generation {
        snapshot: candidate.snapshot,
        diagnostics: candidate.diagnostics,
        stamp: session.document().stamp(),
    };
    Ok(tag)
}

#[cfg(test)]
#[path = "edits_tests.rs"]
mod edits_tests;
