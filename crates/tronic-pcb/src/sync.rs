//! Keeps a board in line with the netlist of the schematic.

use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tronic_schematic::{Netlist, Point, Rotation};

use crate::model::*;

/// What a sync changed and what it could not do.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SyncReport {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub updated: Vec<String>,
    /// Components without an assigned footprint.
    pub no_footprint: Vec<String>,
    /// (reference, footprint id) not found in the library.
    pub unknown_footprint: Vec<(String, String)>,
    /// (reference, pin) without a pad of that number.
    pub missing_pads: Vec<(String, String)>,
}

fn pin_nets(netlist: &Netlist) -> HashMap<&str, BTreeMap<String, String>> {
    let mut map: HashMap<&str, BTreeMap<String, String>> = HashMap::new();
    for net in &netlist.nets {
        for node in &net.nodes {
            map.entry(node.reference.as_str())
                .or_default()
                .insert(node.pin.clone(), net.name.clone());
        }
    }
    map
}

/// Where new parts go: in a column to the right of the board.
struct Parking {
    x: i64,
    y: i64,
    top: i64,
    column_width: i64,
}

impl Parking {
    fn new(board: &Board) -> Self {
        let max_x = board
            .outline
            .iter()
            .map(|p| p.x)
            .chain(board.footprints.iter().map(|f| f.at.x))
            .max()
            .unwrap_or(0);
        let top = board.outline.iter().map(|p| p.y).min().unwrap_or(0);
        Self {
            x: max_x + 10 * MM,
            y: top,
            top,
            column_width: 0,
        }
    }

    fn next(&mut self, definition: &FootprintDef) -> Point {
        let width = definition.courtyard[1].x - definition.courtyard[0].x;
        if self.y - self.top > 150 * MM {
            self.x += self.column_width + 2 * MM;
            self.y = self.top;
            self.column_width = 0;
        }
        self.column_width = self.column_width.max(width);
        // On full millimetres, so that later moves stay on the editing grid.
        let ceil_mm = |v: i64| v.div_euclid(MM) * MM + if v.rem_euclid(MM) > 0 { MM } else { 0 };
        let at = Point::new(
            ceil_mm(self.x - definition.courtyard[0].x),
            ceil_mm(self.y - definition.courtyard[0].y),
        );
        self.y = at.y + definition.courtyard[1].y + 2 * MM;
        at
    }
}

/// Applies the netlist to the board. Running it twice changes nothing more.
pub fn sync_board(
    board: &mut Board,
    netlist: &Netlist,
    find_footprint: &dyn Fn(&str) -> Option<FootprintDef>,
) -> SyncReport {
    let mut report = SyncReport::default();
    let nets = pin_nets(netlist);
    let existing: HashMap<String, usize> = board
        .footprints
        .iter()
        .enumerate()
        .map(|(index, instance)| (instance.reference.clone(), index))
        .collect();

    let mut parking = Parking::new(board);
    let mut result: Vec<FootprintInstance> = Vec::with_capacity(netlist.components.len());
    let empty = BTreeMap::new();

    for component in &netlist.components {
        let Some(wanted) = &component.footprint else {
            report.no_footprint.push(component.reference.clone());
            continue;
        };
        if !board.footprint_defs.contains_key(wanted) {
            match find_footprint(wanted) {
                Some(definition) => {
                    board
                        .footprint_defs
                        .insert(wanted.clone(), Arc::new(definition));
                }
                None => {
                    report
                        .unknown_footprint
                        .push((component.reference.clone(), wanted.clone()));
                    continue;
                }
            }
        }
        let Some(definition) = board.footprint_defs.get(wanted).cloned() else {
            continue;
        };

        let pins = nets.get(component.reference.as_str()).unwrap_or(&empty);
        let mut pad_nets = BTreeMap::new();
        for (pin, net) in pins {
            if definition.pads.iter().any(|pad| pad.number == *pin) {
                pad_nets.insert(pin.clone(), net.clone());
            } else {
                report
                    .missing_pads
                    .push((component.reference.clone(), pin.clone()));
            }
        }

        match existing.get(&component.reference) {
            Some(&index) => {
                let mut instance = board.footprints[index].clone();
                let changed = instance.value != component.value
                    || instance.footprint != *wanted
                    || instance.pad_nets != pad_nets;
                instance.value = component.value.clone();
                instance.footprint = wanted.clone();
                instance.pad_nets = pad_nets;
                if changed {
                    report.updated.push(component.reference.clone());
                }
                result.push(instance);
            }
            None => {
                let id = board.next_id;
                board.next_id += 1;
                result.push(FootprintInstance {
                    id,
                    reference: component.reference.clone(),
                    value: component.value.clone(),
                    footprint: wanted.clone(),
                    at: parking.next(&definition),
                    rotation: Rotation::R0,
                    bottom: false,
                    pad_nets,
                });
                report.added.push(component.reference.clone());
            }
        }
    }

    for instance in board.footprints.iter() {
        if !result
            .iter()
            .any(|kept| kept.reference == instance.reference)
        {
            report.removed.push(instance.reference.clone());
        }
    }
    board.footprints = Arc::new(result);
    report
}

/// A difference between the board and the netlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Deviation {
    MissingFootprint {
        reference: String,
    },
    ExtraFootprint {
        reference: String,
    },
    ValueDiffers {
        reference: String,
    },
    PinNetDiffers {
        reference: String,
        pin: String,
        expected: String,
        actual: Option<String>,
    },
    ExtraPadNet {
        reference: String,
        pad: String,
        net: String,
    },
}

/// Lists every difference; an empty list means the board matches the netlist.
pub fn compare_with_netlist(board: &Board, netlist: &Netlist) -> Vec<Deviation> {
    let mut deviations = Vec::new();
    let nets = pin_nets(netlist);
    let by_reference: HashMap<&str, &FootprintInstance> = board
        .footprints
        .iter()
        .map(|instance| (instance.reference.as_str(), instance))
        .collect();
    let empty = BTreeMap::new();
    for component in &netlist.components {
        let Some(instance) = by_reference.get(component.reference.as_str()) else {
            deviations.push(Deviation::MissingFootprint {
                reference: component.reference.clone(),
            });
            continue;
        };
        if instance.value != component.value {
            deviations.push(Deviation::ValueDiffers {
                reference: component.reference.clone(),
            });
        }
        let pins = nets.get(component.reference.as_str()).unwrap_or(&empty);
        for (pin, expected) in pins {
            let actual = instance.pad_nets.get(pin);
            if actual != Some(expected) {
                deviations.push(Deviation::PinNetDiffers {
                    reference: component.reference.clone(),
                    pin: pin.clone(),
                    expected: expected.clone(),
                    actual: actual.cloned(),
                });
            }
        }
        for (pad, net) in &instance.pad_nets {
            if !pins.contains_key(pad) {
                deviations.push(Deviation::ExtraPadNet {
                    reference: component.reference.clone(),
                    pad: pad.clone(),
                    net: net.clone(),
                });
            }
        }
    }
    for instance in board.footprints.iter() {
        if !netlist
            .components
            .iter()
            .any(|component| component.reference == instance.reference)
        {
            deviations.push(Deviation::ExtraFootprint {
                reference: instance.reference.clone(),
            });
        }
    }
    deviations
}
