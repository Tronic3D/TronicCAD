//! Electrical rule check. Rules and severities: `docs/specs/schematic.md`.

use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use tronic_schematic::connectivity::{parse_bus_name, SegmentIndex};
use tronic_schematic::{
    Connectivity, ElementId, LabelKind, Net, PinKind, PinNode, Point, Schematic, Segment,
    SymbolKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Violation {
    /// Stable identifier, also the key of the translated message.
    pub code: &'static str,
    pub severity: Severity,
    pub sheet: ElementId,
    pub at: Point,
    pub elements: Vec<ElementId>,
    pub params: BTreeMap<&'static str, String>,
}

struct Report(Vec<Violation>);

impl Report {
    fn add(
        &mut self,
        code: &'static str,
        severity: Severity,
        sheet: ElementId,
        at: Point,
        elements: Vec<ElementId>,
        params: &[(&'static str, String)],
    ) {
        self.0.push(Violation {
            code,
            severity,
            sheet,
            at,
            elements,
            params: params.iter().cloned().collect(),
        });
    }
}

/// Checks a schematic. The result is deterministic.
pub fn check(doc: &Schematic) -> Vec<Violation> {
    let connectivity = Connectivity::analyze(doc);
    let mut report = Report(Vec::new());
    check_references(doc, &mut report);
    for net in &connectivity.nets {
        check_net(net, &mut report);
    }
    check_shorted_components(doc, &connectivity, &mut report);
    check_sheets(doc, &connectivity, &mut report);
    report.0.sort_by(|a, b| {
        (a.severity, a.sheet, a.at, a.code, &a.elements).cmp(&(
            b.severity,
            b.sheet,
            b.at,
            b.code,
            &b.elements,
        ))
    });
    report.0
}

fn check_references(doc: &Schematic, report: &mut Report) {
    let mut seen: HashMap<&str, Vec<(ElementId, ElementId, Point)>> = HashMap::new();
    for sheet in &doc.sheets {
        for component in &sheet.components {
            let is_supply = doc
                .symbols
                .get(&component.symbol)
                .is_some_and(|symbol| symbol.kind == SymbolKind::SupplyPort);
            if is_supply {
                continue;
            }
            if component.reference.contains('?') {
                report.add(
                    "reference_not_annotated",
                    Severity::Warning,
                    sheet.id,
                    component.at,
                    vec![component.id],
                    &[("reference", component.reference.clone())],
                );
            } else {
                seen.entry(&component.reference).or_default().push((
                    sheet.id,
                    component.id,
                    component.at,
                ));
            }
        }
    }
    for (reference, places) in seen {
        if places.len() > 1 {
            for (sheet, id, at) in places {
                report.add(
                    "duplicate_reference",
                    Severity::Error,
                    sheet,
                    at,
                    vec![id],
                    &[("reference", reference.to_owned())],
                );
            }
        }
    }
}

fn pin_label(pin: &PinNode) -> (&'static str, String) {
    ("pin", format!("{}.{}", pin.reference, pin.number))
}

fn pin_list(pins: &[&PinNode]) -> String {
    pins.iter()
        .map(|pin| format!("{}.{}", pin.reference, pin.number))
        .collect::<Vec<_>>()
        .join(", ")
}

fn check_net(net: &Net, report: &mut Report) {
    let of_kind = |kinds: &[PinKind]| -> Vec<&PinNode> {
        net.pins
            .iter()
            .filter(|pin| kinds.contains(&pin.kind))
            .collect()
    };
    let outputs = of_kind(&[PinKind::Output]);
    let power_outputs = of_kind(&[PinKind::PowerOut]);
    let supply_ports = of_kind(&[PinKind::SupplyPort]);
    let weak_drivers = of_kind(&[
        PinKind::TriState,
        PinKind::OpenCollector,
        PinKind::OpenEmitter,
    ]);
    let any_driver = of_kind(&[
        PinKind::Output,
        PinKind::PowerOut,
        PinKind::SupplyPort,
        PinKind::TriState,
        PinKind::OpenCollector,
        PinKind::OpenEmitter,
        PinKind::Bidirectional,
        PinKind::Passive,
    ]);
    let net_name = ("net", net.name.clone());

    // Short circuits between supplies.
    if net.supply_names.len() > 1 {
        let names: Vec<String> = net.supply_names.iter().cloned().collect();
        for pin in &supply_ports {
            report.add(
                "supply_short",
                Severity::Error,
                pin.sheet,
                pin.at,
                vec![pin.component],
                &[("names", names.join(", "))],
            );
        }
    }

    // Several drivers on one net.
    if outputs.len() > 1 {
        for pin in &outputs {
            report.add(
                "multiple_outputs",
                Severity::Error,
                pin.sheet,
                pin.at,
                vec![pin.component],
                &[net_name.clone(), ("pins", pin_list(&outputs))],
            );
        }
    }
    if power_outputs.len() > 1 {
        for pin in &power_outputs {
            report.add(
                "multiple_power_outputs",
                Severity::Error,
                pin.sheet,
                pin.at,
                vec![pin.component],
                &[net_name.clone(), ("pins", pin_list(&power_outputs))],
            );
        }
    }
    if !outputs.is_empty() && !power_outputs.is_empty() {
        for pin in outputs.iter().chain(&power_outputs) {
            report.add(
                "output_conflict",
                Severity::Error,
                pin.sheet,
                pin.at,
                vec![pin.component],
                std::slice::from_ref(&net_name),
            );
        }
    }
    if !outputs.is_empty() && !weak_drivers.is_empty() {
        for pin in outputs.iter().chain(&weak_drivers) {
            report.add(
                "output_conflict",
                Severity::Error,
                pin.sheet,
                pin.at,
                vec![pin.component],
                std::slice::from_ref(&net_name),
            );
        }
    }
    if !outputs.is_empty() && !supply_ports.is_empty() {
        for pin in &outputs {
            report.add(
                "output_on_supply",
                Severity::Error,
                pin.sheet,
                pin.at,
                vec![pin.component],
                &[pin_label(pin), net_name.clone()],
            );
        }
    }

    // Pins the part says must stay open.
    if net.pins.len() > 1 {
        for pin in of_kind(&[PinKind::NotConnected]) {
            report.add(
                "not_connected_pin_wired",
                Severity::Error,
                pin.sheet,
                pin.at,
                vec![pin.component],
                &[pin_label(pin)],
            );
        }
    }

    // Missing supply.
    if supply_ports.is_empty() && power_outputs.is_empty() {
        for pin in of_kind(&[PinKind::PowerIn]) {
            report.add(
                "missing_supply",
                Severity::Error,
                pin.sheet,
                pin.at,
                vec![pin.component],
                &[pin_label(pin)],
            );
        }
    }

    // Inputs that nothing can drive.
    if any_driver.is_empty() && net.pins.len() > 1 {
        for pin in of_kind(&[PinKind::Input]) {
            report.add(
                "input_not_driven",
                Severity::Warning,
                pin.sheet,
                pin.at,
                vec![pin.component],
                &[pin_label(pin)],
            );
        }
    }

    // Open pins and no-connect markers.
    if net.pins.len() == 1 {
        let pin = &net.pins[0];
        if pin.kind == PinKind::SupplyPort {
            if net.wires.is_empty() && net.labels.is_empty() {
                report.add(
                    "supply_port_unused",
                    Severity::Warning,
                    pin.sheet,
                    pin.at,
                    vec![pin.component],
                    std::slice::from_ref(&net_name),
                );
            }
        } else if pin.kind != PinKind::NotConnected
            && pin.kind != PinKind::PowerIn
            && net.no_connects.is_empty()
        {
            report.add(
                "pin_not_connected",
                Severity::Error,
                pin.sheet,
                pin.at,
                vec![pin.component],
                &[pin_label(pin)],
            );
        }
    } else if net.pins.len() > 1 {
        for marker in &net.no_connects {
            report.add(
                "no_connect_on_net",
                Severity::Warning,
                marker.sheet,
                marker.at,
                vec![marker.id],
                std::slice::from_ref(&net_name),
            );
        }
    }
    if net.pins.is_empty() {
        for marker in &net.no_connects {
            report.add(
                "no_connect_unattached",
                Severity::Warning,
                marker.sheet,
                marker.at,
                vec![marker.id],
                &[],
            );
        }
    }

    // Different names for one net. Supply and global names are compared with
    // local ones only when they differ in meaning, so `/Sheet/X` next to `X` counts.
    let distinct: HashSet<&str> = net
        .label_names
        .iter()
        .map(String::as_str)
        .chain(net.supply_names.iter().map(String::as_str))
        .collect();
    if distinct.len() > 1 && net.supply_names.len() <= 1 {
        let mut names: Vec<&str> = distinct.into_iter().collect();
        names.sort_unstable();
        if let Some(label) = net.labels.first() {
            report.add(
                "conflicting_net_names",
                Severity::Warning,
                label.sheet,
                label.at,
                vec![label.id],
                &[("names", names.join(", "))],
            );
        }
    }
}

fn check_shorted_components(doc: &Schematic, connectivity: &Connectivity, report: &mut Report) {
    let mut pins_on_net: HashMap<ElementId, (usize, HashSet<usize>)> = HashMap::new();
    for (index, net) in connectivity.nets.iter().enumerate() {
        for pin in &net.pins {
            let entry = pins_on_net.entry(pin.component).or_default();
            entry.0 += 1;
            entry.1.insert(index);
        }
    }
    for sheet in &doc.sheets {
        for component in &sheet.components {
            let Some(symbol) = doc.symbols.get(&component.symbol) else {
                continue;
            };
            if symbol.kind == SymbolKind::SupplyPort || symbol.pins.len() < 2 {
                continue;
            }
            if let Some((count, nets)) = pins_on_net.get(&component.id) {
                if *count >= 2 && nets.len() == 1 {
                    report.add(
                        "component_shorted",
                        Severity::Warning,
                        sheet.id,
                        component.at,
                        vec![component.id],
                        &[("reference", component.reference.clone())],
                    );
                }
            }
        }
    }
}

fn check_sheets(doc: &Schematic, connectivity: &Connectivity, report: &mut Report) {
    for sheet in &doc.sheets {
        let wire_index = SegmentIndex::new(&sheet.wires);

        // Everything a wire end or a label can attach to.
        let mut pin_points: HashSet<Point> = HashSet::new();
        for component in &sheet.components {
            pin_points.extend(doc.pin_points(component));
        }
        let port_points: HashSet<Point> = sheet
            .instances
            .iter()
            .flat_map(|instance| {
                instance
                    .ports
                    .iter()
                    .map(|port| instance.at.offset(port.at))
            })
            .collect();
        let label_points: HashSet<Point> = sheet.labels.iter().map(|label| label.at).collect();
        let bus_index = SegmentIndex::new(&sheet.buses);

        // Labels.
        for label in &sheet.labels {
            let on_bus = bus_index.at(label.at).next().is_some();
            match parse_bus_name(&label.name) {
                Some(Err(())) => report.add(
                    "bus_label_invalid",
                    Severity::Error,
                    sheet.id,
                    label.at,
                    vec![label.id],
                    &[("name", label.name.clone())],
                ),
                Some(Ok(_)) => {
                    if !on_bus {
                        report.add(
                            "label_not_connected",
                            Severity::Warning,
                            sheet.id,
                            label.at,
                            vec![label.id],
                            &[("name", label.name.clone())],
                        );
                    }
                }
                None => {
                    let attached = wire_index.at(label.at).next().is_some()
                        || pin_points.contains(&label.at)
                        || port_points.contains(&label.at);
                    if !attached {
                        report.add(
                            "label_not_connected",
                            Severity::Warning,
                            sheet.id,
                            label.at,
                            vec![label.id],
                            &[("name", label.name.clone())],
                        );
                    }
                    if label.kind == LabelKind::Hierarchical && sheet.parent.is_none() {
                        report.add(
                            "hierarchical_label_without_block",
                            Severity::Error,
                            sheet.id,
                            label.at,
                            vec![label.id],
                            &[("name", label.name.clone())],
                        );
                    }
                }
            }
        }

        // Wire ends in the air.
        for wire in &sheet.wires {
            for end in [wire.a, wire.b] {
                let others = wire_index
                    .at(end)
                    .filter(|other| other.id != wire.id)
                    .count();
                let attached = others > 0
                    || pin_points.contains(&end)
                    || port_points.contains(&end)
                    || label_points.contains(&end)
                    || bus_index.at(end).next().is_some();
                if !attached {
                    report.add(
                        "wire_dangling",
                        Severity::Warning,
                        sheet.id,
                        end,
                        vec![wire.id],
                        &[],
                    );
                }
            }
        }

        // Block ports without anything behind or in front of them.
        for instance in &sheet.instances {
            for (index, port) in instance.ports.iter().enumerate() {
                let at = instance.at.offset(port.at);
                let connected_outside =
                    wire_index.at(at).next().is_some() || pin_points.contains(&at);
                let net = connectivity.nets.iter().find(|net| {
                    net.ports
                        .iter()
                        .any(|(a, i)| a.id == instance.id && *i == index)
                });
                let has_pins = net.is_some_and(|net| !net.pins.is_empty());
                if !connected_outside || !has_pins {
                    report.add(
                        "port_not_connected",
                        Severity::Warning,
                        sheet.id,
                        at,
                        vec![instance.id],
                        &[("name", port.name.clone())],
                    );
                }
            }
        }

        check_buses(sheet, connectivity, report);
    }
}

fn check_buses(sheet: &tronic_schematic::Sheet, connectivity: &Connectivity, report: &mut Report) {
    if sheet.buses.is_empty() {
        return;
    }
    // Group touching bus segments.
    let mut group: HashMap<ElementId, usize> = HashMap::new();
    let mut groups: Vec<Vec<Segment>> = Vec::new();
    let index = SegmentIndex::new(&sheet.buses);
    for start in &sheet.buses {
        if group.contains_key(&start.id) {
            continue;
        }
        let number = groups.len();
        let mut members = Vec::new();
        let mut pending = vec![*start];
        group.insert(start.id, number);
        while let Some(segment) = pending.pop() {
            members.push(segment);
            for end in [segment.a, segment.b] {
                for other in index.at(end) {
                    if group.insert(other.id, number).is_none() {
                        pending.push(*other);
                    }
                }
            }
            // A segment may end on this one.
            for other in &sheet.buses {
                if !group.contains_key(&other.id)
                    && (segment.contains(other.a) || segment.contains(other.b))
                {
                    group.insert(other.id, number);
                    pending.push(*other);
                }
            }
        }
        groups.push(members);
    }

    for members in &groups {
        let on_bus = |p: Point| members.iter().any(|segment| segment.contains(p));
        let member_names: Option<HashSet<String>> = sheet
            .labels
            .iter()
            .filter(|label| on_bus(label.at))
            .find_map(|label| match parse_bus_name(&label.name) {
                Some(Ok((_, names))) => Some(names.into_iter().collect()),
                _ => None,
            });
        let first = &members[0];
        let Some(member_names) = member_names else {
            report.add(
                "bus_label_missing",
                Severity::Warning,
                sheet.id,
                first.a,
                vec![first.id],
                &[],
            );
            continue;
        };
        for wire in &sheet.wires {
            let end = if on_bus(wire.a) {
                wire.a
            } else if on_bus(wire.b) {
                wire.b
            } else {
                continue;
            };
            let carries_member = connectivity.net_of(sheet.id, wire.id).is_some_and(|net| {
                sheet.labels.iter().any(|label| {
                    member_names.contains(&label.name)
                        && net.labels.iter().any(|attached| attached.id == label.id)
                })
            });
            if !carries_member {
                report.add(
                    "bus_entry_not_member",
                    Severity::Warning,
                    sheet.id,
                    end,
                    vec![wire.id],
                    &[],
                );
            }
        }
    }
}
