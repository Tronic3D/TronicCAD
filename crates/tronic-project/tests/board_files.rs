//! The way from schematic to board, with the real library, through a file.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use tempfile::tempdir;
use tronic_library::{builtin, find_footprint};
use tronic_pcb::{compare_with_netlist, PcbCommand, MM};
use tronic_project::format::sha256_hex;
use tronic_project::{ProjectSession, SaveMode};
use tronic_schematic::{EditCommand, Point};

const SHEET: u64 = 1;

fn place(session: &mut ProjectSession, symbol: &str, x: i64, y: i64) {
    session
        .apply(EditCommand::PlaceComponent {
            sheet: SHEET,
            symbol: symbol.to_owned(),
            at: Point::grid(x, y),
            rotation: Default::default(),
            mirror: false,
        })
        .unwrap();
}

fn led_circuit(path: &std::path::Path) -> ProjectSession {
    let mut session = ProjectSession::create(path, "LED", "").unwrap();
    session.add_symbols(builtin());
    place(&mut session, "device:resistor", 0, 0);
    place(&mut session, "device:led", 0, 10);
    place(&mut session, "supply:5v", 0, -8);
    place(&mut session, "supply:gnd", 0, 18);
    for points in [
        [Point::grid(0, -8), Point::grid(0, -4)],
        [Point::grid(0, 4), Point::grid(0, 6)],
        [Point::grid(0, 14), Point::grid(0, 18)],
    ] {
        session
            .apply(EditCommand::AddWires {
                sheet: SHEET,
                points: points.to_vec(),
            })
            .unwrap();
    }
    session
}

#[test]
fn the_board_matches_the_netlist_of_the_schematic_without_deviation() {
    let dir = tempdir().unwrap();
    let mut session = led_circuit(&dir.path().join("a.tronic"));
    let report = session.sync_board(&find_footprint);
    assert_eq!(report.added, ["D1", "R1"]);
    assert!(report.no_footprint.is_empty());
    assert!(report.missing_pads.is_empty());
    assert_eq!(
        compare_with_netlist(session.board(), &session.netlist()),
        []
    );

    let pads = session.board().resolved_pads();
    let net = |reference: &str, number: &str| {
        pads.iter()
            .find(|pad| pad.reference == reference && pad.number == number)
            .and_then(|pad| pad.net.clone())
            .unwrap()
    };
    assert_eq!(net("R1", "1"), "+5V");
    assert_eq!(net("R1", "2"), net("D1", "1"));
    assert_eq!(net("D1", "2"), "GND");
}

#[test]
fn a_changed_schematic_shows_up_until_synced() {
    let dir = tempdir().unwrap();
    let mut session = led_circuit(&dir.path().join("a.tronic"));
    session.sync_board(&find_footprint);
    place(&mut session, "device:capacitor", 20, 0);
    assert!(!compare_with_netlist(session.board(), &session.netlist()).is_empty());
    let report = session.sync_board(&find_footprint);
    assert_eq!(report.added, ["C1"]);
    assert_eq!(
        compare_with_netlist(session.board(), &session.netlist()),
        []
    );
}

#[test]
fn a_footprint_chosen_in_the_schematic_reaches_the_board() {
    let dir = tempdir().unwrap();
    let mut session = led_circuit(&dir.path().join("a.tronic"));
    let id = session.schematic().sheets[0]
        .components
        .iter()
        .find(|c| c.reference == "R1")
        .unwrap()
        .id;
    session
        .apply(EditCommand::SetFootprint {
            sheet: SHEET,
            id,
            footprint: Some("chip:1206".to_owned()),
        })
        .unwrap();
    session.sync_board(&find_footprint);
    let r1 = session
        .board()
        .footprints
        .iter()
        .find(|f| f.reference == "R1")
        .unwrap();
    assert_eq!(r1.footprint, "chip:1206");
}

#[test]
fn the_board_survives_save_and_reopen_exactly() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.tronic");
    let mut session = led_circuit(&path);
    session.sync_board(&find_footprint);
    session
        .apply_pcb(PcbCommand::SetOutline {
            points: vec![
                Point::new(0, 0),
                Point::new(30 * MM, 0),
                Point::new(30 * MM, 20 * MM),
                Point::new(0, 20 * MM),
            ],
        })
        .unwrap();
    session
        .apply_pcb(PcbCommand::AddTracks {
            net: "GND".to_owned(),
            layer: 1,
            points: vec![Point::new(MM, MM), Point::new(5 * MM, MM)],
            width: 300_000,
        })
        .unwrap();
    session.save(SaveMode::IfUnchanged).unwrap();
    let reopened = ProjectSession::open(&path).unwrap();
    assert_eq!(reopened.board(), session.board());
    assert_eq!(reopened.schematic(), session.schematic());
}

#[test]
fn a_version_2_file_opens_with_an_empty_board() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("alt.tronic");
    let body = r#"{
  "id": "952f1c74-8ec6-4e37-8b62-462a9d4b0b7e",
  "name": "Format 2",
  "description": "",
  "created_at": "2026-11-04T10:00:00Z",
  "modified_at": "2026-11-04T10:00:00Z",
  "schematic": { "next_id": 2, "sheets": [{ "id": 1, "name": "1", "parent": null, "components": [], "wires": [], "buses": [], "labels": [], "no_connects": [], "junctions": [], "instances": [] }], "symbols": {} }
}"#;
    let file = format!(
        "{{\n  \"format\": \"tronicproject\",\n  \"format_version\": 2,\n  \"checksum\": {{ \"algorithm\": \"sha-256\", \"value\": \"{}\" }},\n  \"project\": {body}\n}}\n",
        sha256_hex(body.as_bytes())
    );
    fs::write(&path, file).unwrap();
    let session = ProjectSession::open(&path).unwrap();
    assert_eq!(session.project().name, "Format 2");
    assert!(session.board().footprints.is_empty());
    assert!(!session.is_dirty());
}

#[test]
fn a_board_edit_counts_as_an_unsaved_change_and_can_be_undone() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.tronic");
    let mut session = ProjectSession::create(&path, "A", "").unwrap();
    session
        .apply_pcb(PcbCommand::SetLayerCount { layers: 4 })
        .unwrap();
    assert!(session.is_dirty());
    assert!(session.needs_autosave());
    session.undo_pcb().unwrap();
    assert_eq!(session.board().copper_layers, 2);
}
