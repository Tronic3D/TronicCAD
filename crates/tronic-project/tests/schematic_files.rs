//! Schematics in project files, and reading files of older format versions.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use tempfile::tempdir;
use tronic_library::builtin;
use tronic_project::format::sha256_hex;
use tronic_project::{ProjectSession, SaveMode, FORMAT_VERSION};
use tronic_schematic::{EditCommand, LabelKind, Point};

const SHEET: u64 = 1;

#[test]
fn a_schematic_survives_save_and_reopen_exactly() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.tronic");
    let mut session = ProjectSession::create(&path, "A", "").unwrap();
    session.add_symbols(builtin());
    session
        .apply(EditCommand::PlaceComponent {
            sheet: SHEET,
            symbol: "device:resistor".to_owned(),
            at: Point::grid(0, 0),
            rotation: Default::default(),
            mirror: false,
        })
        .unwrap();
    session
        .apply(EditCommand::AddWires {
            sheet: SHEET,
            points: vec![Point::grid(0, 4), Point::grid(10, 4)],
        })
        .unwrap();
    session
        .apply(EditCommand::AddLabel {
            sheet: SHEET,
            at: Point::grid(10, 4),
            name: "信号 Größe".to_owned(),
            kind: LabelKind::Global,
        })
        .unwrap();
    session
        .apply(EditCommand::AddBlock {
            sheet: SHEET,
            at: Point::grid(20, 0),
            name: "Unterblatt".to_owned(),
        })
        .unwrap();
    assert!(session.is_dirty());
    session.save(SaveMode::IfUnchanged).unwrap();
    assert!(!session.is_dirty());

    let reopened = ProjectSession::open(&path).unwrap();
    assert_eq!(reopened.schematic(), session.schematic());
    assert_eq!(reopened.schematic().sheets.len(), 2);
}

#[test]
fn a_project_needs_no_library_to_be_read() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.tronic");
    let mut session = ProjectSession::create(&path, "A", "").unwrap();
    session.add_symbols(builtin().into_iter().filter(|s| s.id == "device:led"));
    session
        .apply(EditCommand::PlaceComponent {
            sheet: SHEET,
            symbol: "device:led".to_owned(),
            at: Point::default(),
            rotation: Default::default(),
            mirror: false,
        })
        .unwrap();
    session.save(SaveMode::IfUnchanged).unwrap();
    let reopened = ProjectSession::open(&path).unwrap();
    assert_eq!(reopened.schematic().symbols.len(), 1);
    assert!(reopened.schematic().symbols.contains_key("device:led"));
}

#[test]
fn a_version_1_file_opens_with_an_empty_schematic_and_saves_as_current_version() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("alt.tronic");
    let body = r#"{
  "id": "952f1c74-8ec6-4e37-8b62-462a9d4b0b7e",
  "name": "Altes Projekt",
  "description": "Format 1",
  "created_at": "2026-11-04T10:00:00Z",
  "modified_at": "2026-11-04T10:00:00Z"
}"#;
    let file = format!(
        "{{\n  \"format\": \"tronicproject\",\n  \"format_version\": 1,\n  \"checksum\": {{ \"algorithm\": \"sha-256\", \"value\": \"{}\" }},\n  \"project\": {body}\n}}\n",
        sha256_hex(body.as_bytes())
    );
    fs::write(&path, file).unwrap();

    let mut session = ProjectSession::open(&path).unwrap();
    assert_eq!(session.project().name, "Altes Projekt");
    assert_eq!(session.schematic().sheets.len(), 1);
    assert!(!session.is_dirty(), "reading alone changes nothing");

    session.save(SaveMode::IfUnchanged).unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.contains(&format!("\"format_version\": {FORMAT_VERSION}")));
    assert_eq!(
        ProjectSession::open(&path).unwrap().project().name,
        "Altes Projekt"
    );
    // The old version stays recoverable.
    assert!(fs::read_to_string(dir.path().join("alt.tronic.bak"))
        .unwrap()
        .contains("\"format_version\": 1"));
}

#[test]
fn undo_marks_the_project_as_changed() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.tronic");
    let mut session = ProjectSession::create(&path, "A", "").unwrap();
    session
        .apply(EditCommand::AddNoConnect {
            sheet: SHEET,
            at: Point::default(),
        })
        .unwrap();
    session.save(SaveMode::IfUnchanged).unwrap();
    session.undo().unwrap();
    assert!(session.is_dirty());
    assert!(session.schematic().sheets[0].no_connects.is_empty());
}

#[test]
fn a_rejected_edit_leaves_the_project_clean() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.tronic");
    let mut session = ProjectSession::create(&path, "A", "").unwrap();
    assert!(session
        .apply(EditCommand::RenameSheet {
            sheet: 999,
            name: "x".to_owned()
        })
        .is_err());
    assert!(!session.is_dirty());
}
