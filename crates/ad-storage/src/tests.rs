use super::*;
use ad_document::{Cancellation, DocumentId, ImportLimits, MeshId};
use std::{fs, path::PathBuf, sync::Arc};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let parent =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/storage-tests");
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
                return Self(path.canonicalize().unwrap());
            }
        }
        panic!("cannot create owned test directory")
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn stages(&self) -> usize {
        fs::read_dir(&self.0)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".asset-doctor-copy-")
            })
            .count()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        // Only flat files inside this freshly created owned directory; never recurse.
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../artifacts/storage-tests")
            .canonicalize()
            .unwrap();
        if !self.0.starts_with(root) {
            return;
        }
        if let Ok(entries) = fs::read_dir(&self.0) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|t| t.is_file()) {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        let _ = fs::remove_dir(&self.0);
    }
}
fn document() -> ad_document::AssetDocument {
    ad_formats::import_glb(
        Arc::from(include_bytes!("../../../tests/fixtures/Geometry_Findings.glb").as_slice()),
        "storage".into(),
        DocumentId(1),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap()
}
fn proof(doc: &ad_document::AssetDocument) -> ad_formats::VerifiedGlb {
    ad_formats::prepare_glb(
        doc,
        doc.tag(),
        Default::default(),
        Default::default(),
        &Cancellation::default(),
    )
    .unwrap()
}
#[test]
fn saved_copy_is_verified_from_disk_and_preserves_current_document() {
    let dir = Directory::new();
    let mut doc = document();
    let c = Cancellation::default();
    let original = doc.source().clone();
    let p = doc
        .prepare_triangle_removal(doc.tag(), MeshId(0), &[1], Default::default(), &c)
        .unwrap();
    doc.apply_triangle_removal(p, Default::default(), &c)
        .unwrap();
    let verified = proof(&doc);
    let path = dir.path("copy.glb");
    let saved = save_copy(&verified, &doc, &path, &c).unwrap();
    assert_eq!(saved.tag(), doc.tag());
    assert_eq!(saved.path(), path);
    assert!(!saved.cleanup_warning());
    assert!(saved.is_current(&doc));
    assert!(!saved.is_current(&document()));
    let bytes: Arc<[u8]> = fs::read(&path).unwrap().into();
    assert_eq!(&**verified.bytes(), bytes.as_ref());
    let reimport =
        ad_formats::import_glb(bytes, "disk".into(), DocumentId(2), Default::default(), &c)
            .unwrap();
    assert_eq!(
        reimport.primitives()[0].indices,
        doc.primitives()[0].indices
    );
    assert!(Arc::ptr_eq(doc.source(), &original));
    assert_eq!(dir.stages(), 0);
}

#[test]
fn existing_destination_and_original_path_are_never_replaced() {
    let dir = Directory::new();
    let doc = document();
    let v = proof(&doc);
    for (name, content) in [
        ("existing.glb", b"owner bytes".as_slice()),
        ("original.glb", doc.source().as_ref()),
    ] {
        let path = dir.path(name);
        fs::write(&path, content).unwrap();
        assert!(
            matches!(save_copy(&v,&doc,&path,&Cancellation::default()),Err(e) if e.code==ErrorCode::Io)
        );
        assert_eq!(fs::read(path).unwrap(), content);
        assert_eq!(dir.stages(), 0);
    }
}

#[test]
fn receipt_retains_private_origin_when_public_tags_are_equal() {
    let dir = Directory::new();
    let mut doc = document();
    let foreign = document();
    let c = Cancellation::default();
    let saved = save_copy(&proof(&doc), &doc, &dir.path("copy.glb"), &c).unwrap();
    assert_eq!(saved.tag(), foreign.tag());
    assert!(saved.is_current(&doc));
    assert!(!saved.is_current(&foreign));
    let p = doc
        .prepare_triangle_removal(doc.tag(), MeshId(0), &[1], Default::default(), &c)
        .unwrap();
    doc.apply_triangle_removal(p, Default::default(), &c)
        .unwrap();
    assert!(!saved.is_current(&doc));
}

#[test]
fn stale_foreign_and_cancelled_proofs_do_not_create_files() {
    let dir = Directory::new();
    let mut doc = document();
    let v = proof(&doc);
    let c = Cancellation::default();
    let foreign = document();
    assert_eq!(doc.tag(), foreign.tag());
    assert!(
        matches!(save_copy(&v,&foreign,&dir.path("foreign.glb"),&c),Err(e) if e.code==ErrorCode::Conflict)
    );
    let p = doc
        .prepare_triangle_removal(doc.tag(), MeshId(0), &[1], Default::default(), &c)
        .unwrap();
    doc.apply_triangle_removal(p, Default::default(), &c)
        .unwrap();
    assert!(
        matches!(save_copy(&v,&doc,&dir.path("stale.glb"),&c),Err(e) if e.code==ErrorCode::Conflict)
    );
    let v = proof(&doc);
    c.cancel();
    assert!(
        matches!(save_copy(&v,&doc,&dir.path("cancel.glb"),&c),Err(e) if e.code==ErrorCode::Cancelled)
    );
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 0);
}

#[test]
fn cancellation_after_readback_leaves_no_destination_or_staging() {
    let dir = Directory::new();
    let doc = document();
    let v = proof(&doc);
    let c = Cancellation::default();
    let path = dir.path("cancel.glb");
    let result = save_copy_with(&v, &doc, &path, &c, |stage, _| {
        if stage == Stage::Ready {
            c.cancel();
        }
        Ok(())
    });
    assert!(matches!(result,Err(e) if e.code==ErrorCode::Cancelled));
    assert!(!path.exists());
    assert_eq!(dir.stages(), 0);
}

#[test]
fn disk_readback_refuses_corruption_and_truncation() {
    let dir = Directory::new();
    let doc = document();
    let v = proof(&doc);
    for truncate in [false, true] {
        let path = dir.path("bad.glb");
        let result = save_copy_with(&v, &doc, &path, &Cancellation::default(), |stage, file| {
            if stage == Stage::Written {
                if truncate {
                    file.set_len(1).map_err(io_error)?;
                } else {
                    file.seek(SeekFrom::Start(0)).map_err(io_error)?;
                    file.write_all(b"BAD!").map_err(io_error)?;
                }
            }
            Ok(())
        });
        assert!(matches!(result,Err(e) if e.code==ErrorCode::VerificationFailed));
        assert!(!path.exists());
        assert_eq!(dir.stages(), 0);
    }
}

#[test]
fn injected_io_failure_cleans_partial_staging() {
    let dir = Directory::new();
    let doc = document();
    let v = proof(&doc);
    let path = dir.path("failure.glb");
    let result = save_copy_with(&v, &doc, &path, &Cancellation::default(), |stage, file| {
        if stage == Stage::Created {
            file.write_all(b"partial").map_err(io_error)?;
            return Err(AssetError::new(
                ErrorCode::Io,
                "injected after partial staging write",
            ));
        }
        Ok(())
    });
    assert!(matches!(result,Err(e) if e.code==ErrorCode::Io));
    assert!(!path.exists());
    assert_eq!(dir.stages(), 0);
}

#[test]
fn racing_existing_destination_survives_publication_failure() {
    let dir = Directory::new();
    let doc = document();
    let v = proof(&doc);
    let path = dir.path("race.glb");
    let result = save_copy_with(&v, &doc, &path, &Cancellation::default(), |stage, _| {
        if stage == Stage::Ready {
            fs::write(&path, b"concurrent owner").map_err(io_error)?;
        }
        Ok(())
    });
    assert!(matches!(result,Err(e) if e.code==ErrorCode::Io));
    assert_eq!(fs::read(path).unwrap(), b"concurrent owner");
    assert_eq!(dir.stages(), 0);
}

#[test]
fn concurrent_copies_publish_exactly_once() {
    let dir = Directory::new();
    let doc = document();
    let v = proof(&doc);
    let path = dir.path("race.glb");
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let save = || {
            save_copy_with(&v, &doc, &path, &Cancellation::default(), |stage, _| {
                if stage == Stage::Ready {
                    barrier.wait();
                }
                Ok(())
            })
        };
        let first = scope.spawn(save);
        let second = scope.spawn(save);
        [first.join().unwrap(), second.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r,Err(e) if e.code==ErrorCode::Io))
            .count(),
        1
    );
    assert_eq!(fs::read(path).unwrap(), v.bytes().as_ref());
    assert_eq!(dir.stages(), 0);
}

#[test]
fn invalid_destinations_leave_no_staging() {
    let dir = Directory::new();
    let doc = document();
    let v = proof(&doc);
    for path in [
        PathBuf::from("relative.glb"),
        dir.path("wrong.txt"),
        dir.path("missing/copy.glb"),
    ] {
        assert!(
            matches!(save_copy(&v,&doc,&path,&Cancellation::default()),Err(e) if e.code==ErrorCode::Io)
        );
        assert_eq!(dir.stages(), 0);
    }
}

#[test]
fn cleanup_is_disarmed_after_removing_owned_staging() {
    let dir = Directory::new();
    let mut stage = Staging::create(&dir.0, &Cancellation::default()).unwrap();
    let path = stage.path.clone();
    stage.cleanup().unwrap();
    fs::write(&path, b"new owner").unwrap();
    drop(stage);
    assert_eq!(fs::read(path).unwrap(), b"new owner");
}

#[cfg(windows)]
#[test]
fn windows_staging_handle_denies_other_writers_and_deletion() {
    let dir = Directory::new();
    let doc = document();
    let v = proof(&doc);
    save_copy_with(
        &v,
        &doc,
        &dir.path("copy.GLB"),
        &Cancellation::default(),
        |stage, _| {
            if stage == Stage::Written {
                let staging = fs::read_dir(&dir.0)
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path();
                assert!(OpenOptions::new().write(true).open(&staging).is_err());
                assert!(fs::remove_file(&staging).is_err());
                assert_eq!(fs::read(staging).unwrap(), v.bytes().as_ref());
            }
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(dir.stages(), 0);
}

#[cfg(windows)]
#[test]
fn cleanup_failure_after_publication_returns_saved_with_warning() {
    let dir = Directory::new();
    let doc = document();
    let v = proof(&doc);
    let path = dir.path("saved.glb");
    let mut original_permissions = None;
    let saved = save_copy_with(&v, &doc, &path, &Cancellation::default(), |stage, file| {
        if stage == Stage::Ready {
            let mut permissions = file.metadata().map_err(io_error)?.permissions();
            original_permissions = Some(permissions.clone());
            permissions.set_readonly(true);
            file.set_permissions(permissions).map_err(io_error)?;
        }
        Ok(())
    })
    .unwrap();
    // Remove only the Windows readonly bit so this test can clean its owned files.
    fs::set_permissions(&path, original_permissions.unwrap()).unwrap();
    assert!(saved.cleanup_warning());
    assert_eq!(fs::read(&path).unwrap(), v.bytes().as_ref());
    assert_eq!(dir.stages(), 1);
}
