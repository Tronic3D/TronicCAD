//! Integration tests against the real file system.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use tempfile::tempdir;
use tronic_project::store::backup_path;
use tronic_project::{ProjectError, ProjectSession, SaveMode};

#[test]
fn create_save_and_reopen_preserve_the_project() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("Netzteil.tronic");

    let mut session = ProjectSession::create(&path, "Netzteil", "Erste Fassung").unwrap();
    assert!(!session.is_dirty());
    let id = session.project().id;
    let created = session.project().created_at;

    session
        .update_metadata("Netzteil 电源", "Zweite Fassung")
        .unwrap();
    assert!(session.is_dirty());
    session.save(SaveMode::IfUnchanged).unwrap();
    assert!(!session.is_dirty());

    let reopened = ProjectSession::open(&path).unwrap();
    assert_eq!(reopened.project().id, id);
    assert_eq!(reopened.project().name, "Netzteil 电源");
    assert_eq!(reopened.project().description, "Zweite Fassung");
    assert_eq!(reopened.project().created_at, created);
    assert!(reopened.project().modified_at >= created);
}

#[test]
fn create_appends_the_extension() {
    let dir = tempdir().unwrap();
    let session = ProjectSession::create(&dir.path().join("Projekt"), "P", "").unwrap();
    assert_eq!(session.path(), dir.path().join("Projekt.tronic"));
    assert!(session.path().is_file());
}

#[test]
fn create_refuses_to_replace_an_existing_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.tronic");
    fs::write(&path, b"keep me").unwrap();
    assert!(matches!(
        ProjectSession::create(&path, "A", ""),
        Err(ProjectError::AlreadyExists { .. })
    ));
    assert_eq!(fs::read(&path).unwrap(), b"keep me");
}

#[test]
fn create_with_invalid_name_writes_nothing() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.tronic");
    assert!(matches!(
        ProjectSession::create(&path, "   ", ""),
        Err(ProjectError::Validation(_))
    ));
    assert!(!path.exists());
}

#[test]
fn external_change_is_detected_and_not_overwritten() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.tronic");

    let mut mine = ProjectSession::create(&path, "Original", "").unwrap();
    let mut theirs = ProjectSession::open(&path).unwrap();
    theirs.update_metadata("Ihre Fassung", "").unwrap();
    theirs.save(SaveMode::IfUnchanged).unwrap();

    mine.update_metadata("Meine Fassung", "").unwrap();
    assert!(matches!(
        mine.save(SaveMode::IfUnchanged),
        Err(ProjectError::Conflict { .. })
    ));
    assert!(mine.is_dirty(), "unsaved work must stay marked");
    assert_eq!(
        ProjectSession::open(&path).unwrap().project().name,
        "Ihre Fassung"
    );

    mine.save(SaveMode::Overwrite).unwrap();
    assert_eq!(
        ProjectSession::open(&path).unwrap().project().name,
        "Meine Fassung"
    );
    // The overwritten version stays recoverable.
    assert_eq!(
        ProjectSession::open(&backup_path(&path))
            .unwrap()
            .project()
            .name,
        "Ihre Fassung"
    );
}

#[test]
fn save_recreates_a_file_deleted_externally() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.tronic");
    let mut session = ProjectSession::create(&path, "A", "").unwrap();
    fs::remove_file(&path).unwrap();
    session.save(SaveMode::IfUnchanged).unwrap();
    assert!(ProjectSession::open(&path).is_ok());
}

#[test]
fn save_keeps_previous_version_as_backup() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.tronic");
    let mut session = ProjectSession::create(&path, "Eins", "").unwrap();
    assert!(!backup_path(&path).exists());

    session.update_metadata("Zwei", "").unwrap();
    session.save(SaveMode::IfUnchanged).unwrap();
    assert_eq!(
        ProjectSession::open(&backup_path(&path))
            .unwrap()
            .project()
            .name,
        "Eins"
    );
}

#[test]
fn save_leaves_no_temporary_files() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.tronic");
    let mut session = ProjectSession::create(&path, "A", "").unwrap();
    session.update_metadata("B", "").unwrap();
    session.save(SaveMode::IfUnchanged).unwrap();
    let mut names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["a.tronic", "a.tronic.bak"]);
}

#[test]
fn failed_save_keeps_the_work_in_memory() {
    let dir = tempdir().unwrap();
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    let path = sub.join("a.tronic");
    let mut session = ProjectSession::create(&path, "A", "").unwrap();
    session.update_metadata("B", "").unwrap();
    let before = session.project().modified_at;

    fs::remove_dir_all(&sub).unwrap();
    assert!(matches!(
        session.save(SaveMode::IfUnchanged),
        Err(ProjectError::Io { .. })
    ));
    assert!(session.is_dirty());
    assert_eq!(session.project().name, "B");
    assert_eq!(session.project().modified_at, before);
}

#[test]
fn save_as_switches_the_session_to_the_new_file() {
    let dir = tempdir().unwrap();
    let first = dir.path().join("a.tronic");
    let second = dir.path().join("b");
    let mut session = ProjectSession::create(&first, "A", "").unwrap();
    session.update_metadata("B", "").unwrap();
    session.save_as(&second).unwrap();

    assert_eq!(session.path(), dir.path().join("b.tronic"));
    assert!(!session.is_dirty());
    assert_eq!(ProjectSession::open(&first).unwrap().project().name, "A");
    assert_eq!(
        ProjectSession::open(session.path()).unwrap().project().name,
        "B"
    );
}

#[test]
fn open_reports_missing_and_corrupted_files() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("missing.tronic");
    assert!(matches!(
        ProjectSession::open(&missing),
        Err(ProjectError::Io { .. })
    ));

    let path = dir.path().join("a.tronic");
    ProjectSession::create(&path, "Original", "").unwrap();
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replace("Original", "Verändert")).unwrap();
    assert!(matches!(
        ProjectSession::open(&path),
        Err(ProjectError::ChecksumMismatch { .. })
    ));

    fs::write(&path, &text[..text.len() / 2]).unwrap();
    assert!(matches!(
        ProjectSession::open(&path),
        Err(ProjectError::NotAProject { .. })
    ));
}

#[test]
fn paths_with_non_ascii_characters_work() {
    let dir = tempdir().unwrap();
    let sub = dir.path().join("Übersicht 电路 é");
    fs::create_dir(&sub).unwrap();
    let path = sub.join("Größe 电源.tronic");
    ProjectSession::create(&path, "X", "").unwrap();
    assert!(ProjectSession::open(&path).is_ok());
}
