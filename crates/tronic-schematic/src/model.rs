//! Persistent schematic data.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use thiserror::Error;

use crate::geometry::{place, Point, Rotation};

pub type ElementId = u64;

/// Pin behaviour, the basis of the electrical rule check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinKind {
    Input,
    Output,
    Bidirectional,
    TriState,
    Passive,
    PowerIn,
    PowerOut,
    OpenCollector,
    OpenEmitter,
    NotConnected,
    /// The pin of a supply symbol.
    SupplyPort,
}

/// Direction from the connection point into the symbol body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinDef {
    pub number: String,
    pub name: String,
    pub kind: PinKind,
    /// Connection point in symbol space.
    pub at: Point,
    pub direction: PinDirection,
    pub length: i64,
    #[serde(default)]
    pub hide_name: bool,
    #[serde(default)]
    pub hide_number: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Graphic {
    Line {
        points: Vec<Point>,
    },
    Polygon {
        points: Vec<Point>,
        filled: bool,
    },
    Rect {
        a: Point,
        b: Point,
    },
    Circle {
        center: Point,
        radius: i64,
        filled: bool,
    },
    Arc {
        center: Point,
        radius: i64,
        start_deg: i32,
        end_deg: i32,
    },
    Text {
        at: Point,
        text: String,
        size: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    #[default]
    Part,
    /// The net name is the component value and applies on all sheets.
    SupplyPort,
}

/// A named limit from a datasheet, for example `Vds max = 60 V`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limit {
    pub parameter: String,
    pub value: String,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PartInfo {
    pub footprint: Option<String>,
    pub model_3d: Option<String>,
    pub manufacturer: Option<String>,
    pub part_number: Option<String>,
    pub datasheet: Option<String>,
    #[serde(default)]
    pub limits: Vec<Limit>,
}

/// Where a symbol comes from and under which terms (IP_COMPLIANCE.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub source: String,
    pub author: String,
    pub license: String,
    /// True once the pins were checked against the datasheet by a person.
    pub reviewed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolDef {
    pub id: String,
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub kind: SymbolKind,
    pub reference_prefix: String,
    #[serde(default)]
    pub default_value: String,
    #[serde(default)]
    pub description: String,
    pub graphics: Vec<Graphic>,
    pub pins: Vec<PinDef>,
    #[serde(default)]
    pub part: PartInfo,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    pub id: ElementId,
    pub symbol: String,
    pub reference: String,
    pub value: String,
    pub at: Point,
    pub rotation: Rotation,
    pub mirror: bool,
    /// Overrides the footprint of the symbol. None uses the symbol's own.
    #[serde(default)]
    pub footprint: Option<String>,
}

/// An orthogonal wire or bus segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub id: ElementId,
    pub a: Point,
    pub b: Point,
}

impl Segment {
    pub fn is_orthogonal(&self) -> bool {
        self.a.x == self.b.x || self.a.y == self.b.y
    }

    /// Whether `p` lies on the segment, including both ends.
    pub fn contains(&self, p: Point) -> bool {
        let within = |v: i64, s: i64, e: i64| v >= s.min(e) && v <= s.max(e);
        if self.a.y == self.b.y {
            p.y == self.a.y && within(p.x, self.a.x, self.b.x)
        } else {
            p.x == self.a.x && within(p.y, self.a.y, self.b.y)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelKind {
    Local,
    Global,
    Hierarchical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub id: ElementId,
    pub at: Point,
    pub name: String,
    pub kind: LabelKind,
    pub rotation: Rotation,
}

/// A point element: no-connect marker or explicit junction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub id: ElementId,
    pub at: Point,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetPort {
    pub name: String,
    /// Relative to the block position.
    pub at: Point,
}

/// A block on a sheet that stands for a sub-sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetInstance {
    pub id: ElementId,
    pub sheet: ElementId,
    pub at: Point,
    pub size: Point,
    pub ports: Vec<SheetPort>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Sheet {
    pub id: ElementId,
    pub name: String,
    /// The sheet holding the block of this sheet. None for top-level sheets.
    pub parent: Option<ElementId>,
    pub components: Vec<Component>,
    pub wires: Vec<Segment>,
    pub buses: Vec<Segment>,
    pub labels: Vec<Label>,
    pub no_connects: Vec<Marker>,
    pub junctions: Vec<Marker>,
    pub instances: Vec<SheetInstance>,
}

/// A complete schematic. Sheets and symbols are shared between undo states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schematic {
    pub next_id: ElementId,
    pub sheets: Vec<Arc<Sheet>>,
    pub symbols: BTreeMap<String, Arc<SymbolDef>>,
}

impl Default for Schematic {
    fn default() -> Self {
        Self {
            next_id: 2,
            sheets: vec![Arc::new(Sheet {
                id: 1,
                name: "1".to_owned(),
                ..Sheet::default()
            })],
            symbols: BTreeMap::new(),
        }
    }
}

/// A file breaks a structural rule of the schematic.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("schematic is inconsistent: {0}")]
pub struct InvalidSchematic(pub String);

impl Schematic {
    pub fn sheet(&self, id: ElementId) -> Option<&Arc<Sheet>> {
        self.sheets.iter().find(|sheet| sheet.id == id)
    }

    /// Connection points of a component's pins, in symbol order.
    /// Empty if the symbol is unknown, which [`Schematic::validate`] rejects.
    pub fn pin_points(&self, component: &Component) -> Vec<Point> {
        self.symbols
            .get(&component.symbol)
            .map(|symbol| {
                symbol
                    .pins
                    .iter()
                    .map(|pin| place(pin.at, component.at, component.rotation, component.mirror))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Sheets ordered as a tree: every sheet directly before its children.
    pub fn sheets_in_tree_order(&self) -> Vec<(usize, &Arc<Sheet>)> {
        fn visit<'a>(
            doc: &'a Schematic,
            parent: Option<ElementId>,
            depth: usize,
            out: &mut Vec<(usize, &'a Arc<Sheet>)>,
        ) {
            for sheet in doc.sheets.iter().filter(|sheet| sheet.parent == parent) {
                out.push((depth, sheet));
                visit(doc, Some(sheet.id), depth + 1, out);
            }
        }
        let mut out = Vec::with_capacity(self.sheets.len());
        visit(self, None, 0, &mut out);
        out
    }

    /// Checks structural rules of data read from outside.
    pub fn validate(&self) -> Result<(), InvalidSchematic> {
        let fail = |message: String| Err(InvalidSchematic(message));
        if self.sheets.is_empty() {
            return fail("no sheets".to_owned());
        }
        let mut seen = HashSet::new();
        let mut register = |id: ElementId| -> Result<(), InvalidSchematic> {
            if id >= self.next_id || !seen.insert(id) {
                return Err(InvalidSchematic(format!("bad element id {id}")));
            }
            Ok(())
        };
        for sheet in &self.sheets {
            register(sheet.id)?;
        }
        let mut instantiated = HashSet::new();
        for sheet in &self.sheets {
            for component in &sheet.components {
                register(component.id)?;
                if !self.symbols.contains_key(&component.symbol) {
                    return fail(format!("unknown symbol {}", component.symbol));
                }
            }
            for segment in sheet.wires.iter().chain(&sheet.buses) {
                register(segment.id)?;
                if !segment.is_orthogonal() {
                    return fail(format!("segment {} is not orthogonal", segment.id));
                }
            }
            for label in &sheet.labels {
                register(label.id)?;
            }
            for marker in sheet.no_connects.iter().chain(&sheet.junctions) {
                register(marker.id)?;
            }
            for instance in &sheet.instances {
                register(instance.id)?;
                let Some(child) = self.sheet(instance.sheet) else {
                    return fail(format!("block {} has no sheet", instance.id));
                };
                if child.parent != Some(sheet.id) || !instantiated.insert(instance.sheet) {
                    return fail(format!("sheet {} is wrongly linked", instance.sheet));
                }
            }
        }
        for sheet in &self.sheets {
            if sheet.parent.is_some() && !instantiated.contains(&sheet.id) {
                return fail(format!("sheet {} has no block", sheet.id));
            }
        }
        // Parent cycles would leave sheets unreachable from the top level.
        if self.sheets_in_tree_order().len() != self.sheets.len() {
            return fail("sheet hierarchy contains a cycle".to_owned());
        }
        for symbol in self.symbols.values() {
            validate_symbol(symbol).map_err(InvalidSchematic)?;
        }
        Ok(())
    }
}

/// Rules every symbol must satisfy, also for imported libraries.
pub fn validate_symbol(symbol: &SymbolDef) -> Result<(), String> {
    if symbol.id.trim().is_empty() || symbol.name.trim().is_empty() {
        return Err("symbol without id or name".to_owned());
    }
    if symbol.pins.is_empty() {
        return Err(format!("symbol {} has no pins", symbol.id));
    }
    let mut numbers = HashSet::new();
    for pin in &symbol.pins {
        if !numbers.insert(&pin.number) {
            return Err(format!("symbol {} repeats pin {}", symbol.id, pin.number));
        }
    }
    Ok(())
}
