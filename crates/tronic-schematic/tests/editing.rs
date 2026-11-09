//! Behaviour of commands, connectivity and the netlist on small circuits.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeSet;
use tronic_schematic::*;

const SHEET: ElementId = 1;

fn two_pin() -> SymbolDef {
    let pin = |number: &str, at: Point, kind| PinDef {
        number: number.to_owned(),
        name: number.to_owned(),
        kind,
        at,
        direction: PinDirection::Down,
        length: GRID,
        hide_name: true,
        hide_number: false,
    };
    SymbolDef {
        id: "t:r".to_owned(),
        name: "R".to_owned(),
        category: "resistors".to_owned(),
        kind: SymbolKind::Part,
        reference_prefix: "R".to_owned(),
        default_value: "1k".to_owned(),
        description: String::new(),
        graphics: Vec::new(),
        pins: vec![
            pin("1", Point::grid(0, -2), PinKind::Passive),
            pin("2", Point::grid(0, 2), PinKind::Passive),
        ],
        part: PartInfo::default(),
        provenance: Provenance {
            source: "test".to_owned(),
            author: "test".to_owned(),
            license: "test".to_owned(),
            reviewed: false,
        },
    }
}

fn supply() -> SymbolDef {
    let mut symbol = two_pin();
    symbol.id = "t:vcc".to_owned();
    symbol.kind = SymbolKind::SupplyPort;
    symbol.reference_prefix = "#PWR".to_owned();
    symbol.default_value = "VCC".to_owned();
    symbol.pins.truncate(1);
    symbol.pins[0].at = Point::default();
    symbol.pins[0].kind = PinKind::SupplyPort;
    symbol
}

fn editor() -> Editor {
    let mut editor = Editor::default();
    editor.add_symbols([two_pin(), supply()]);
    editor
}

fn place(editor: &mut Editor, sheet: ElementId, x: i64, y: i64) -> ElementId {
    editor
        .apply(EditCommand::PlaceComponent {
            sheet,
            symbol: "t:r".to_owned(),
            at: Point::grid(x, y),
            rotation: Rotation::R0,
            mirror: false,
        })
        .unwrap()
        .created[0]
}

fn wires(editor: &mut Editor, sheet: ElementId, points: &[(i64, i64)]) {
    editor
        .apply(EditCommand::AddWires {
            sheet,
            points: points.iter().map(|&(x, y)| Point::grid(x, y)).collect(),
        })
        .unwrap();
}

fn nets(editor: &Editor) -> BTreeSet<BTreeSet<String>> {
    let connectivity = Connectivity::analyze(editor.doc());
    Netlist::build(editor.doc(), &connectivity)
        .nets
        .into_iter()
        .map(|net| {
            net.nodes
                .into_iter()
                .map(|node| format!("{}.{}", node.reference, node.pin))
                .collect()
        })
        .collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|&item| item.to_owned()).collect()
}

fn connected(editor: &Editor, a: &str, b: &str) -> bool {
    nets(editor)
        .iter()
        .any(|net| net.contains(a) && net.contains(b))
}

#[test]
fn references_count_up_and_reuse_gaps() {
    let mut e = editor();
    let first = place(&mut e, SHEET, 0, 0);
    place(&mut e, SHEET, 5, 0);
    e.apply(EditCommand::Delete {
        sheet: SHEET,
        ids: vec![first],
    })
    .unwrap();
    place(&mut e, SHEET, 10, 0);
    let refs: BTreeSet<_> = e.doc().sheets[0]
        .components
        .iter()
        .map(|c| c.reference.clone())
        .collect();
    assert_eq!(refs, set(&["R1", "R2"]));
}

#[test]
fn a_wire_connects_pins_at_its_ends() {
    let mut e = editor();
    place(&mut e, SHEET, 0, 0);
    place(&mut e, SHEET, 6, 0);
    wires(&mut e, SHEET, &[(0, 2), (6, 2)]);
    assert!(connected(&e, "R1.2", "R2.2"));
    assert!(!connected(&e, "R1.1", "R2.1"));
}

#[test]
fn crossing_wires_stay_apart_without_a_junction() {
    let mut e = editor();
    let wire_h = |e: &mut Editor| wires(e, SHEET, &[(0, 2), (10, 2)]);
    place(&mut e, SHEET, 0, 0); // R1.2 at (0,2)
    place(&mut e, SHEET, 10, 0); // R2.2 at (10,2)
    place(&mut e, SHEET, 5, -5); // R3.2 at (5,-3)
    place(&mut e, SHEET, 5, 9); // R4.1 at (5,7)
    wire_h(&mut e);
    wires(&mut e, SHEET, &[(5, -3), (5, 7)]);
    assert!(connected(&e, "R1.2", "R2.2"));
    assert!(connected(&e, "R3.2", "R4.1"));
    assert!(!connected(&e, "R1.2", "R3.2"));

    e.apply(EditCommand::AddJunction {
        sheet: SHEET,
        at: Point::grid(5, 2),
    })
    .unwrap();
    assert!(connected(&e, "R1.2", "R3.2"));
}

#[test]
fn a_wire_ending_on_another_connects_as_a_t() {
    let mut e = editor();
    place(&mut e, SHEET, 0, 0);
    place(&mut e, SHEET, 10, 0);
    place(&mut e, SHEET, 5, 9);
    wires(&mut e, SHEET, &[(0, 2), (10, 2)]);
    wires(&mut e, SHEET, &[(5, 2), (5, 7)]);
    assert!(connected(&e, "R1.2", "R3.1"));
    let junctions = connectivity::junction_points(e.doc(), &e.doc().sheets[0]);
    assert_eq!(junctions, vec![Point::grid(5, 2)]);
}

#[test]
fn local_labels_connect_on_their_sheet_only() {
    let mut e = editor();
    place(&mut e, SHEET, 0, 0);
    place(&mut e, SHEET, 10, 0);
    let other = e
        .apply(EditCommand::AddSheet {
            name: "2".to_owned(),
        })
        .unwrap()
        .created[0];
    place(&mut e, other, 0, 0);
    for (sheet, x) in [(SHEET, 0), (SHEET, 10), (other, 0)] {
        e.apply(EditCommand::AddLabel {
            sheet,
            at: Point::grid(x, 2),
            name: "SIG".to_owned(),
            kind: LabelKind::Local,
        })
        .unwrap();
    }
    assert!(connected(&e, "R1.2", "R2.2"));
    assert!(!connected(&e, "R1.2", "R3.2"));
}

#[test]
fn global_labels_and_supplies_connect_across_sheets_and_cross_reference() {
    let mut e = editor();
    place(&mut e, SHEET, 0, 0);
    let other = e
        .apply(EditCommand::AddSheet {
            name: "2".to_owned(),
        })
        .unwrap()
        .created[0];
    place(&mut e, other, 0, 0);
    for sheet in [SHEET, other] {
        e.apply(EditCommand::AddLabel {
            sheet,
            at: Point::grid(0, 2),
            name: "CLK".to_owned(),
            kind: LabelKind::Global,
        })
        .unwrap();
        e.apply(EditCommand::PlaceComponent {
            sheet,
            symbol: "t:vcc".to_owned(),
            at: Point::grid(0, -2),
            rotation: Rotation::R0,
            mirror: false,
        })
        .unwrap();
    }
    assert!(connected(&e, "R1.2", "R2.2"));
    assert!(connected(&e, "R1.1", "R2.1"));

    let connectivity = Connectivity::analyze(e.doc());
    let clk = connectivity
        .nets
        .iter()
        .find(|net| net.name == "CLK")
        .unwrap();
    assert_eq!(clk.sheets, BTreeSet::from([SHEET, other]));
    assert!(connectivity.nets.iter().any(|net| net.name == "VCC"));
}

#[test]
fn a_supply_and_a_global_label_of_one_name_are_one_net() {
    let mut e = editor();
    place(&mut e, SHEET, 0, 0);
    e.apply(EditCommand::AddLabel {
        sheet: SHEET,
        at: Point::grid(0, -2),
        name: "VCC".to_owned(),
        kind: LabelKind::Global,
    })
    .unwrap();
    e.apply(EditCommand::PlaceComponent {
        sheet: SHEET,
        symbol: "t:vcc".to_owned(),
        at: Point::grid(0, 2),
        rotation: Rotation::R0,
        mirror: false,
    })
    .unwrap();
    assert!(connected(&e, "R1.1", "R1.2"));
}

#[test]
fn hierarchical_labels_connect_through_the_block_port() {
    let mut e = editor();
    place(&mut e, SHEET, 0, 0); // R1.2 at (0,2)
    e.apply(EditCommand::AddBlock {
        sheet: SHEET,
        at: Point::grid(10, 0),
        name: "Sub".to_owned(),
    })
    .unwrap();
    let child = e.doc().sheets[0].instances[0].sheet;
    place(&mut e, child, 0, 0);
    e.apply(EditCommand::AddLabel {
        sheet: child,
        at: Point::grid(0, 2),
        name: "IN".to_owned(),
        kind: LabelKind::Hierarchical,
    })
    .unwrap();
    let block = e.doc().sheets[0].instances[0].clone();
    assert_eq!(block.ports.len(), 1);
    let port = block.at.offset(block.ports[0].at);
    assert_eq!(port, Point::grid(10, 2));
    wires(&mut e, SHEET, &[(0, 2), (10, 2)]);
    assert!(connected(&e, "R1.2", "R2.2"));

    // The port of a second label does not disturb the first one.
    e.apply(EditCommand::AddLabel {
        sheet: child,
        at: Point::grid(0, -2),
        name: "AAA".to_owned(),
        kind: LabelKind::Hierarchical,
    })
    .unwrap();
    let block = e.doc().sheets[0].instances[0].clone();
    let first = block.ports.iter().find(|p| p.name == "IN").unwrap();
    assert_eq!(block.at.offset(first.at), Point::grid(10, 2));
    assert!(connected(&e, "R1.2", "R2.2"));

    // Local names of the higher sheet name the net.
    e.apply(EditCommand::AddLabel {
        sheet: SHEET,
        at: Point::grid(0, 2),
        name: "TOP".to_owned(),
        kind: LabelKind::Local,
    })
    .unwrap();
    let connectivity = Connectivity::analyze(e.doc());
    let net = connectivity
        .net_of_pin(e.doc().sheets[0].components[0].id, 1)
        .unwrap();
    assert_eq!(net.name, "/1/TOP");
}

#[test]
fn deleting_a_block_removes_its_sheets_and_undo_restores_them() {
    let mut e = editor();
    let block = e
        .apply(EditCommand::AddBlock {
            sheet: SHEET,
            at: Point::grid(0, 0),
            name: "Sub".to_owned(),
        })
        .unwrap()
        .created[0];
    let child = e.doc().sheets[0].instances[0].sheet;
    e.apply(EditCommand::AddBlock {
        sheet: child,
        at: Point::grid(0, 0),
        name: "SubSub".to_owned(),
    })
    .unwrap();
    assert_eq!(e.doc().sheets.len(), 3);
    e.apply(EditCommand::Delete {
        sheet: SHEET,
        ids: vec![block],
    })
    .unwrap();
    assert_eq!(e.doc().sheets.len(), 1);
    e.doc().validate().unwrap();
    e.undo().unwrap();
    assert_eq!(e.doc().sheets.len(), 3);
    e.doc().validate().unwrap();
}

#[test]
fn sheets_cannot_be_deleted_into_an_invalid_state() {
    let mut e = editor();
    assert_eq!(
        e.apply(EditCommand::DeleteSheet { sheet: SHEET }),
        Err(EditError::LastSheet)
    );
    e.apply(EditCommand::AddBlock {
        sheet: SHEET,
        at: Point::default(),
        name: "Sub".to_owned(),
    })
    .unwrap();
    let child = e.doc().sheets[0].instances[0].sheet;
    assert_eq!(
        e.apply(EditCommand::DeleteSheet { sheet: child }),
        Err(EditError::SheetHasBlock)
    );
    assert_eq!(
        e.apply(EditCommand::AddSheet {
            name: "Sub".to_owned()
        }),
        Err(EditError::DuplicateSheetName)
    );
}

#[test]
fn moving_a_part_keeps_its_connections() {
    let mut e = editor();
    let r1 = place(&mut e, SHEET, 0, 0);
    place(&mut e, SHEET, 10, 0);
    wires(&mut e, SHEET, &[(0, 2), (10, 2)]);
    e.apply(EditCommand::Move {
        sheet: SHEET,
        ids: vec![r1],
        delta: Point::grid(-3, 4),
    })
    .unwrap();
    assert!(connected(&e, "R1.2", "R2.2"));
    assert_eq!(e.doc().sheets[0].components[0].at, Point::grid(-3, 4));
}

#[test]
fn rotating_and_mirroring_keep_connections_to_unselected_parts() {
    let mut e = editor();
    let r1 = place(&mut e, SHEET, 0, 0);
    place(&mut e, SHEET, 10, 0);
    wires(&mut e, SHEET, &[(0, 2), (10, 2)]);
    e.apply(EditCommand::Rotate {
        sheet: SHEET,
        ids: vec![r1],
    })
    .unwrap();
    assert!(connected(&e, "R1.2", "R2.2"));
    assert_eq!(e.doc().sheets[0].components[0].rotation, Rotation::R90);
    e.apply(EditCommand::Mirror {
        sheet: SHEET,
        ids: vec![r1],
    })
    .unwrap();
    assert!(connected(&e, "R1.2", "R2.2"));
    assert!(e.doc().sheets[0].components[0].mirror);
}

#[test]
fn moving_everything_adds_no_wires() {
    let mut e = editor();
    let r1 = place(&mut e, SHEET, 0, 0);
    let r2 = place(&mut e, SHEET, 10, 0);
    wires(&mut e, SHEET, &[(0, 2), (10, 2)]);
    let wire = e.doc().sheets[0].wires[0].id;
    e.apply(EditCommand::Move {
        sheet: SHEET,
        ids: vec![r1, r2, wire],
        delta: Point::grid(1, 1),
    })
    .unwrap();
    assert_eq!(e.doc().sheets[0].wires.len(), 1);
    assert!(connected(&e, "R1.2", "R2.2"));
}

#[test]
fn paste_assigns_new_ids_and_references() {
    let mut e = editor();
    let r1 = place(&mut e, SHEET, 0, 0);
    let original = e.doc().sheets[0].components[0].clone();
    let created = e
        .apply(EditCommand::Paste {
            sheet: SHEET,
            clip: Clip {
                components: vec![original],
                ..Clip::default()
            },
            delta: Point::grid(5, 0),
        })
        .unwrap()
        .created;
    assert_eq!(created.len(), 1);
    assert_ne!(created[0], r1);
    let pasted = &e.doc().sheets[0].components[1];
    assert_eq!(pasted.reference, "R2");
    assert_eq!(pasted.at, Point::grid(5, 0));
    e.doc().validate().unwrap();
}

#[test]
fn annotate_orders_by_sheet_then_position() {
    let mut e = editor();
    place(&mut e, SHEET, 10, 0);
    place(&mut e, SHEET, 0, 0);
    place(&mut e, SHEET, 0, 10);
    e.apply(EditCommand::Annotate).unwrap();
    let by_position: Vec<_> = e.doc().sheets[0]
        .components
        .iter()
        .map(|c| (c.at, c.reference.clone()))
        .collect();
    assert!(by_position.contains(&(Point::grid(0, 0), "R1".to_owned())));
    assert!(by_position.contains(&(Point::grid(10, 0), "R2".to_owned())));
    assert!(by_position.contains(&(Point::grid(0, 10), "R3".to_owned())));
}

#[test]
fn undo_and_redo_restore_exact_states() {
    let mut e = editor();
    assert!(!e.can_undo());
    let start = e.doc().clone();
    place(&mut e, SHEET, 0, 0);
    let placed = e.doc().clone();
    assert!(e.can_undo());
    e.undo().unwrap();
    assert_eq!(e.doc(), &start);
    assert!(e.can_redo());
    e.redo().unwrap();
    assert_eq!(e.doc(), &placed);
    e.undo().unwrap();
    place(&mut e, SHEET, 1, 1);
    assert!(!e.can_redo(), "a new edit discards the redo states");
}

#[test]
fn a_failing_command_changes_nothing_and_adds_no_undo_step() {
    let mut e = editor();
    let before = e.doc().clone();
    assert_eq!(
        e.apply(EditCommand::AddWires {
            sheet: SHEET,
            points: vec![Point::grid(0, 0), Point::grid(1, 0), Point::grid(2, 3)],
        }),
        Err(EditError::NotOrthogonal)
    );
    assert_eq!(e.doc(), &before);
    assert!(!e.can_undo());
    assert!(matches!(
        e.apply(EditCommand::PlaceComponent {
            sheet: SHEET,
            symbol: "unknown".to_owned(),
            at: Point::default(),
            rotation: Rotation::R0,
            mirror: false,
        }),
        Err(EditError::UnknownSymbol(_))
    ));
    assert_eq!(
        e.apply(EditCommand::AddLabel {
            sheet: SHEET,
            at: Point::default(),
            name: " ".to_owned(),
            kind: LabelKind::Local,
        }),
        Err(EditError::EmptyText)
    );
}

#[test]
fn a_no_change_command_adds_no_undo_step() {
    let mut e = editor();
    let r = place(&mut e, SHEET, 0, 0);
    e.undo().unwrap();
    e.redo().unwrap();
    let steps_before = e.can_undo();
    e.apply(EditCommand::Move {
        sheet: SHEET,
        ids: vec![r],
        delta: Point::default(),
    })
    .unwrap();
    assert_eq!(e.can_undo(), steps_before);
    assert!(!e.can_redo());
}

#[test]
fn net_names_and_netlist_text_are_stable() {
    let mut e = editor();
    place(&mut e, SHEET, 0, 0);
    place(&mut e, SHEET, 10, 0);
    wires(&mut e, SHEET, &[(0, 2), (10, 2)]);
    e.apply(EditCommand::AddLabel {
        sheet: SHEET,
        at: Point::grid(0, -2),
        name: "SIG".to_owned(),
        kind: LabelKind::Global,
    })
    .unwrap();
    let connectivity = Connectivity::analyze(e.doc());
    let text = Netlist::build(e.doc(), &connectivity).to_text();
    assert_eq!(
        text,
        "# TronicCAD netlist\n\n[components]\nR1\t1k\tt:r\t1\nR2\t1k\tt:r\t1\n\n\
         [nets]\nNet-R1-2\tR1.2 R2.2\nNet-R2-1\tR2.1\nSIG\tR1.1\n"
    );
}

#[test]
fn validation_rejects_broken_files() {
    let mut e = editor();
    let r = place(&mut e, SHEET, 0, 0);
    let good = e.doc().clone();
    good.validate().unwrap();

    let mut unknown_symbol = good.clone();
    unknown_symbol.symbols.clear();
    assert!(unknown_symbol.validate().is_err());

    let mut duplicate_id = good.clone();
    let sheet = std::sync::Arc::make_mut(&mut duplicate_id.sheets[0]);
    sheet.wires.push(Segment {
        id: r,
        a: Point::default(),
        b: Point::grid(1, 0),
    });
    assert!(duplicate_id.validate().is_err());

    let mut oblique = good.clone();
    let id = oblique.next_id;
    oblique.next_id += 1;
    std::sync::Arc::make_mut(&mut oblique.sheets[0])
        .wires
        .push(Segment {
            id,
            a: Point::default(),
            b: Point::grid(1, 1),
        });
    assert!(oblique.validate().is_err());

    let mut no_sheets = good.clone();
    no_sheets.sheets.clear();
    assert!(no_sheets.validate().is_err());

    let mut cycle = good;
    std::sync::Arc::make_mut(&mut cycle.sheets[0]).parent = Some(SHEET);
    assert!(cycle.validate().is_err());
}

/// Tidying wires must never change what is connected. Random circuits with a
/// fixed seed, compared against the same wires without tidying.
#[test]
fn normalization_keeps_connectivity_on_random_wires() {
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, bound: i64) -> i64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((self.0 >> 33) % bound as u64) as i64
        }
    }
    let mut rng = Lcg(42);
    for _round in 0..200 {
        let mut plain = Schematic::default();
        plain
            .symbols
            .insert("t:r".to_owned(), std::sync::Arc::new(two_pin()));
        let mut components = Vec::new();
        for n in 0..6u64 {
            components.push(Component {
                id: 100 + n,
                symbol: "t:r".to_owned(),
                reference: format!("R{}", n + 1),
                value: String::new(),
                at: Point::grid(rng.next(12), rng.next(12)),
                rotation: Rotation::R0,
                mirror: false,
                footprint: None,
            });
        }
        let mut segments = Vec::new();
        for n in 0..12u64 {
            let a = (rng.next(12), rng.next(12));
            let length = rng.next(8) - 4;
            let b = if rng.next(2) == 0 {
                (a.0 + length, a.1)
            } else {
                (a.0, a.1 + length)
            };
            segments.push(Segment {
                id: 200 + n,
                a: Point::grid(a.0, a.1),
                b: Point::grid(b.0, b.1),
            });
        }
        plain.next_id = 1000;
        {
            let sheet = std::sync::Arc::make_mut(&mut plain.sheets[0]);
            sheet.components = components;
            sheet.wires = segments.clone();
        }
        // Editor commands normalize; adding a far away wire triggers it.
        let mut editor = Editor::new(plain.clone());
        editor
            .apply(EditCommand::AddWires {
                sheet: SHEET,
                points: vec![Point::grid(1000, 1000), Point::grid(1001, 1000)],
            })
            .unwrap();
        let partition = |doc: &Schematic| -> BTreeSet<BTreeSet<String>> {
            let connectivity = Connectivity::analyze(doc);
            Netlist::build(doc, &connectivity)
                .nets
                .into_iter()
                .map(|net| {
                    net.nodes
                        .into_iter()
                        .map(|node| format!("{}.{}", node.reference, node.pin))
                        .collect()
                })
                .collect()
        };
        assert_eq!(partition(&plain), partition(editor.doc()));
    }
}
