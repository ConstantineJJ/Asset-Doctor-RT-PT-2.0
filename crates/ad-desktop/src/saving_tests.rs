//! ADR-0031 stage D: Save Copy, receipt-based dirty state and unsaved guard.
use super::*;
use crate::edits::{
    EditKind, EditOutcome, SceneStage, StagedSceneSink, prepare_edit, publish_edit,
};
use crate::imports::{PreparedAsset, prepare};
use crate::worker::Request;
use ad_document::{Cancellation, ErrorCode, GeometrySnapshot, MeshId};
use std::{fs, path::PathBuf};

struct Scene;
struct Staged;
impl SceneStage for Scene {
    fn stage_scene<'a>(
        &'a mut self,
        _: &GeometrySnapshot,
    ) -> Result<Box<dyn StagedSceneSink + 'a>, String> {
        Ok(Box::new(Staged))
    }
}
impl StagedSceneSink for Staged {
    fn activate(self: Box<Self>) {}
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/Duplicate_Heal.glb")
}
fn open() -> (PreparedAsset, Confirmed) {
    let asset = prepare(&Request {
        ticket: 1,
        input: fixture(),
        cancel: Cancellation::default(),
    })
    .unwrap();
    let confirmed = Confirmed::imported(&asset);
    (asset, confirmed)
}
fn edit(asset: &mut PreparedAsset, kind: EditKind) {
    let c = Cancellation::default();
    let EditOutcome::Candidate(candidate) =
        prepare_edit(&asset.session, &asset.generation.stamp, kind, &c).unwrap()
    else {
        panic!("expected a change");
    };
    publish_edit(asset, &mut Scene, *candidate, &c).unwrap();
}
/// Owned directory under ignored workspace artifacts, never user folders.
fn directory() -> PathBuf {
    let parent =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/desktop-save-tests");
    fs::create_dir_all(&parent).unwrap();
    for n in 0..64 {
        let path = parent.join(format!(
            "owned-{}-{}-{n}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        if fs::create_dir(&path).is_ok() {
            return path.canonicalize().unwrap();
        }
    }
    panic!("cannot create owned test directory")
}
fn input(asset: &PreparedAsset, path: PathBuf) -> SaveInput {
    SaveInput {
        session: asset.session.clone(),
        base: asset.generation.stamp.clone(),
        target: SaveTarget::Copy(path),
    }
}

/// ADR-0035: «Сохранить» replaces the working file only after verification,
/// removes its backup afterwards and refuses a file changed on disk.
#[test]
fn save_over_working_file_verifies_replaces_and_refuses_outside_changes() {
    let dir = directory();
    let work = dir.join("work.glb");
    fs::copy(fixture(), &work).unwrap();
    let mut asset = prepare(&Request {
        ticket: 1,
        input: work.clone(),
        cancel: Cancellation::default(),
    })
    .unwrap();
    edit(&mut asset, EditKind::RemoveDuplicates(MeshId(0)));
    let replace = |asset: &PreparedAsset| SaveInput {
        session: asset.session.clone(),
        base: asset.generation.stamp.clone(),
        target: SaveTarget::Replace {
            path: work.clone(),
            on_disk: asset.document().source().clone(),
        },
    };
    let outcome = save_copy_job(&replace(&asset), &Cancellation::default()).unwrap();
    assert!(outcome.replaced && !outcome.backup_kept);
    assert!(outcome.receipt.is_current(asset.document()));
    let saved = fs::read(&work).unwrap();
    assert_ne!(saved, fs::read(fixture()).unwrap());
    assert_eq!(
        fs::read_dir(&dir).unwrap().count(),
        1,
        "backup removed after verification"
    );
    // The program still believes the old bytes are on disk: a second Save must
    // refuse instead of overwriting what is now there.
    let e = save_copy_job(&replace(&asset), &Cancellation::default())
        .err()
        .unwrap();
    assert_eq!(e.code, ErrorCode::Conflict);
    assert_eq!(fs::read(&work).unwrap(), saved);
}

#[test]
fn save_copy_publishes_a_new_verified_file_and_clears_dirty_only_for_that_revision() {
    let original = fs::read(fixture()).unwrap();
    let (mut asset, mut confirmed) = open();
    assert!(
        !confirmed.is_dirty(&asset),
        "fresh import is the confirmed state"
    );
    edit(&mut asset, EditKind::RemoveDuplicates(MeshId(0)));
    assert!(confirmed.is_dirty(&asset));
    let dir = directory();
    let target = dir.join("healed.glb");
    let outcome = save_copy_job(&input(&asset, target.clone()), &Cancellation::default()).unwrap();
    assert_eq!(outcome.receipt.tag(), asset.document().tag());
    assert!(!outcome.exact_copy, "changed indices need a rewritten copy");
    assert_eq!(outcome.reimported_triangles, 3840);
    assert_eq!(fs::metadata(&target).unwrap().len() as usize, outcome.bytes);
    assert!(confirmed.accept(outcome.receipt, &asset));
    assert!(!confirmed.is_dirty(&asset));
    assert_eq!(
        fs::read(fixture()).unwrap(),
        original,
        "source never written"
    );
    // A later edit makes the model dirty again; Undo does not "unsave" it.
    edit(&mut asset, EditKind::Undo);
    assert!(confirmed.is_dirty(&asset));
    // Saving the restored original content is an exact source copy.
    let restored = dir.join("restored.glb");
    let outcome =
        save_copy_job(&input(&asset, restored.clone()), &Cancellation::default()).unwrap();
    assert!(outcome.exact_copy);
    assert_eq!(fs::read(&restored).unwrap(), original);
}

#[test]
fn existing_destination_cancel_stale_and_late_receipts_never_count_as_saved() {
    let (mut asset, mut confirmed) = open();
    edit(&mut asset, EditKind::RemoveDuplicates(MeshId(0)));
    let dir = directory();
    let existing = dir.join("existing.glb");
    fs::write(&existing, b"owner data").unwrap();
    let result = save_copy_job(&input(&asset, existing.clone()), &Cancellation::default());
    assert!(result.is_err());
    assert_eq!(fs::read(&existing).unwrap(), b"owner data");
    assert!(confirmed.is_dirty(&asset));

    let cancelled = Cancellation::default();
    cancelled.cancel();
    let target = dir.join("cancelled.glb");
    let result = save_copy_job(&input(&asset, target.clone()), &cancelled);
    assert!(matches!(result, Err(e) if e.code == ErrorCode::Cancelled));
    assert!(!target.exists());

    // Holding a Session clone (as a running save does) blocks publication.
    let stale_stamp = asset.generation.stamp.clone();
    edit(&mut asset, EditKind::Undo);
    let stale = SaveInput {
        session: asset.session.clone(),
        base: stale_stamp,
        target: SaveTarget::Copy(dir.join("stale.glb")),
    };
    let result = save_copy_job(&stale, &Cancellation::default());
    assert!(matches!(result, Err(e) if e.code == ErrorCode::Conflict));
    drop(stale);
    assert!(!dir.join("stale.glb").exists());

    // A receipt for an older revision is a real file but not the current state.
    edit(&mut asset, EditKind::Redo);
    let late = save_copy_job(
        &input(&asset, dir.join("late.glb")),
        &Cancellation::default(),
    )
    .unwrap();
    edit(&mut asset, EditKind::Undo);
    assert!(!confirmed.accept(late.receipt, &asset));
    assert!(confirmed.is_dirty(&asset));
}

#[test]
fn unsaved_guard_prompts_only_for_dirty_models_and_cancel_keeps_everything() {
    assert_eq!(guard(false, None), Guard::Proceed);
    assert_eq!(guard(true, None), Guard::Ask);
    assert_eq!(guard(true, Some(UnsavedChoice::Discard)), Guard::Proceed);
    assert_eq!(guard(true, Some(UnsavedChoice::Cancel)), Guard::Stay);
    assert_eq!(
        guard(true, Some(UnsavedChoice::SaveCopy)),
        Guard::SaveThenProceed
    );
}

#[test]
fn save_worker_releases_the_session_before_its_receipt_is_visible() {
    let (mut asset, _) = open();
    edit(&mut asset, EditKind::RemoveDuplicates(MeshId(0)));
    let mut saves = new_saves();
    let ticket = saves.request(input(&asset, directory().join("worker.glb")));
    let start = std::time::Instant::now();
    let completion = loop {
        if let Some(c) = saves.poll() {
            break c;
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert_eq!(completion.ticket, ticket);
    assert_eq!(std::sync::Arc::strong_count(&asset.session), 1);
    assert!(
        completion
            .result
            .unwrap()
            .receipt
            .is_current(asset.document())
    );
}
