//! ADR-0031 stage B4: desktop publication of edits with a fake GPU sink.
//! Every failure before publication must leave document, revision, history
//! (including Redo), generation and the visible scene exactly as before.
use super::*;
use crate::imports::{PreparedAsset, prepare};
use crate::worker::Request;
use ad_document::{Cancellation, DocumentId, ErrorCode, MeshId};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Default)]
struct FakeScene {
    tag: Option<(DocumentId, u64)>,
    fail: bool,
    staged: usize,
}
struct FakeStaged<'a> {
    scene: &'a mut FakeScene,
    tag: (DocumentId, u64),
}
impl SceneStage for FakeScene {
    fn stage_scene<'a>(
        &'a mut self,
        snapshot: &GeometrySnapshot,
    ) -> Result<Box<dyn StagedSceneSink + 'a>, String> {
        self.staged += 1;
        if self.fail {
            return Err("injected GPU stage failure".into());
        }
        let tag = (snapshot.document_id, snapshot.revision);
        Ok(Box::new(FakeStaged { scene: self, tag }))
    }
}
impl StagedSceneSink for FakeStaged<'_> {
    fn activate(self: Box<Self>) {
        self.scene.tag = Some(self.tag);
    }
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}
fn open(name: &str) -> (PreparedAsset, FakeScene) {
    let asset = prepare(&Request {
        ticket: 1,
        input: fixture(name),
        cancel: Cancellation::default(),
    })
    .unwrap();
    let scene = FakeScene {
        tag: Some((asset.document().id(), asset.document().revision())),
        ..Default::default()
    };
    (asset, scene)
}
fn duplicate_mesh(asset: &PreparedAsset) -> MeshId {
    asset
        .diagnostics()
        .as_ref()
        .unwrap()
        .findings
        .iter()
        .find(|f| f.code == ad_analysis::Code::DuplicateTriangle)
        .unwrap()
        .mesh
}

/// Everything a failed publication must preserve.
#[derive(Debug, PartialEq)]
struct Observed {
    tag: (DocumentId, u64),
    usage: ad_commands::HistoryUsage,
    can_redo: bool,
    generation: (DocumentId, u64),
    report: Option<u64>,
    scene: Option<(DocumentId, u64)>,
    indices: Vec<usize>,
}
fn observe(asset: &PreparedAsset, scene: &FakeScene) -> Observed {
    let document = asset.document();
    Observed {
        tag: (document.id(), document.revision()),
        usage: asset.session.history_usage(),
        can_redo: asset.session.can_redo(),
        generation: (asset.snapshot().document_id, asset.snapshot().revision),
        report: asset.diagnostics().as_ref().ok().map(|r| r.revision),
        scene: scene.tag,
        indices: document
            .primitives()
            .iter()
            .map(|m| m.indices.len())
            .collect(),
    }
}
fn candidate(asset: &PreparedAsset, kind: EditKind) -> EditCandidate {
    match prepare_edit(
        &asset.session,
        &asset.generation.stamp,
        kind,
        &Cancellation::default(),
    )
    .unwrap()
    {
        EditOutcome::Candidate(candidate) => *candidate,
        EditOutcome::NoChange(_) => panic!("expected a change"),
    }
}
fn apply(
    asset: &mut PreparedAsset,
    scene: &mut FakeScene,
    kind: EditKind,
) -> ad_document::DocumentTag {
    let candidate = candidate(asset, kind);
    publish_edit(asset, scene, candidate, &Cancellation::default()).unwrap()
}

#[test]
fn apply_publishes_document_history_generation_and_scene_together() {
    let (mut asset, mut scene) = open("Duplicate_Heal.glb");
    let mesh = duplicate_mesh(&asset);
    let c = Cancellation::default();
    let candidate = candidate(&asset, EditKind::RemoveDuplicates(mesh));
    let preview = candidate.preview().unwrap();
    assert_eq!(
        (preview.before_triangles, preview.after_triangles),
        (992, 960)
    );
    assert_eq!(candidate.tag_after().revision, 1);
    // Preparation alone is not publication.
    assert_eq!(asset.document().revision(), 0);
    assert_eq!(scene.staged, 0);
    let tag = publish_edit(&mut asset, &mut scene, candidate, &c).unwrap();
    assert_eq!(tag.revision, 1);
    assert!(asset.is_consistent());
    assert_eq!(scene.tag, Some((tag.document, 1)));
    assert!(asset.session.can_undo() && !asset.session.can_redo());
    let report = asset.diagnostics().as_ref().unwrap();
    assert!(report.is_complete(), "G002 absence needs complete coverage");
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.mesh == mesh && f.code == ad_analysis::Code::DuplicateTriangle)
    );
    assert_eq!(asset.document().primitives()[mesh.0].indices.len(), 960 * 3);
}

#[test]
fn gpu_failure_keeps_document_history_redo_generation_and_scene() {
    let (mut asset, mut scene) = open("Duplicate_Heal.glb");
    let mesh = duplicate_mesh(&asset);
    apply(&mut asset, &mut scene, EditKind::RemoveDuplicates(mesh));
    apply(&mut asset, &mut scene, EditKind::Undo);
    assert!(asset.session.can_redo());
    let before = observe(&asset, &scene);
    let candidate = candidate(&asset, EditKind::RemoveDuplicates(mesh));
    scene.fail = true;
    let result = publish_edit(&mut asset, &mut scene, candidate, &Cancellation::default());
    assert!(matches!(result, Err(PublishError::Gpu(_))));
    assert_eq!(observe(&asset, &scene), before, "nothing became visible");
    assert!(asset.is_consistent());
    scene.fail = false;
    // Redo survived the failed branch and still publishes (AT-09/AT-10).
    let tag = apply(&mut asset, &mut scene, EditKind::Redo);
    assert_eq!(tag.revision, 3);
    assert!(asset.is_consistent());
    assert_eq!(scene.tag, Some((tag.document, 3)));
    assert_eq!(asset.document().primitives()[mesh.0].indices.len(), 960 * 3);
    let tag = apply(&mut asset, &mut scene, EditKind::Undo);
    assert_eq!(tag.revision, 4);
    assert_eq!(asset.document().primitives()[mesh.0].indices.len(), 992 * 3);
    assert!(asset.is_consistent());
}

#[test]
fn stale_candidates_and_a_busy_session_are_refused_without_change() {
    let (mut asset, mut scene) = open("Duplicate_Heal.glb");
    let mesh = duplicate_mesh(&asset);
    let first = candidate(&asset, EditKind::RemoveDuplicates(mesh));
    let second = candidate(&asset, EditKind::RemoveDuplicates(mesh));
    publish_edit(&mut asset, &mut scene, first, &Cancellation::default()).unwrap();
    let before = observe(&asset, &scene);
    let result = publish_edit(&mut asset, &mut scene, second, &Cancellation::default());
    assert!(matches!(result, Err(PublishError::Stale)));
    assert_eq!(observe(&asset, &scene), before);

    let undo = candidate(&asset, EditKind::Undo);
    let reader = asset.session.clone(); // e.g. a still-running worker
    let result = publish_edit(&mut asset, &mut scene, undo, &Cancellation::default());
    assert!(matches!(result, Err(PublishError::Busy)));
    assert_eq!(observe(&asset, &scene), before);
    drop(reader);
    assert_eq!(apply(&mut asset, &mut scene, EditKind::Undo).revision, 2);
}

#[test]
fn cancellation_before_publication_changes_nothing() {
    let (mut asset, mut scene) = open("Duplicate_Heal.glb");
    let mesh = duplicate_mesh(&asset);
    let before = observe(&asset, &scene);
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let result = prepare_edit(
        &asset.session,
        &asset.generation.stamp,
        EditKind::RemoveDuplicates(mesh),
        &cancelled,
    );
    assert!(matches!(result, Err(e) if e.code == ErrorCode::Cancelled));
    let candidate = candidate(&asset, EditKind::RemoveDuplicates(mesh));
    let result = publish_edit(&mut asset, &mut scene, candidate, &cancelled);
    assert!(matches!(result, Err(PublishError::Session(e)) if e.code == ErrorCode::Cancelled));
    assert_eq!(observe(&asset, &scene), before);
    assert_eq!(scene.staged, 0, "GPU work starts only after Session checks");
}

#[test]
fn no_duplicates_and_empty_history_are_explicit_no_change() {
    let (mut asset, mut scene) = open("Duplicate_Heal.glb");
    let mesh = duplicate_mesh(&asset);
    let before = observe(&asset, &scene);
    let clean = (0..asset.document().primitives().len())
        .map(MeshId)
        .find(|&m| m != mesh)
        .unwrap();
    let c = Cancellation::default();
    let outcome = prepare_edit(
        &asset.session,
        &asset.generation.stamp,
        EditKind::RemoveDuplicates(clean),
        &c,
    )
    .unwrap();
    assert!(matches!(outcome, EditOutcome::NoChange(p) if p.before_triangles == p.after_triangles));
    assert!(matches!(
        prepare_edit(&asset.session, &asset.generation.stamp, EditKind::Undo, &c),
        Err(e) if e.code == ErrorCode::HistoryEmpty
    ));
    assert_eq!(observe(&asset, &scene), before);
    apply(&mut asset, &mut scene, EditKind::RemoveDuplicates(mesh));
    let outcome = prepare_edit(
        &asset.session,
        &asset.generation.stamp,
        EditKind::RemoveDuplicates(mesh),
        &c,
    )
    .unwrap();
    assert!(
        matches!(outcome, EditOutcome::NoChange(_)),
        "second Apply is a no-op"
    );
    assert_eq!(asset.document().revision(), 1);
}

#[test]
fn edit_worker_releases_the_session_before_its_latest_result_is_visible() {
    let (mut asset, mut scene) = open("Duplicate_Heal.glb");
    let mesh = duplicate_mesh(&asset);
    let mut edits = new_edits();
    let input = |kind| EditInput {
        session: asset.session.clone(),
        base: asset.generation.stamp.clone(),
        kind,
    };
    edits.request(input(EditKind::Undo));
    let ticket = edits.request(input(EditKind::RemoveDuplicates(mesh)));
    let start = Instant::now();
    let completion = loop {
        if let Some(completion) = edits.poll() {
            break completion;
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(completion.ticket, ticket);
    assert_eq!(std::sync::Arc::strong_count(&asset.session), 1);
    let EditOutcome::Candidate(candidate) = completion.result.unwrap() else {
        panic!("expected a change");
    };
    let tag = publish_edit(&mut asset, &mut scene, *candidate, &Cancellation::default()).unwrap();
    assert_eq!(tag.revision, 1);
    assert!(asset.is_consistent());
}

/// Duplicate_Heal with root-level `extras`: viewable, but a changed export
/// would drop or alter opaque metadata, so Apply must be refused (G002-06).
fn with_root_extras(source: &[u8]) -> Vec<u8> {
    let json_len = u32::from_le_bytes(source[12..16].try_into().unwrap()) as usize;
    let json = &source[20..20 + json_len];
    let mut patched = b"{\"extras\":{\"owner\":1},".to_vec();
    patched.extend_from_slice(&json[1..]);
    while !patched.len().is_multiple_of(4) {
        patched.push(b' ');
    }
    let rest = &source[20 + json_len..];
    let total = 20 + patched.len() + rest.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&source[0..8]);
    out.extend((total as u32).to_le_bytes());
    out.extend((patched.len() as u32).to_le_bytes());
    out.extend_from_slice(&source[16..20]);
    out.extend_from_slice(&patched);
    out.extend_from_slice(rest);
    out
}

#[test]
fn unsupported_changed_export_refuses_apply_before_any_change() {
    let source = std::fs::read(fixture("Duplicate_Heal.glb")).unwrap();
    let c = Cancellation::default();
    let limits = ad_document::ImportLimits::default();
    let document = ad_formats::import_glb(
        with_root_extras(&source).into(),
        "extras".into(),
        DocumentId(9),
        limits,
        &c,
    )
    .unwrap();
    let snapshot = std::sync::Arc::new(document.snapshot(limits, &c).unwrap());
    let diagnostics = ad_analysis::analyze(&document, Default::default(), &c);
    let stamp = document.stamp();
    let mut asset = PreparedAsset {
        session: std::sync::Arc::new(
            ad_commands::Session::new(document, Default::default(), limits).unwrap(),
        ),
        generation: crate::imports::Generation {
            snapshot,
            diagnostics,
            stamp,
        },
    };
    let scene = FakeScene {
        tag: Some((asset.document().id(), 0)),
        ..Default::default()
    };
    let before = observe(&asset, &scene);
    let mesh = duplicate_mesh(&asset);
    let result = prepare_edit(
        &asset.session,
        &asset.generation.stamp,
        EditKind::RemoveDuplicates(mesh),
        &c,
    );
    assert!(matches!(result, Err(e) if e.code == ErrorCode::Unsupported));
    assert_eq!(observe(&asset, &scene), before);
    assert!(asset.is_consistent());
    // The model stays viewable and its unchanged bytes still export exactly.
    let copy = ad_formats::prepare_glb(
        asset.document(),
        asset.document().tag(),
        Default::default(),
        limits,
        &c,
    )
    .unwrap();
    assert_eq!(&**copy.bytes(), &*asset.document().source().clone());
    let _ = &mut asset;
}

#[test]
fn degenerate_repair_uses_the_same_preview_publication_and_history() {
    let (mut asset, mut scene) = open("Geometry_Findings.glb");
    let c = Cancellation::default();
    let original = asset.document().primitives()[0].indices.clone();
    let candidate = candidate(&asset, EditKind::RemoveDegenerate(MeshId(0)));
    let info = candidate.info().unwrap();
    assert_eq!(info.kind, ad_heal::RepairKind::DegenerateTriangles);
    assert_eq!((info.before, info.after, info.references), (5, 3, 2));
    assert!(info.complete);
    assert_eq!(info.target_left, Some(0));
    assert_eq!(asset.document().revision(), 0);
    let tag = publish_edit(&mut asset, &mut scene, candidate, &c).unwrap();
    assert_eq!(tag.revision, 1);
    assert!(asset.is_consistent());
    assert_eq!(asset.document().primitives()[0].indices.len(), 9);
    let report = asset.diagnostics().as_ref().unwrap();
    assert!(report.is_complete());
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.mesh == MeshId(0) && f.code == ad_analysis::Code::ZeroArea)
    );
    apply(&mut asset, &mut scene, EditKind::Undo);
    assert_eq!(asset.document().primitives()[0].indices, original);
    // Undo/Redo candidates name the primitive their history entry changes, and
    // the per-rule counts are read for that primitive (manual-check log bug:
    // Undo/Redo once reported G002 under the G001 label).
    let redo = self::candidate(&asset, EditKind::Redo);
    assert_eq!(redo.mesh(), MeshId(0));
    publish_edit(&mut asset, &mut scene, redo, &c).unwrap();
    let report = asset.diagnostics().as_ref().unwrap();
    assert_eq!(
        findings_in_mesh(report, MeshId(0), ad_heal::RepairKind::DegenerateTriangles),
        0
    );
    assert_eq!(
        findings_in_mesh(report, MeshId(0), ad_heal::RepairKind::DuplicateTriangles),
        1
    );
    // Repeating the repair after Redo is a no-op.
    let outcome = prepare_edit(
        &asset.session,
        &asset.generation.stamp,
        EditKind::RemoveDegenerate(MeshId(0)),
        &c,
    )
    .unwrap();
    assert!(matches!(outcome, EditOutcome::NoChange(_)));
}

#[test]
fn normals_repairs_create_and_fix_normals_and_refuse_unusable_vertices() {
    let (mut asset, mut scene) = open("Normals_Repair.glb");
    let c = Cancellation::default();
    for (kind, mesh, changed, code) in [
        (
            EditKind::CreateNormals(MeshId(0)),
            0,
            4,
            ad_heal::RepairKind::MissingNormals,
        ),
        (
            EditKind::FixNormals(MeshId(1)),
            1,
            2,
            ad_heal::RepairKind::ZeroNormals,
        ),
    ] {
        assert_eq!(kind.repair(), Some((MeshId(mesh), code)));
        let candidate = candidate(&asset, kind);
        let info = candidate.info().unwrap();
        assert_eq!(info.changed_normals, changed);
        assert!(info.complete);
        assert_eq!(info.target_left, Some(0));
        publish_edit(&mut asset, &mut scene, candidate, &c).unwrap();
        let report = asset.diagnostics().as_ref().unwrap();
        assert_eq!(findings_in_mesh(report, MeshId(mesh), code), 0);
    }
    assert_eq!(repair_label(ad_heal::RepairKind::MissingNormals), "G004");
    assert_eq!(repair_label(ad_heal::RepairKind::ZeroNormals), "G005");
    let result = prepare_edit(
        &asset.session,
        &asset.generation.stamp,
        EditKind::FixNormals(MeshId(2)),
        &c,
    );
    assert!(
        matches!(result, Err(e) if e.code == ErrorCode::Unsupported && e.message.contains("G003"))
    );
    apply(&mut asset, &mut scene, EditKind::Undo);
    assert!(asset.document().primitives()[1].normals.as_ref().unwrap()[1] == [0.; 3]);
}

#[test]
fn vertex_merge_welds_a_seam_and_refuses_topology_regression() {
    let (mut asset, mut scene) = open("Vertex_Weld.glb");
    let c = Cancellation::default();
    let kind = EditKind::MergeVertices(MeshId(0));
    assert_eq!(
        kind.repair(),
        Some((MeshId(0), ad_heal::RepairKind::DuplicateVertices))
    );
    assert_eq!(repair_label(ad_heal::RepairKind::DuplicateVertices), "G012");
    let candidate = candidate(&asset, kind);
    let info = candidate.info().unwrap();
    assert_eq!((info.vertices_before, info.vertices_after), (9, 7));
    assert_eq!((info.before, info.after), (3, 3));
    assert_eq!(info.boundary_edges, Some((9, 7)), "the split seam welds");
    assert!(info.complete);
    assert_eq!(info.target_left, Some(0));
    publish_edit(&mut asset, &mut scene, candidate, &c).unwrap();
    assert_eq!(asset.document().primitives()[0].positions.len(), 7);
    let report = asset.diagnostics().as_ref().unwrap();
    assert_eq!(
        findings_in_mesh(report, MeshId(0), ad_heal::RepairKind::DuplicateVertices),
        0
    );
    apply(&mut asset, &mut scene, EditKind::Undo);
    assert_eq!(asset.document().primitives()[0].positions.len(), 9);

    // Merging two coincident triangles' vertices would create a duplicate
    // triangle: the edit is refused before anything becomes visible.
    let (asset, _) = open("Edge_Topology.glb");
    let mesh = asset
        .diagnostics()
        .as_ref()
        .unwrap()
        .findings
        .iter()
        .find(|f| f.code == ad_analysis::Code::DuplicateVertex)
        .unwrap()
        .mesh;
    let result = prepare_edit(
        &asset.session,
        &asset.generation.stamp,
        EditKind::MergeVertices(mesh),
        &c,
    );
    assert!(matches!(result, Err(e) if e.code == ErrorCode::Unsupported));
    assert_eq!(asset.document().revision(), 0);
}

#[test]
fn unused_vertex_repair_compacts_publishes_undoes_and_saves() {
    let (mut asset, mut scene) = open("Geometry_Findings.glb");
    let c = Cancellation::default();
    let kind = EditKind::RemoveUnused(MeshId(1));
    assert_eq!(
        kind.repair(),
        Some((MeshId(1), ad_heal::RepairKind::UnusedVertices))
    );
    assert_eq!(
        EditKind::from_repair(MeshId(1), ad_heal::RepairKind::UnusedVertices),
        kind
    );
    assert_eq!(repair_label(ad_heal::RepairKind::UnusedVertices), "G003");
    let candidate = candidate(&asset, kind.clone());
    let info = candidate.info().unwrap();
    assert_eq!(info.kind, ad_heal::RepairKind::UnusedVertices);
    assert_eq!((info.before, info.after), (1, 1), "triangles are kept");
    assert_eq!((info.vertices_before, info.vertices_after), (5, 3));
    assert_eq!(info.references, 1);
    assert!(info.complete);
    assert_eq!(info.target_left, Some(0));
    // POSITION and NORMAL (12 + 12 bytes) for 3 vertices plus 3 U32 indices.
    assert_eq!(info.appended_bytes, 3 * 24 + 3 * 4);
    assert_eq!(asset.document().revision(), 0);
    publish_edit(&mut asset, &mut scene, candidate, &c).unwrap();
    assert!(asset.is_consistent());
    assert_eq!(asset.document().primitives()[1].positions.len(), 3);
    assert_eq!(asset.document().primitives()[0].positions.len(), 5);
    let report = asset.diagnostics().as_ref().unwrap();
    assert!(report.is_complete());
    assert_eq!(
        findings_in_mesh(report, MeshId(1), ad_heal::RepairKind::UnusedVertices),
        0
    );
    assert_eq!(
        findings_in_mesh(report, MeshId(0), ad_heal::RepairKind::UnusedVertices),
        1,
        "the sibling sharing the accessor is untouched"
    );
    let saved = ad_formats::prepare_glb(
        asset.document(),
        asset.document().tag(),
        Default::default(),
        ImportLimits::default(),
        &c,
    )
    .unwrap();
    let reopened = ad_formats::import_glb(
        saved.bytes().clone(),
        "copy".into(),
        DocumentId(9),
        ImportLimits::default(),
        &c,
    )
    .unwrap();
    assert_eq!(reopened.primitives()[1].positions.len(), 3);
    let undo = self::candidate(&asset, EditKind::Undo);
    assert_eq!(undo.mesh(), MeshId(1));
    publish_edit(&mut asset, &mut scene, undo, &c).unwrap();
    assert_eq!(asset.document().primitives()[1].positions.len(), 5);
    apply(&mut asset, &mut scene, EditKind::Redo);
    assert_eq!(asset.document().primitives()[1].positions.len(), 3);
    let outcome = prepare_edit(&asset.session, &asset.generation.stamp, kind, &c).unwrap();
    assert!(matches!(outcome, EditOutcome::NoChange(_)));
}

#[test]
fn a_joint_edit_is_prepared_verified_published_and_undone() {
    use ad_document::{JointEdit, NodeId};
    let (mut asset, mut scene) = open("Animation_Checker.glb");
    let nodes = asset.document().nodes().len();
    let cancel = Cancellation::default();
    let mut run = |asset: &mut PreparedAsset, kind: EditKind, joint: Option<JointEdit>| {
        let outcome = prepare_edit_with_clip(
            &asset.session,
            &asset.generation.stamp,
            kind,
            None,
            joint.as_ref(),
            &cancel,
        )
        .unwrap();
        let EditOutcome::Candidate(candidate) = outcome else {
            panic!("expected a change");
        };
        publish_edit(asset, &mut scene, *candidate, &cancel).unwrap();
    };
    run(
        &mut asset,
        EditKind::Animation(AnimationEdit::Joint, Some((0, 0.))),
        Some(JointEdit::Add {
            parent: NodeId(1),
            name: "Tip_end".into(),
            translation: [0., 0.25, 0.],
        }),
    );
    assert_eq!(asset.document().nodes().len(), nodes + 1);
    assert!(asset.session.can_undo());
    run(
        &mut asset,
        EditKind::Animation(AnimationEdit::Undo, Some((0, 0.))),
        None,
    );
    assert_eq!(asset.document().nodes().len(), nodes);
    // A joint edit without its edit data is refused as stale.
    assert!(
        prepare_edit_with_clip(
            &asset.session,
            &asset.generation.stamp,
            EditKind::Animation(AnimationEdit::Joint, None),
            None,
            None,
            &cancel,
        )
        .is_err()
    );
}
