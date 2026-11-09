//! All changes to a schematic, as atomic commands with undo.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;

use crate::connectivity::{parse_bus_name, SegmentIndex};
use crate::geometry::{Point, Rotation, Transform, GRID};
use crate::model::*;
use crate::normalize::normalize_segments;

const MAX_UNDO: usize = 200;
const MAX_TEXT_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditError {
    #[error("unknown sheet")]
    UnknownSheet,
    #[error("unknown element")]
    UnknownElement,
    #[error("unknown symbol {0}")]
    UnknownSymbol(String),
    #[error("segment is not horizontal or vertical")]
    NotOrthogonal,
    #[error("text must not be empty")]
    EmptyText,
    #[error("text is too long or contains control characters")]
    InvalidText,
    #[error("the only top-level sheet cannot be deleted")]
    LastSheet,
    #[error("a sheet with a block is deleted by deleting the block")]
    SheetHasBlock,
    #[error("a sheet with this name exists")]
    DuplicateSheetName,
    #[error("nothing to undo or redo")]
    NothingToRestore,
}

/// Elements on the clipboard. Ids and references are reassigned on paste.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Clip {
    #[serde(default)]
    pub components: Vec<Component>,
    #[serde(default)]
    pub wires: Vec<Segment>,
    #[serde(default)]
    pub buses: Vec<Segment>,
    #[serde(default)]
    pub labels: Vec<Label>,
    #[serde(default)]
    pub no_connects: Vec<Marker>,
    #[serde(default)]
    pub junctions: Vec<Marker>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EditCommand {
    PlaceComponent {
        sheet: ElementId,
        symbol: String,
        at: Point,
        #[serde(default)]
        rotation: Rotation,
        #[serde(default)]
        mirror: bool,
    },
    AddWires {
        sheet: ElementId,
        points: Vec<Point>,
    },
    AddBuses {
        sheet: ElementId,
        points: Vec<Point>,
    },
    AddLabel {
        sheet: ElementId,
        at: Point,
        name: String,
        kind: LabelKind,
    },
    AddNoConnect {
        sheet: ElementId,
        at: Point,
    },
    AddJunction {
        sheet: ElementId,
        at: Point,
    },
    AddSheet {
        name: String,
    },
    AddBlock {
        sheet: ElementId,
        at: Point,
        name: String,
    },
    RenameSheet {
        sheet: ElementId,
        name: String,
    },
    DeleteSheet {
        sheet: ElementId,
    },
    Move {
        sheet: ElementId,
        ids: Vec<ElementId>,
        delta: Point,
    },
    Rotate {
        sheet: ElementId,
        ids: Vec<ElementId>,
    },
    Mirror {
        sheet: ElementId,
        ids: Vec<ElementId>,
    },
    Delete {
        sheet: ElementId,
        ids: Vec<ElementId>,
    },
    SetComponent {
        sheet: ElementId,
        id: ElementId,
        reference: String,
        value: String,
    },
    SetFootprint {
        sheet: ElementId,
        id: ElementId,
        footprint: Option<String>,
    },
    SetLabel {
        sheet: ElementId,
        id: ElementId,
        name: String,
    },
    Paste {
        sheet: ElementId,
        clip: Clip,
        delta: Point,
    },
    Annotate,
}

/// What a command produced, for selecting new elements.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Applied {
    pub created: Vec<ElementId>,
}

/// A schematic with its undo history.
#[derive(Debug, Clone, Default)]
pub struct Editor {
    doc: Schematic,
    undo: Vec<Schematic>,
    redo: Vec<Schematic>,
}

impl Editor {
    pub fn new(doc: Schematic) -> Self {
        Self {
            doc,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn doc(&self) -> &Schematic {
        &self.doc
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Makes symbols available to the document. Not an undo step: an unused
    /// symbol changes nothing electrically.
    pub fn add_symbols(&mut self, symbols: impl IntoIterator<Item = SymbolDef>) {
        for symbol in symbols {
            self.doc.symbols.insert(symbol.id.clone(), Arc::new(symbol));
        }
    }

    /// Runs a command. On error the document is unchanged.
    pub fn apply(&mut self, command: EditCommand) -> Result<Applied, EditError> {
        let before = self.doc.clone();
        match run(&mut self.doc, command) {
            Ok(applied) => {
                if self.doc != before {
                    self.undo.push(before);
                    if self.undo.len() > MAX_UNDO {
                        self.undo.remove(0);
                    }
                    self.redo.clear();
                }
                Ok(applied)
            }
            Err(error) => {
                self.doc = before;
                Err(error)
            }
        }
    }

    /// Takes over a document worked out elsewhere, as one undo step.
    pub fn commit(&mut self, doc: Schematic) {
        if doc != self.doc {
            self.undo.push(std::mem::replace(&mut self.doc, doc));
            if self.undo.len() > MAX_UNDO {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }

    pub fn undo(&mut self) -> Result<(), EditError> {
        let state = self.undo.pop().ok_or(EditError::NothingToRestore)?;
        self.redo.push(std::mem::replace(&mut self.doc, state));
        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), EditError> {
        let state = self.redo.pop().ok_or(EditError::NothingToRestore)?;
        self.undo.push(std::mem::replace(&mut self.doc, state));
        Ok(())
    }
}

fn check_text(text: &str) -> Result<String, EditError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(EditError::EmptyText);
    }
    if text.chars().count() > MAX_TEXT_CHARS || text.chars().any(char::is_control) {
        return Err(EditError::InvalidText);
    }
    Ok(text.to_owned())
}

/// Sheet names appear in net names, which must stay unique.
fn check_sheet_name(
    doc: &Schematic,
    name: &str,
    own: Option<ElementId>,
) -> Result<String, EditError> {
    let name = check_text(name)?;
    if doc
        .sheets
        .iter()
        .any(|sheet| sheet.name == name && Some(sheet.id) != own)
    {
        return Err(EditError::DuplicateSheetName);
    }
    Ok(name)
}

fn allocate_id(doc: &mut Schematic) -> ElementId {
    let id = doc.next_id;
    doc.next_id += 1;
    id
}

fn sheet_mut(doc: &mut Schematic, id: ElementId) -> Result<&mut Sheet, EditError> {
    doc.sheets
        .iter_mut()
        .find(|sheet| sheet.id == id)
        .map(Arc::make_mut)
        .ok_or(EditError::UnknownSheet)
}

/// Hands out the lowest free number per prefix.
struct ReferenceAllocator(HashMap<String, HashSet<u64>>);

impl ReferenceAllocator {
    fn new(doc: &Schematic) -> Self {
        let mut used: HashMap<String, HashSet<u64>> = HashMap::new();
        for sheet in &doc.sheets {
            for component in &sheet.components {
                if let Some((prefix, number)) = split_reference(&component.reference) {
                    used.entry(prefix.to_owned()).or_default().insert(number);
                }
            }
        }
        Self(used)
    }

    fn next(&mut self, prefix: &str) -> String {
        let used = self.0.entry(prefix.to_owned()).or_default();
        let number = (1u64..).find(|n| !used.contains(n)).unwrap_or(1);
        used.insert(number);
        format!("{prefix}{number}")
    }
}

fn split_reference(reference: &str) -> Option<(&str, u64)> {
    let digits = reference.trim_end_matches(|c: char| c.is_ascii_digit());
    let number = reference[digits.len()..].parse().ok()?;
    Some((digits, number))
}

fn run(doc: &mut Schematic, command: EditCommand) -> Result<Applied, EditError> {
    let mut applied = Applied::default();
    let mut touched = None;
    match command {
        EditCommand::PlaceComponent {
            sheet,
            symbol,
            at,
            rotation,
            mirror,
        } => {
            let definition = doc
                .symbols
                .get(&symbol)
                .cloned()
                .ok_or(EditError::UnknownSymbol(symbol.clone()))?;
            let reference = ReferenceAllocator::new(doc).next(&definition.reference_prefix);
            let id = allocate_id(doc);
            sheet_mut(doc, sheet)?.components.push(Component {
                id,
                symbol,
                reference,
                value: definition.default_value.clone(),
                at,
                rotation,
                mirror,
                footprint: None,
            });
            applied.created.push(id);
            touched = Some(sheet);
        }
        EditCommand::AddWires { sheet, points } => {
            applied.created = add_segments(doc, sheet, &points, false)?;
            touched = Some(sheet);
        }
        EditCommand::AddBuses { sheet, points } => {
            applied.created = add_segments(doc, sheet, &points, true)?;
            touched = Some(sheet);
        }
        EditCommand::AddLabel {
            sheet,
            at,
            name,
            kind,
        } => {
            let name = check_text(&name)?;
            let id = allocate_id(doc);
            sheet_mut(doc, sheet)?.labels.push(Label {
                id,
                at,
                name,
                kind,
                rotation: Rotation::R0,
            });
            applied.created.push(id);
        }
        EditCommand::AddNoConnect { sheet, at } => {
            let id = allocate_id(doc);
            sheet_mut(doc, sheet)?.no_connects.push(Marker { id, at });
            applied.created.push(id);
        }
        EditCommand::AddJunction { sheet, at } => {
            let id = allocate_id(doc);
            sheet_mut(doc, sheet)?.junctions.push(Marker { id, at });
            applied.created.push(id);
        }
        EditCommand::AddSheet { name } => {
            let name = check_sheet_name(doc, &name, None)?;
            let id = allocate_id(doc);
            doc.sheets.push(Arc::new(Sheet {
                id,
                name,
                ..Sheet::default()
            }));
            applied.created.push(id);
        }
        EditCommand::AddBlock { sheet, at, name } => {
            let name = check_sheet_name(doc, &name, None)?;
            doc.sheet(sheet).ok_or(EditError::UnknownSheet)?;
            let child = allocate_id(doc);
            let block = allocate_id(doc);
            doc.sheets.push(Arc::new(Sheet {
                id: child,
                name,
                parent: Some(sheet),
                ..Sheet::default()
            }));
            sheet_mut(doc, sheet)?.instances.push(SheetInstance {
                id: block,
                sheet: child,
                at,
                size: Point::grid(16, 12),
                ports: Vec::new(),
            });
            applied.created.push(block);
        }
        EditCommand::RenameSheet { sheet, name } => {
            let name = check_sheet_name(doc, &name, Some(sheet))?;
            sheet_mut(doc, sheet)?.name = name;
        }
        EditCommand::DeleteSheet { sheet } => {
            let target = doc.sheet(sheet).ok_or(EditError::UnknownSheet)?;
            if target.parent.is_some() {
                return Err(EditError::SheetHasBlock);
            }
            if doc.sheets.iter().filter(|s| s.parent.is_none()).count() <= 1 {
                return Err(EditError::LastSheet);
            }
            remove_sheet_tree(doc, sheet);
        }
        EditCommand::Move { sheet, ids, delta } => {
            if delta != Point::default() {
                transform(doc, sheet, &ids, Transform::Translate(delta))?;
                touched = Some(sheet);
            }
        }
        EditCommand::Rotate { sheet, ids } => {
            let pivot = pivot_of(doc, sheet, &ids)?;
            transform(doc, sheet, &ids, Transform::RotateClockwise(pivot))?;
            touched = Some(sheet);
        }
        EditCommand::Mirror { sheet, ids } => {
            let pivot = pivot_of(doc, sheet, &ids)?;
            transform(doc, sheet, &ids, Transform::MirrorX(pivot))?;
            touched = Some(sheet);
        }
        EditCommand::Delete { sheet, ids } => {
            delete_elements(doc, sheet, &ids)?;
        }
        EditCommand::SetComponent {
            sheet,
            id,
            reference,
            value,
        } => {
            let reference = check_text(&reference)?;
            if value.chars().count() > MAX_TEXT_CHARS || value.chars().any(char::is_control) {
                return Err(EditError::InvalidText);
            }
            let component = sheet_mut(doc, sheet)?
                .components
                .iter_mut()
                .find(|component| component.id == id)
                .ok_or(EditError::UnknownElement)?;
            component.reference = reference;
            component.value = value.trim().to_owned();
        }
        EditCommand::SetFootprint {
            sheet,
            id,
            footprint,
        } => {
            sheet_mut(doc, sheet)?
                .components
                .iter_mut()
                .find(|component| component.id == id)
                .ok_or(EditError::UnknownElement)?
                .footprint = footprint;
        }
        EditCommand::SetLabel { sheet, id, name } => {
            let name = check_text(&name)?;
            sheet_mut(doc, sheet)?
                .labels
                .iter_mut()
                .find(|label| label.id == id)
                .ok_or(EditError::UnknownElement)?
                .name = name;
        }
        EditCommand::Paste { sheet, clip, delta } => {
            applied.created = paste(doc, sheet, clip, delta)?;
            touched = Some(sheet);
        }
        EditCommand::Annotate => annotate(doc),
    }
    if let Some(sheet) = touched {
        let target = sheet_mut(doc, sheet)?;
        normalize_segments(&mut target.wires);
        normalize_segments(&mut target.buses);
    }
    sync_ports(doc);
    Ok(applied)
}

fn add_segments(
    doc: &mut Schematic,
    sheet: ElementId,
    points: &[Point],
    bus: bool,
) -> Result<Vec<ElementId>, EditError> {
    doc.sheet(sheet).ok_or(EditError::UnknownSheet)?;
    let mut created = Vec::new();
    for pair in points.windows(2) {
        let id = allocate_id(doc);
        let segment = Segment {
            id,
            a: pair[0],
            b: pair[1],
        };
        if !segment.is_orthogonal() {
            return Err(EditError::NotOrthogonal);
        }
        let target = sheet_mut(doc, sheet)?;
        if bus {
            target.buses.push(segment);
        } else {
            target.wires.push(segment);
        }
        created.push(id);
    }
    Ok(created)
}

fn remove_sheet_tree(doc: &mut Schematic, root: ElementId) {
    let mut remove = HashSet::from([root]);
    loop {
        let before = remove.len();
        for sheet in &doc.sheets {
            if sheet.parent.is_some_and(|parent| remove.contains(&parent)) {
                remove.insert(sheet.id);
            }
        }
        if remove.len() == before {
            break;
        }
    }
    doc.sheets.retain(|sheet| !remove.contains(&sheet.id));
    for sheet in &mut doc.sheets {
        if sheet.instances.iter().any(|i| remove.contains(&i.sheet)) {
            Arc::make_mut(sheet)
                .instances
                .retain(|instance| !remove.contains(&instance.sheet));
        }
    }
}

fn delete_elements(
    doc: &mut Schematic,
    sheet: ElementId,
    ids: &[ElementId],
) -> Result<(), EditError> {
    let ids: HashSet<ElementId> = ids.iter().copied().collect();
    let blocks: Vec<ElementId> = doc
        .sheet(sheet)
        .ok_or(EditError::UnknownSheet)?
        .instances
        .iter()
        .filter(|instance| ids.contains(&instance.id))
        .map(|instance| instance.sheet)
        .collect();
    let target = sheet_mut(doc, sheet)?;
    target.components.retain(|e| !ids.contains(&e.id));
    target.wires.retain(|e| !ids.contains(&e.id));
    target.buses.retain(|e| !ids.contains(&e.id));
    target.labels.retain(|e| !ids.contains(&e.id));
    target.no_connects.retain(|e| !ids.contains(&e.id));
    target.junctions.retain(|e| !ids.contains(&e.id));
    for child in blocks {
        remove_sheet_tree(doc, child);
    }
    Ok(())
}

/// The grid point nearest to the centre of the selected elements.
fn pivot_of(doc: &Schematic, sheet: ElementId, ids: &[ElementId]) -> Result<Point, EditError> {
    let sheet = doc.sheet(sheet).ok_or(EditError::UnknownSheet)?;
    let ids: HashSet<ElementId> = ids.iter().copied().collect();
    let mut points = Vec::new();
    points.extend(
        sheet
            .components
            .iter()
            .filter(|e| ids.contains(&e.id))
            .map(|e| e.at),
    );
    for segment in sheet.wires.iter().chain(&sheet.buses) {
        if ids.contains(&segment.id) {
            points.push(segment.a);
            points.push(segment.b);
        }
    }
    points.extend(
        sheet
            .labels
            .iter()
            .filter(|e| ids.contains(&e.id))
            .map(|e| e.at),
    );
    for marker in sheet.no_connects.iter().chain(&sheet.junctions) {
        if ids.contains(&marker.id) {
            points.push(marker.at);
        }
    }
    if points.is_empty() {
        return Err(EditError::UnknownElement);
    }
    let count = points.len() as i64;
    let sum = points
        .iter()
        .fold((0i64, 0i64), |acc, p| (acc.0 + p.x, acc.1 + p.y));
    Ok(Point::new(sum.0 / count, sum.1 / count).snapped())
}

/// Moves, rotates or mirrors the selection. Where a moved pin or wire end
/// touched something that stays in place, a new wire keeps the connection.
fn transform(
    doc: &mut Schematic,
    sheet_id: ElementId,
    ids: &[ElementId],
    transformation: Transform,
) -> Result<(), EditError> {
    let ids: HashSet<ElementId> = ids.iter().copied().collect();
    let sheet = doc.sheet(sheet_id).ok_or(EditError::UnknownSheet)?.clone();

    let staying_wires = sheet.wires.iter().filter(|wire| !ids.contains(&wire.id));
    let staying_index = SegmentIndex::new(staying_wires);
    let mut staying_pins: HashSet<Point> = HashSet::new();
    let mut moved_pins: Vec<(ElementId, Vec<Point>)> = Vec::new();
    for component in &sheet.components {
        let pins = doc.pin_points(component);
        if ids.contains(&component.id) {
            moved_pins.push((component.id, pins));
        } else {
            staying_pins.extend(pins);
        }
    }
    let touches_staying =
        |p: Point| staying_pins.contains(&p) || staying_index.at(p).next().is_some();

    // Old connection points with their new position.
    let mut bridges: Vec<(Point, Point)> = Vec::new();
    for wire in sheet.wires.iter().filter(|wire| ids.contains(&wire.id)) {
        for end in [wire.a, wire.b] {
            if touches_staying(end) {
                bridges.push((end, transformation.point(end)));
            }
        }
    }

    {
        let target = sheet_mut(doc, sheet_id)?;
        for component in &mut target.components {
            if ids.contains(&component.id) {
                component.at = transformation.point(component.at);
                let (rotation, mirror) =
                    transformation.orientation(component.rotation, component.mirror);
                component.rotation = rotation;
                component.mirror = mirror;
            }
        }
        for segment in target.wires.iter_mut().chain(target.buses.iter_mut()) {
            if ids.contains(&segment.id) {
                segment.a = transformation.point(segment.a);
                segment.b = transformation.point(segment.b);
            }
        }
        for label in &mut target.labels {
            if ids.contains(&label.id) {
                label.at = transformation.point(label.at);
                label.rotation = transformation.orientation(label.rotation, false).0;
            }
        }
        for marker in target
            .no_connects
            .iter_mut()
            .chain(target.junctions.iter_mut())
        {
            if ids.contains(&marker.id) {
                marker.at = transformation.point(marker.at);
            }
        }
        if let Transform::Translate(delta) = transformation {
            for instance in &mut target.instances {
                if ids.contains(&instance.id) {
                    instance.at = instance.at.offset(delta);
                }
            }
        }
    }

    for (id, old_pins) in &moved_pins {
        let new_pins = doc
            .sheet(sheet_id)
            .and_then(|s| s.components.iter().find(|c| c.id == *id))
            .map(|component| doc.pin_points(component))
            .unwrap_or_default();
        for (old, new) in old_pins.iter().zip(new_pins) {
            if touches_staying(*old) {
                bridges.push((*old, new));
            }
        }
    }

    for (old, new) in bridges {
        if old == new {
            continue;
        }
        let corner = Point::new(new.x, old.y);
        let path = if corner == old || corner == new {
            vec![old, new]
        } else {
            vec![old, corner, new]
        };
        add_segments(doc, sheet_id, &path, false)?;
    }
    Ok(())
}

fn paste(
    doc: &mut Schematic,
    sheet: ElementId,
    clip: Clip,
    delta: Point,
) -> Result<Vec<ElementId>, EditError> {
    doc.sheet(sheet).ok_or(EditError::UnknownSheet)?;
    let mut allocator = ReferenceAllocator::new(doc);
    let mut created = Vec::new();
    let mut components = Vec::with_capacity(clip.components.len());
    for mut component in clip.components {
        let symbol = doc
            .symbols
            .get(&component.symbol)
            .ok_or_else(|| EditError::UnknownSymbol(component.symbol.clone()))?;
        component.reference = allocator.next(&symbol.reference_prefix);
        component.at = component.at.offset(delta);
        component.id = allocate_id(doc);
        created.push(component.id);
        components.push(component);
    }
    let mut segments = |list: Vec<Segment>| -> Result<Vec<Segment>, EditError> {
        list.into_iter()
            .map(|mut segment| {
                if !segment.is_orthogonal() {
                    return Err(EditError::NotOrthogonal);
                }
                segment.id = allocate_id(doc);
                segment.a = segment.a.offset(delta);
                segment.b = segment.b.offset(delta);
                created.push(segment.id);
                Ok(segment)
            })
            .collect()
    };
    let wires = segments(clip.wires)?;
    let buses = segments(clip.buses)?;
    let mut labels = Vec::with_capacity(clip.labels.len());
    for mut label in clip.labels {
        label.name = check_text(&label.name)?;
        label.id = allocate_id(doc);
        label.at = label.at.offset(delta);
        created.push(label.id);
        labels.push(label);
    }
    let mut markers = |list: Vec<Marker>| -> Vec<Marker> {
        list.into_iter()
            .map(|mut marker| {
                marker.id = allocate_id(doc);
                marker.at = marker.at.offset(delta);
                created.push(marker.id);
                marker
            })
            .collect()
    };
    let no_connects = markers(clip.no_connects);
    let junctions = markers(clip.junctions);

    let target = sheet_mut(doc, sheet)?;
    target.components.extend(components);
    target.wires.extend(wires);
    target.buses.extend(buses);
    target.labels.extend(labels);
    target.no_connects.extend(no_connects);
    target.junctions.extend(junctions);
    Ok(created)
}

/// Renumbers all parts in sheet order, top to bottom, left to right.
fn annotate(doc: &mut Schematic) {
    let order: Vec<ElementId> = doc
        .sheets_in_tree_order()
        .into_iter()
        .map(|(_, sheet)| sheet.id)
        .collect();
    let symbols = doc.symbols.clone();
    let mut counters: BTreeMap<String, u64> = BTreeMap::new();
    for id in order {
        let Ok(sheet) = sheet_mut(doc, id) else {
            continue;
        };
        let mut indices: Vec<usize> = (0..sheet.components.len()).collect();
        indices.sort_by_key(|&i| (sheet.components[i].at.y, sheet.components[i].at.x));
        for index in indices {
            let component = &mut sheet.components[index];
            let Some(symbol) = symbols.get(&component.symbol) else {
                continue;
            };
            let counter = counters.entry(symbol.reference_prefix.clone()).or_default();
            *counter += 1;
            component.reference = format!("{}{}", symbol.reference_prefix, counter);
        }
    }
}

/// Keeps block ports in line with the hierarchical labels of their sub-sheet.
/// Existing ports stay where they are so that wires do not come off.
fn sync_ports(doc: &mut Schematic) {
    let mut wanted: HashMap<ElementId, Vec<String>> = HashMap::new();
    for sheet in &doc.sheets {
        if sheet.parent.is_some() {
            let mut names: Vec<String> = sheet
                .labels
                .iter()
                .filter(|label| {
                    label.kind == LabelKind::Hierarchical && parse_bus_name(&label.name).is_none()
                })
                .map(|label| label.name.clone())
                .collect();
            names.sort();
            names.dedup();
            wanted.insert(sheet.id, names);
        }
    }
    for sheet in &mut doc.sheets {
        let up_to_date = sheet.instances.iter().all(|instance| {
            let names = wanted.get(&instance.sheet);
            let names = names.map_or(&[][..], Vec::as_slice);
            instance.ports.len() == names.len()
                && instance.ports.iter().all(|port| names.contains(&port.name))
        });
        if up_to_date {
            continue;
        }
        for instance in &mut Arc::make_mut(sheet).instances {
            let names = wanted.get(&instance.sheet).cloned().unwrap_or_default();
            instance.ports.retain(|port| names.contains(&port.name));
            for name in names {
                if instance.ports.iter().any(|port| port.name == name) {
                    continue;
                }
                let slot = (1i64..)
                    .map(|i| Point::new(0, i * 2 * GRID))
                    .find(|at| instance.ports.iter().all(|port| port.at != *at))
                    .unwrap_or_default();
                instance.ports.push(SheetPort { name, at: slot });
                instance.size.y = instance.size.y.max(slot.y + 2 * GRID);
            }
        }
    }
}
