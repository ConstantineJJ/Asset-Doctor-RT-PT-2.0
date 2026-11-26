use super::*;
use std::fs;

fn in_owned_directory(run: impl FnOnce(&Path)) {
    // Tests run in parallel and can read the same clock value: a counter
    // keeps the names apart, and create_dir refuses a folder that exists.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    fs::create_dir_all(&root).unwrap();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = root.join(format!(
        "ad-replace-test-{}-{unique}-{n}",
        std::process::id()
    ));
    fs::create_dir(&dir).unwrap();
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
        assert_eq!(
            fs::read(dir.join("backup.glb.rollback")).unwrap(),
            b"staged"
        );
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

#[test]
fn extended_windows_paths_cover_drive_unc_and_verbatim_paths() {
    for (input, expected) in [
        (r"E:\a\..\b.glb", r"\\?\E:\b.glb"),
        (r"\\server\share\file.glb", r"\\?\UNC\server\share\file.glb"),
        (r"\\?\E:\file.glb", r"\\?\E:\file.glb"),
        (
            r"\\?\UNC\server\share\file.glb",
            r"\\?\UNC\server\share\file.glb",
        ),
    ] {
        let expected: Vec<_> = expected.encode_utf16().chain([0]).collect();
        assert_eq!(wide(Path::new(input)).unwrap(), expected);
    }
}

#[test]
fn publish_and_restore_support_paths_longer_than_max_path() {
    use std::io::Write;

    in_owned_directory(|dir| {
        let first = dir.join("a".repeat(120));
        let deep = first.join("b".repeat(120));
        fs::create_dir_all(&deep).unwrap();
        let target = deep.join("working.glb");
        let absolute = std::path::absolute(&target).unwrap();
        assert!(absolute.as_os_str().encode_wide().count() > 260);

        fs::write(&target, b"original").unwrap();
        let cancel = Cancellation::default();
        let mut staging = Staging::create(&deep, &cancel).unwrap();
        let file = staging.file.as_mut().unwrap();
        file.write_all(b"staged").unwrap();
        file.sync_all().unwrap();

        let backup = publish(&mut staging, &target, b"original", b"staged", &cancel).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"staged");
        assert_eq!(fs::read(&backup).unwrap(), b"original");
        fs::remove_file(&target).unwrap();
        restore_missing(&target, &backup).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original");

        fs::remove_file(&target).unwrap();
        fs::remove_dir(deep).unwrap();
        fs::remove_dir(first).unwrap();
    });
}
