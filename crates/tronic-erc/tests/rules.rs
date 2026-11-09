//! One failing case per rule, one clean reference circuit per circuit type.
#![allow(clippy::unwrap_used, clippy::panic)]

use tronic_erc::{check, Severity};
use tronic_library::builtin;
use tronic_schematic::{EditCommand, Editor, ElementId, LabelKind, Point};

const SHEET: ElementId = 1;

struct Bench(Editor);

impl Bench {
    fn new() -> Self {
        let mut editor = Editor::default();
        editor.add_symbols(builtin());
        Self(editor)
    }

    fn place(&mut self, symbol: &str, x: i64, y: i64) -> ElementId {
        self.0
            .apply(EditCommand::PlaceComponent {
                sheet: SHEET,
                symbol: symbol.to_owned(),
                at: Point::grid(x, y),
                rotation: Default::default(),
                mirror: false,
            })
            .unwrap()
            .created[0]
    }

    fn pin(&self, id: ElementId, number: &str) -> Point {
        let doc = self.0.doc();
        for sheet in &doc.sheets {
            if let Some(component) = sheet.components.iter().find(|c| c.id == id) {
                let symbol = &doc.symbols[&component.symbol];
                let index = symbol.pins.iter().position(|p| p.number == number).unwrap();
                return doc.pin_points(component)[index];
            }
        }
        panic!("component {id} not found");
    }

    /// Connects two points with an L-shaped wire.
    fn wire(&mut self, a: Point, b: Point) {
        self.wire_on(SHEET, a, b);
    }

    fn wire_on(&mut self, sheet: ElementId, a: Point, b: Point) {
        let corner = Point::new(b.x, a.y);
        let points = if corner == a || corner == b {
            vec![a, b]
        } else {
            vec![a, corner, b]
        };
        self.0
            .apply(EditCommand::AddWires { sheet, points })
            .unwrap();
    }

    fn label(&mut self, at: Point, name: &str, kind: LabelKind) -> ElementId {
        self.0
            .apply(EditCommand::AddLabel {
                sheet: SHEET,
                at,
                name: name.to_owned(),
                kind,
            })
            .unwrap()
            .created[0]
    }

    fn no_connect(&mut self, at: Point) {
        self.0
            .apply(EditCommand::AddNoConnect { sheet: SHEET, at })
            .unwrap();
    }

    fn codes(&self) -> Vec<&'static str> {
        let mut codes: Vec<_> = check(self.0.doc()).into_iter().map(|v| v.code).collect();
        codes.sort_unstable();
        codes.dedup();
        codes
    }

    fn has(&self, code: &str) -> bool {
        self.codes().contains(&code)
    }
}

// Reference circuits must be free of findings: no false alarms.

#[test]
fn clean_empty_schematic() {
    assert!(Bench::new().codes().is_empty());
}

/// LED with series resistor on a fixed supply.
#[test]
fn clean_led_circuit() {
    let mut b = Bench::new();
    let r = b.place("device:resistor", 0, 0);
    let led = b.place("device:led", 0, 12);
    let vcc = b.place("supply:5v", 0, -8);
    let gnd = b.place("supply:gnd", 0, 20);
    b.wire(b.pin(vcc, "1"), b.pin(r, "1"));
    b.wire(b.pin(r, "2"), b.pin(led, "1"));
    b.wire(b.pin(led, "2"), b.pin(gnd, "1"));
    assert_eq!(b.codes(), Vec::<&str>::new());
}

/// 7805 with input and output capacitor.
#[test]
fn clean_regulator_circuit() {
    let mut b = Bench::new();
    let reg = b.place("power:7805", 0, 0);
    let c1 = b.place("device:capacitor", -10, 6);
    let c2 = b.place("device:capacitor", 10, 6);
    let input = b.place("supply:12v", -14, -4);
    let gnd = b.place("supply:gnd", 0, 16);
    let out = b.label(b.pin(reg, "3"), "VOUT", LabelKind::Local);
    let _ = out;
    b.wire(b.pin(input, "1"), b.pin(reg, "1"));
    b.wire(b.pin(reg, "1"), b.pin(c1, "1"));
    b.wire(b.pin(reg, "3"), b.pin(c2, "1"));
    b.wire(b.pin(c1, "2"), b.pin(gnd, "1"));
    b.wire(b.pin(c2, "2"), b.pin(gnd, "1"));
    b.wire(b.pin(reg, "2"), b.pin(gnd, "1"));
    assert_eq!(b.codes(), Vec::<&str>::new());
}

/// Inverter driven by a connector, output to a connector.
#[test]
fn clean_logic_circuit() {
    let mut b = Bench::new();
    let gate = b.place("logic:not", 0, 0);
    let input = b.place("connector:1x02", -20, 0);
    let output = b.place("connector:1x02", 20, 0);
    let vcc = b.place("supply:vcc", 0, -12);
    let gnd = b.place("supply:gnd", 0, 12);
    b.wire(b.pin(input, "1"), b.pin(gate, "1"));
    b.wire(b.pin(gate, "2"), b.pin(output, "1"));
    b.wire(b.pin(vcc, "1"), b.pin(gate, "3"));
    b.wire(b.pin(gate, "4"), b.pin(gnd, "1"));
    b.no_connect(b.pin(input, "2"));
    b.no_connect(b.pin(output, "2"));
    assert_eq!(b.codes(), Vec::<&str>::new());
}

// One test per rule.

#[test]
fn finds_a_short_between_supplies() {
    let mut b = Bench::new();
    let a = b.place("supply:vcc", 0, 0);
    let c = b.place("supply:gnd", 10, 0);
    b.wire(b.pin(a, "1"), b.pin(c, "1"));
    let violations = check(b.0.doc());
    let short: Vec<_> = violations
        .iter()
        .filter(|v| v.code == "supply_short")
        .collect();
    assert_eq!(short.len(), 2);
    assert_eq!(short[0].severity, Severity::Error);
    assert_eq!(short[0].params["names"], "GND, VCC");
}

#[test]
fn finds_a_shorted_component() {
    let mut b = Bench::new();
    let r = b.place("device:resistor", 0, 0);
    b.wire(b.pin(r, "1"), b.pin(r, "2"));
    assert!(b.has("component_shorted"));
}

#[test]
fn finds_an_unconnected_pin() {
    let mut b = Bench::new();
    b.place("device:resistor", 0, 0);
    let violations = check(b.0.doc());
    let open: Vec<_> = violations
        .iter()
        .filter(|v| v.code == "pin_not_connected")
        .collect();
    assert_eq!(open.len(), 2);
    assert_eq!(open[0].params["pin"], "R1.1");
}

#[test]
fn a_no_connect_marker_silences_an_open_pin_only() {
    let mut b = Bench::new();
    let r = b.place("device:resistor", 0, 0);
    b.no_connect(b.pin(r, "1"));
    let open: Vec<_> = check(b.0.doc())
        .into_iter()
        .filter(|v| v.code == "pin_not_connected")
        .collect();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].params["pin"], "R1.2");
}

#[test]
fn finds_a_no_connect_marker_on_a_connected_net() {
    let mut b = Bench::new();
    let r1 = b.place("device:resistor", 0, 0);
    let r2 = b.place("device:resistor", 10, 0);
    b.wire(b.pin(r1, "1"), b.pin(r2, "1"));
    b.no_connect(b.pin(r1, "1"));
    assert!(b.has("no_connect_on_net"));
}

#[test]
fn finds_a_no_connect_marker_in_the_air() {
    let mut b = Bench::new();
    b.no_connect(Point::grid(5, 5));
    assert!(b.has("no_connect_unattached"));
}

#[test]
fn finds_a_wired_not_connected_pin() {
    let mut b = Bench::new();
    let mcu = b.place("mcu:generic_24", 0, 0);
    let r = b.place("device:resistor", -40, 0);
    b.wire(b.pin(mcu, "4"), b.pin(r, "1"));
    assert!(b.has("not_connected_pin_wired"));
}

#[test]
fn finds_an_output_on_a_supply() {
    let mut b = Bench::new();
    let gate = b.place("logic:not", 0, 0);
    let gnd = b.place("supply:gnd", 20, 0);
    b.wire(b.pin(gate, "2"), b.pin(gnd, "1"));
    assert!(b.has("output_on_supply"));
}

#[test]
fn finds_a_missing_supply() {
    let mut b = Bench::new();
    let gate = b.place("logic:not", 0, 0);
    let violations = check(b.0.doc());
    let missing: Vec<_> = violations
        .iter()
        .filter(|v| v.code == "missing_supply")
        .collect();
    assert_eq!(missing.len(), 2);

    // A supply symbol on the pin fixes it for that pin.
    let vcc = b.place("supply:vcc", 0, -12);
    b.wire(b.pin(vcc, "1"), b.pin(gate, "3"));
    let missing: Vec<_> = check(b.0.doc())
        .into_iter()
        .filter(|v| v.code == "missing_supply")
        .collect();
    assert_eq!(missing.len(), 1);
}

#[test]
fn a_power_output_counts_as_a_source() {
    let mut b = Bench::new();
    let reg = b.place("power:7805", 0, 0);
    let gate = b.place("logic:not", 30, 0);
    b.wire(b.pin(reg, "3"), b.pin(gate, "3"));
    let gate_supply = b.pin(gate, "3");
    let on_vcc = check(b.0.doc())
        .into_iter()
        .any(|v| v.code == "missing_supply" && v.at == gate_supply);
    assert!(!on_vcc);
}

#[test]
fn finds_inputs_without_a_driver() {
    let mut b = Bench::new();
    let a = b.place("logic:not", 0, 0);
    let c = b.place("logic:not", 30, 10);
    b.wire(b.pin(c, "1"), b.pin(a, "1"));
    assert!(b.has("input_not_driven"));
}

#[test]
fn finds_conflicting_net_names() {
    let mut b = Bench::new();
    let r1 = b.place("device:resistor", 0, 0);
    let r2 = b.place("device:resistor", 10, 0);
    b.wire(b.pin(r1, "1"), b.pin(r2, "1"));
    b.label(b.pin(r1, "1"), "A", LabelKind::Local);
    b.label(b.pin(r2, "1"), "B", LabelKind::Local);
    assert!(b.has("conflicting_net_names"));
}

#[test]
fn finds_a_label_in_the_air() {
    let mut b = Bench::new();
    b.label(Point::grid(3, 3), "X", LabelKind::Local);
    assert!(b.has("label_not_connected"));
}

#[test]
fn finds_a_dangling_wire() {
    let mut b = Bench::new();
    b.wire(Point::grid(0, 0), Point::grid(5, 0));
    assert!(b.has("wire_dangling"));
}

#[test]
fn finds_an_unused_supply_symbol() {
    let mut b = Bench::new();
    b.place("supply:gnd", 0, 0);
    assert!(b.has("supply_port_unused"));
}

#[test]
fn finds_a_hierarchical_label_on_a_top_level_sheet() {
    let mut b = Bench::new();
    let r = b.place("device:resistor", 0, 0);
    b.label(b.pin(r, "1"), "IN", LabelKind::Hierarchical);
    assert!(b.has("hierarchical_label_without_block"));
}

#[test]
fn finds_an_unconnected_block_port() {
    let mut b = Bench::new();
    let block =
        b.0.apply(EditCommand::AddBlock {
            sheet: SHEET,
            at: Point::grid(10, 10),
            name: "Unterblatt".to_owned(),
        })
        .unwrap()
        .created[0];
    let child = b.0.doc().sheet(SHEET).unwrap().instances[0].sheet;
    assert_eq!(b.0.doc().sheet(SHEET).unwrap().instances[0].id, block);
    b.0.apply(EditCommand::AddLabel {
        sheet: child,
        at: Point::grid(0, 0),
        name: "IN".to_owned(),
        kind: LabelKind::Hierarchical,
    })
    .unwrap();
    assert!(b.has("port_not_connected"));
}

#[test]
fn finds_several_outputs_on_a_net() {
    let mut b = Bench::new();
    let a = b.place("logic:not", 0, 0);
    let c = b.place("logic:not", 30, 0);
    b.wire(b.pin(a, "2"), b.pin(c, "2"));
    let violations = check(b.0.doc());
    let multiple: Vec<_> = violations
        .iter()
        .filter(|v| v.code == "multiple_outputs")
        .collect();
    assert_eq!(multiple.len(), 2);
    assert_eq!(multiple[0].params["pins"], "U1.2, U2.2");
}

#[test]
fn finds_several_power_outputs_on_a_net() {
    let mut b = Bench::new();
    let a = b.place("power:7805", 0, 0);
    let c = b.place("power:7805", 30, 0);
    b.wire(b.pin(a, "3"), b.pin(c, "3"));
    assert!(b.has("multiple_power_outputs"));
}

#[test]
fn finds_an_output_against_a_power_output() {
    let mut b = Bench::new();
    let reg = b.place("power:7805", 0, 0);
    let gate = b.place("logic:not", 30, 0);
    b.wire(b.pin(reg, "3"), b.pin(gate, "2"));
    assert!(b.has("output_conflict"));
}

#[test]
fn open_collector_outputs_may_share_a_net() {
    let mut b = Bench::new();
    let a = b.place("timer:ne555", 0, 0);
    let c = b.place("timer:ne555", 50, 0);
    b.wire(b.pin(a, "7"), b.pin(c, "7"));
    assert!(!b.has("multiple_outputs"));
    assert!(!b.has("output_conflict"));
}

#[test]
fn finds_duplicate_references_across_sheets() {
    let mut b = Bench::new();
    let first = b.place("device:resistor", 0, 0);
    let sheet2 =
        b.0.apply(EditCommand::AddSheet {
            name: "2".to_owned(),
        })
        .unwrap()
        .created[0];
    let second =
        b.0.apply(EditCommand::PlaceComponent {
            sheet: sheet2,
            symbol: "device:resistor".to_owned(),
            at: Point::grid(0, 0),
            rotation: Default::default(),
            mirror: false,
        })
        .unwrap()
        .created[0];
    b.0.apply(EditCommand::SetComponent {
        sheet: sheet2,
        id: second,
        reference: "R1".to_owned(),
        value: "1k".to_owned(),
    })
    .unwrap();
    let duplicates: Vec<_> = check(b.0.doc())
        .into_iter()
        .filter(|v| v.code == "duplicate_reference")
        .collect();
    assert_eq!(duplicates.len(), 2);
    assert!(duplicates.iter().any(|v| v.elements == [first]));
    assert!(duplicates
        .iter()
        .any(|v| v.elements == [second] && v.sheet == sheet2));
}

#[test]
fn finds_unannotated_references() {
    let mut b = Bench::new();
    let r = b.place("device:resistor", 0, 0);
    b.0.apply(EditCommand::SetComponent {
        sheet: SHEET,
        id: r,
        reference: "R?".to_owned(),
        value: String::new(),
    })
    .unwrap();
    assert!(b.has("reference_not_annotated"));
}

mod buses {
    use super::*;

    fn with_bus(label: Option<&str>) -> Bench {
        let mut b = Bench::new();
        b.0.apply(EditCommand::AddBuses {
            sheet: SHEET,
            points: vec![Point::grid(0, 0), Point::grid(20, 0)],
        })
        .unwrap();
        if let Some(name) = label {
            b.label(Point::grid(0, 0), name, LabelKind::Local);
        }
        b
    }

    #[test]
    fn finds_a_bus_without_a_label() {
        assert!(with_bus(None).has("bus_label_missing"));
    }

    #[test]
    fn finds_an_invalid_bus_name() {
        assert!(with_bus(Some("D[0..x]")).has("bus_label_invalid"));
    }

    #[test]
    fn accepts_a_valid_bus_with_a_member_entry() {
        let mut b = with_bus(Some("D[0..7]"));
        let r = b.place("device:resistor", 5, 10);
        let pin = b.pin(r, "1");
        b.wire(pin, Point::new(pin.x, 0));
        b.label(pin, "D3", LabelKind::Local);
        assert!(!b.has("bus_entry_not_member"));
        assert!(!b.has("bus_label_missing"));
        assert!(!b.has("wire_dangling"));
    }

    #[test]
    fn finds_a_wire_to_a_bus_without_a_member_label() {
        let mut b = with_bus(Some("D[0..7]"));
        let r = b.place("device:resistor", 5, 10);
        let pin = b.pin(r, "1");
        b.wire(pin, Point::new(pin.x, 0));
        assert!(b.has("bus_entry_not_member"));
        b.label(pin, "X9", LabelKind::Local);
        assert!(b.has("bus_entry_not_member"));
    }
}

#[test]
fn the_result_is_deterministic() {
    let mut b = Bench::new();
    b.place("device:resistor", 0, 0);
    b.place("logic:not", 30, 0);
    assert_eq!(check(b.0.doc()), check(b.0.doc()));
}
