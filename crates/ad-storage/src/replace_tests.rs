use super::*;
use std::fs;

fn in_owned_directory(run: impl FnOnce(&Path)) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = root.join(format!("ad-replace-test-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    run(&dir);
    for entry in fs::read_dir(&dir).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::remove_dir(&dir).unwrap();
}

#[test]
fn atomic_replace_audits_the_actual_displaced_file() {
    in_owned_directory(|dir| {
        let target = dir.join("working.glb");
        let candidate = dir.join("candidate.glb");
        let backup = dir.join("backup.glb");
        fs::write(&target, b"original").unwrap();
        fs::write(&candidate, b"staged").unwrap();
        let read = verify_expected(&target, b"original", &Cancellation::default()).unwrap();
        assert!(OpenOptions::new().write(true).open(&target).is_err());
        replace_file(&target, &candidate, &backup).unwrap();
        drop(read);
        assert_eq!(
            audit_backup(&target, backup.clone(), b"original", b"staged").unwrap(),
            backup
        );
        assert_eq!(fs::read(&target).unwrap(), b"staged");
        assert_eq!(fs::read(backup).unwrap(), b"original");
    });
}

#[test]
fn audit_restores_an_external_replacement_after_the_prior_check() {
    in_owned_directory(|dir| {
        let target = dir.join("working.glb");
        let candidate = dir.join("candidate.glb");
        let external = dir.join("external.glb");
        let backup = dir.join("backup.glb");
        fs::write(&target, b"original").unwrap();
        fs::write(&candidate, b"staged").unwrap();
        fs::write(&external, b"external").unwrap();
        let read = verify_expected(&target, b"original", &Cancellation::default()).unwrap();
        replace_file(&target, &external, &dir.join("external-backup.glb")).unwrap();
        replace_file(&target, &candidate, &backup).unwrap();
        drop(read);
        let err = audit_backup(&target, backup, b"original", b"staged").unwrap_err();
        assert_eq!(err.code, ErrorCode::Conflict);
        assert_eq!(fs::read(&target).unwrap(), b"external");
        assert_eq!(fs::read(dir.join("backup.glb.rollback")).unwrap(), b"staged");
    });
}

#[test]
fn a_rollback_preserves_a_later_save_and_can_recover_a_missing_working_path() {
    in_owned_directory(|dir| {
        let target = dir.join("working.glb");
        let backup = dir.join("backup.glb");
        fs::write(&target, b"later").unwrap();
        fs::write(&backup, b"external").unwrap();
        let err = audit_backup(&target, backup.clone(), b"original", b"staged").unwrap_err();
        assert_eq!(err.code, ErrorCode::Conflict);
        assert_eq!(fs::read(&target).unwrap(), b"later");
        assert_eq!(fs::read(&backup).unwrap(), b"external");

        fs::remove_file(&target).unwrap();
        restore_missing(&target, &backup).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"external");
    });
}

#[test]
fn missing_path_recovers_only_a_unique_matching_backup() {
    in_owned_directory(|dir| {
        let target = dir.join("working.glb");
        let backup = backup_path(&target, &dir.join("first.tmp"));
        let other = backup_path(&target, &dir.join("second.tmp"));
        let c = Cancellation::default();
        fs::write(&backup, b"original").unwrap();
        fs::write(&other, b"original").unwrap();
        let err = recover_missing(&target, b"original", &c).unwrap_err();
        assert_eq!(err.code, ErrorCode::Conflict);
        assert!(!target.exists());

        fs::write(&other, b"external").unwrap();
        recover_missing(&target, b"original", &c).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(fs::read(&other).unwrap(), b"external");
    });
}
#[test]
fn publish_consumes_verified_staging_and_retains_the_original_backup() {
    use std::io::Write;

    in_owned_directory(|dir| {
        let target = dir.join("working.glb");
        fs::write(&target, b"original").unwrap();
        let c = Cancellation::default();
        let mut staging = Staging::create(dir, &c).unwrap();
        let file = staging.file.as_mut().unwrap();
        file.write_all(b"staged").unwrap();
        file.sync_all().unwrap();
        let path = staging.path.clone();

        let backup = publish(&mut staging, &target, b"original", b"staged", &c).unwrap();
        drop(staging);
        assert!(!path.exists());
        assert_eq!(fs::read(&target).unwrap(), b"staged");
        assert_eq!(fs::read(&backup).unwrap(), b"original");
    });
}