//! Crash recovery against the real file system. A crash is simulated by
//! dropping the store, which releases its lock just as the system does when a
//! process dies.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};
use tronic_project::store::backup_path;
use tronic_project::{AutosaveStore, DiskState, ProjectError, ProjectSession, SaveMode};
use tronic_schematic::{EditCommand, Point};

struct Bench {
    _dir: TempDir,
    snapshots: PathBuf,
    project: PathBuf,
}

fn bench() -> Bench {
    let dir = tempdir().unwrap();
    Bench {
        snapshots: dir.path().join("autosave"),
        project: dir.path().join("a.tronic"),
        _dir: dir,
    }
}

fn edit(session: &mut ProjectSession) {
    session
        .apply(EditCommand::AddNoConnect {
            sheet: 1,
            at: Point::grid(3, 4),
        })
        .unwrap();
}

fn snapshot_files(directory: &Path) -> usize {
    fs::read_dir(directory)
        .unwrap()
        .flatten()
        .filter(|f| f.file_name().to_string_lossy().ends_with(".autosave"))
        .count()
}

/// Leaves a snapshot of an edited project behind, as a crash would.
fn crashed(b: &Bench) {
    let store = AutosaveStore::open(&b.snapshots).unwrap();
    let mut session = ProjectSession::create(&b.project, "Original", "").unwrap();
    edit(&mut session);
    session.update_metadata("Ungespeichert", "").unwrap();
    assert!(store.write_if_needed(&mut session).unwrap().is_some());
}

#[test]
fn a_clean_session_needs_no_snapshot() {
    let b = bench();
    let store = AutosaveStore::open(&b.snapshots).unwrap();
    let mut session = ProjectSession::create(&b.project, "A", "").unwrap();
    assert_eq!(store.write_if_needed(&mut session).unwrap(), None);
    assert_eq!(snapshot_files(&b.snapshots), 0);
}

#[test]
fn only_new_changes_cause_a_new_snapshot() {
    let b = bench();
    let store = AutosaveStore::open(&b.snapshots).unwrap();
    let mut session = ProjectSession::create(&b.project, "A", "").unwrap();
    edit(&mut session);
    assert!(store.write_if_needed(&mut session).unwrap().is_some());
    assert_eq!(store.write_if_needed(&mut session).unwrap(), None);
    session.undo().unwrap();
    assert!(store.write_if_needed(&mut session).unwrap().is_some());
    assert_eq!(snapshot_files(&b.snapshots), 1);
}

#[test]
fn a_snapshot_leaves_the_project_file_and_dirty_state_alone() {
    let b = bench();
    let store = AutosaveStore::open(&b.snapshots).unwrap();
    let mut session = ProjectSession::create(&b.project, "A", "").unwrap();
    let before = fs::read(&b.project).unwrap();
    edit(&mut session);
    store.write_if_needed(&mut session).unwrap();
    assert_eq!(fs::read(&b.project).unwrap(), before);
    assert!(session.is_dirty());
    assert!(!backup_path(&b.project).exists());
    let bak = fs::read_dir(&b.snapshots)
        .unwrap()
        .flatten()
        .filter(|f| f.file_name().to_string_lossy().ends_with(".bak"))
        .count();
    assert_eq!(bak, 0, "snapshots keep no backups of themselves");
}

#[test]
fn work_is_restored_after_a_crash() {
    let b = bench();
    crashed(&b);
    let store = AutosaveStore::open(&b.snapshots).unwrap();

    let entries = store.recoverable();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "Ungespeichert");
    assert_eq!(entries[0].path, b.project.display().to_string());
    assert_eq!(entries[0].disk_state, DiskState::Unchanged);

    let mut session = store.recover(&b.project).unwrap();
    assert!(session.is_dirty(), "restored work is not on disk yet");
    assert_eq!(session.project().name, "Ungespeichert");
    assert_eq!(session.schematic().sheets[0].no_connects.len(), 1);

    session.save(SaveMode::IfUnchanged).unwrap();
    let saved = ProjectSession::open(&b.project).unwrap();
    assert_eq!(saved.project().name, "Ungespeichert");
    assert_eq!(saved.schematic().sheets[0].no_connects.len(), 1);
}

#[test]
fn the_snapshot_of_a_running_application_is_not_offered() {
    let b = bench();
    let first = AutosaveStore::open(&b.snapshots).unwrap();
    let mut session = ProjectSession::create(&b.project, "A", "").unwrap();
    edit(&mut session);
    first.write(&mut session).unwrap();

    let second = AutosaveStore::open(&b.snapshots).unwrap();
    assert!(second.recoverable().is_empty());
    assert_eq!(second.recoverable_for(&b.project), None);
    second.discard(&b.project).unwrap();
    assert_eq!(snapshot_files(&b.snapshots), 1, "live work must survive");

    drop(first);
    assert_eq!(second.recoverable().len(), 1);
}

#[test]
fn a_recovered_snapshot_belongs_to_the_new_application() {
    let b = bench();
    crashed(&b);
    let second = AutosaveStore::open(&b.snapshots).unwrap();
    let session = second.recover(&b.project).unwrap();
    let third = AutosaveStore::open(&b.snapshots).unwrap();
    assert!(third.recoverable().is_empty());

    // A second crash loses nothing.
    drop(session);
    drop(second);
    assert_eq!(third.recoverable().len(), 1);
}

#[test]
fn an_external_change_is_reported_and_still_blocks_a_plain_save() {
    let b = bench();
    crashed(&b);
    let mut outside = ProjectSession::open(&b.project).unwrap();
    outside.update_metadata("Von außen", "").unwrap();
    outside.save(SaveMode::IfUnchanged).unwrap();
    let their_file = fs::read(&b.project).unwrap();

    let store = AutosaveStore::open(&b.snapshots).unwrap();
    assert_eq!(store.recoverable()[0].disk_state, DiskState::Changed);
    let mut session = store.recover(&b.project).unwrap();
    assert!(matches!(
        session.save(SaveMode::IfUnchanged),
        Err(ProjectError::Conflict { .. })
    ));
    assert_eq!(fs::read(&b.project).unwrap(), their_file);
}

#[test]
fn a_deleted_project_file_can_be_restored() {
    let b = bench();
    crashed(&b);
    fs::remove_file(&b.project).unwrap();
    let store = AutosaveStore::open(&b.snapshots).unwrap();
    assert_eq!(store.recoverable()[0].disk_state, DiskState::Missing);
    let mut session = store.recover(&b.project).unwrap();
    session.save(SaveMode::IfUnchanged).unwrap();
    assert_eq!(
        ProjectSession::open(&b.project).unwrap().project().name,
        "Ungespeichert"
    );
}

#[test]
fn discarding_removes_the_snapshot_and_keeps_the_file() {
    let b = bench();
    crashed(&b);
    let store = AutosaveStore::open(&b.snapshots).unwrap();
    store.discard(&b.project).unwrap();
    assert!(store.recoverable().is_empty());
    assert_eq!(
        ProjectSession::open(&b.project).unwrap().project().name,
        "Original"
    );
}

#[test]
fn remove_own_only_touches_own_snapshots() {
    let b = bench();
    crashed(&b);
    let store = AutosaveStore::open(&b.snapshots).unwrap();
    store.remove_own(&b.project).unwrap();
    assert_eq!(
        store.recoverable().len(),
        1,
        "belongs to the ended application"
    );

    let mut session = store.recover(&b.project).unwrap();
    store.write(&mut session).unwrap();
    store.remove_own(session.path()).unwrap();
    assert_eq!(snapshot_files(&b.snapshots), 0);
}

#[test]
fn damaged_snapshots_are_skipped_and_kept() {
    let b = bench();
    crashed(&b);
    let file = fs::read_dir(&b.snapshots)
        .unwrap()
        .flatten()
        .find(|f| f.file_name().to_string_lossy().ends_with(".autosave"))
        .unwrap()
        .path();
    let good = fs::read_to_string(&file).unwrap();

    fs::write(&file, &good[..good.len() / 2]).unwrap();
    let store = AutosaveStore::open(&b.snapshots).unwrap();
    assert!(store.recoverable().is_empty());
    assert!(file.exists());
    assert!(store.recover(&b.project).is_err());

    // Valid JSON, but the project inside fails its own checksum.
    fs::write(&file, good.replace("Ungespeichert", "Manipuliert")).unwrap();
    assert!(store.recoverable().is_empty());
}

#[test]
fn several_projects_are_listed_with_their_paths_kept_apart() {
    let b = bench();
    crashed(&b);
    {
        let store = AutosaveStore::open(&b.snapshots).unwrap();
        let other = b.project.with_file_name("Größe 电路.tronic");
        let mut session = ProjectSession::create(&other, "Zweites", "").unwrap();
        edit(&mut session);
        store.write(&mut session).unwrap();
    }
    let store = AutosaveStore::open(&b.snapshots).unwrap();
    let mut names: Vec<String> = store.recoverable().into_iter().map(|e| e.name).collect();
    names.sort();
    assert_eq!(names, ["Ungespeichert", "Zweites"]);
}

#[test]
fn lock_files_of_ended_applications_are_cleaned_up() {
    let b = bench();
    drop(AutosaveStore::open(&b.snapshots).unwrap());
    let _store = AutosaveStore::open(&b.snapshots).unwrap();
    let locks = fs::read_dir(&b.snapshots)
        .unwrap()
        .flatten()
        .filter(|f| f.file_name().to_string_lossy().ends_with(".lock"))
        .count();
    assert_eq!(locks, 1);
}
