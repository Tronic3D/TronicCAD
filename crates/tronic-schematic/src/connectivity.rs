//! Turns drawn elements into electrical nets.

use serde::Serialize;
use std::collections::{BTreeSet, HashMap};

use crate::geometry::Point;
use crate::model::*;

/// Finds wires by a point in time proportional to the wires on that line.
#[derive(Default)]
pub struct SegmentIndex {
    horizontal: HashMap<i64, Vec<Segment>>,
    vertical: HashMap<i64, Vec<Segment>>,
}

impl SegmentIndex {
    pub fn new<'a>(segments: impl IntoIterator<Item = &'a Segment>) -> Self {
        let mut index = Self::default();
        for segment in segments {
            if segment.a.y == segment.b.y {
                index
                    .horizontal
                    .entry(segment.a.y)
                    .or_default()
                    .push(*segment);
            } else {
                index
                    .vertical
                    .entry(segment.a.x)
                    .or_default()
                    .push(*segment);
            }
        }
        index
    }

    /// All segments containing `p`.
    pub fn at(&self, p: Point) -> impl Iterator<Item = &Segment> {
        let row = self.horizontal.get(&p.y).into_iter().flatten();
        let column = self.vertical.get(&p.x).into_iter().flatten();
        row.chain(column).filter(move |segment| segment.contains(p))
    }
}

/// Base name and member signals of a bus, or `Err` if the name is malformed.
pub type ParsedBus = Result<(String, Vec<String>), ()>;

/// Whether a label name denotes a bus, like `D[0..7]`.
pub fn parse_bus_name(name: &str) -> Option<ParsedBus> {
    let open = name.find('[')?;
    let inner = name.get(open + 1..)?.strip_suffix(']');
    let parsed = inner.and_then(|inner| {
        let (start, end) = inner.split_once("..")?;
        let start: u32 = start.parse().ok()?;
        let end: u32 = end.parse().ok()?;
        let base = &name[..open];
        if base.is_empty() || start.abs_diff(end) >= 1024 {
            return None;
        }
        let range: Vec<u32> = if start <= end {
            (start..=end).collect()
        } else {
            (end..=start).rev().collect()
        };
        Some((
            base.to_owned(),
            range.iter().map(|i| format!("{base}{i}")).collect(),
        ))
    });
    Some(parsed.ok_or(()))
}

/// One pin in a net.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PinNode {
    pub sheet: ElementId,
    pub component: ElementId,
    pub reference: String,
    pub pin: usize,
    pub number: String,
    pub name: String,
    pub kind: PinKind,
    pub at: Point,
}

/// A non-pin element in a net.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetAttachment {
    pub sheet: ElementId,
    pub id: ElementId,
    pub at: Point,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Net {
    pub name: String,
    pub pins: Vec<PinNode>,
    pub wires: Vec<NetAttachment>,
    pub labels: Vec<NetAttachment>,
    pub no_connects: Vec<NetAttachment>,
    /// Ports of sheet blocks, as (block, port index).
    pub ports: Vec<(NetAttachment, usize)>,
    /// Distinct values of the supply symbols on this net.
    pub supply_names: BTreeSet<String>,
    /// Distinct names given by labels; local ones in `/Sheet/Name` form.
    pub label_names: BTreeSet<String>,
    pub sheets: BTreeSet<ElementId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Element {
    Pin(ElementId, usize),
    Wire(ElementId),
    Label(ElementId),
    NoConnect(ElementId),
    Port(ElementId, usize),
    Junction,
}

struct Node {
    sheet: ElementId,
    element: Element,
    at: Point,
}

struct UnionFind(Vec<usize>);

impl UnionFind {
    fn find(&mut self, mut a: usize) -> usize {
        while self.0[a] != a {
            self.0[a] = self.0[self.0[a]];
            a = self.0[a];
        }
        a
    }

    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a.max(b)] = a.min(b);
        }
    }
}

/// The electrical interpretation of a schematic.
#[derive(Debug, Clone, Default)]
pub struct Connectivity {
    pub nets: Vec<Net>,
    by_element: HashMap<(ElementId, ElementId), usize>,
}

impl Connectivity {
    pub fn analyze(doc: &Schematic) -> Self {
        let mut nodes: Vec<Node> = Vec::new();
        let mut name_groups: HashMap<(Option<ElementId>, String), usize> = HashMap::new();
        let mut pending_unions: Vec<(usize, usize)> = Vec::new();

        let sheet_names: HashMap<ElementId, &str> =
            doc.sheets.iter().map(|s| (s.id, s.name.as_str())).collect();

        // Sheet-level geometry.
        for sheet in &doc.sheets {
            let sid = sheet.id;
            let wire_start = nodes.len();
            let mut wire_node: HashMap<ElementId, usize> =
                HashMap::with_capacity(sheet.wires.len());
            for wire in &sheet.wires {
                wire_node.insert(wire.id, nodes.len());
                nodes.push(Node {
                    sheet: sid,
                    element: Element::Wire(wire.id),
                    at: wire.a,
                });
            }
            debug_assert_eq!(nodes.len() - wire_start, sheet.wires.len());
            let index = SegmentIndex::new(&sheet.wires);
            let mut at_point: HashMap<Point, usize> = HashMap::new();
            let attach = |node: usize,
                          at: Point,
                          at_point: &mut HashMap<Point, usize>,
                          pending: &mut Vec<(usize, usize)>| {
                match at_point.get(&at) {
                    Some(&other) => pending.push((node, other)),
                    None => {
                        at_point.insert(at, node);
                    }
                }
                for wire in index.at(at) {
                    pending.push((node, wire_node[&wire.id]));
                }
            };

            for wire in &sheet.wires {
                let node = wire_node[&wire.id];
                attach(node, wire.a, &mut at_point, &mut pending_unions);
                attach(node, wire.b, &mut at_point, &mut pending_unions);
            }
            for component in &sheet.components {
                for (pin, at) in doc.pin_points(component).into_iter().enumerate() {
                    let node = nodes.len();
                    nodes.push(Node {
                        sheet: sid,
                        element: Element::Pin(component.id, pin),
                        at,
                    });
                    attach(node, at, &mut at_point, &mut pending_unions);
                    let is_supply = doc
                        .symbols
                        .get(&component.symbol)
                        .is_some_and(|symbol| symbol.kind == SymbolKind::SupplyPort);
                    if is_supply && !component.value.is_empty() {
                        // Same group as a global label of that name: two nets
                        // must never carry one name.
                        let group = (None, format!("label:{}", component.value));
                        match name_groups.get(&group) {
                            Some(&other) => pending_unions.push((node, other)),
                            None => {
                                name_groups.insert(group, node);
                            }
                        }
                    }
                }
            }
            for label in &sheet.labels {
                // Bus labels name the bus, they do not form a net.
                if parse_bus_name(&label.name).is_some() {
                    continue;
                }
                let node = nodes.len();
                nodes.push(Node {
                    sheet: sid,
                    element: Element::Label(label.id),
                    at: label.at,
                });
                attach(node, label.at, &mut at_point, &mut pending_unions);
                let scope = match label.kind {
                    LabelKind::Global => None,
                    LabelKind::Local | LabelKind::Hierarchical => Some(sid),
                };
                let group = (scope, format!("label:{}", label.name));
                match name_groups.get(&group) {
                    Some(&other) => pending_unions.push((node, other)),
                    None => {
                        name_groups.insert(group, node);
                    }
                }
            }
            for marker in &sheet.no_connects {
                let node = nodes.len();
                nodes.push(Node {
                    sheet: sid,
                    element: Element::NoConnect(marker.id),
                    at: marker.at,
                });
                attach(node, marker.at, &mut at_point, &mut pending_unions);
            }
            for marker in &sheet.junctions {
                let node = nodes.len();
                nodes.push(Node {
                    sheet: sid,
                    element: Element::Junction,
                    at: marker.at,
                });
                attach(node, marker.at, &mut at_point, &mut pending_unions);
            }
            for instance in &sheet.instances {
                for (port, definition) in instance.ports.iter().enumerate() {
                    let node = nodes.len();
                    let at = instance.at.offset(definition.at);
                    nodes.push(Node {
                        sheet: sid,
                        element: Element::Port(instance.id, port),
                        at,
                    });
                    attach(node, at, &mut at_point, &mut pending_unions);
                    // Joined with the sub-sheet's label of the same name.
                    let group = (Some(instance.sheet), format!("label:{}", definition.name));
                    match name_groups.get(&group) {
                        Some(&other) => pending_unions.push((node, other)),
                        None => {
                            name_groups.insert(group, node);
                        }
                    }
                }
            }
        }

        let mut groups = UnionFind((0..nodes.len()).collect());
        for (a, b) in pending_unions {
            groups.union(a, b);
        }

        // Lookups for building the nets.
        let mut components: HashMap<ElementId, (&Component, &SymbolDef)> = HashMap::new();
        let mut labels: HashMap<ElementId, &Label> = HashMap::new();
        let mut instances: HashMap<ElementId, &SheetInstance> = HashMap::new();
        for sheet in &doc.sheets {
            for component in &sheet.components {
                if let Some(symbol) = doc.symbols.get(&component.symbol) {
                    components.insert(component.id, (component, symbol));
                }
            }
            for label in &sheet.labels {
                labels.insert(label.id, label);
            }
            for instance in &sheet.instances {
                instances.insert(instance.id, instance);
            }
        }

        let mut net_of_root: HashMap<usize, usize> = HashMap::new();
        let mut nets: Vec<Net> = Vec::new();
        let mut global_names: Vec<BTreeSet<String>> = Vec::new();
        let mut by_element = HashMap::new();
        for (index, node) in nodes.iter().enumerate() {
            let root = groups.find(index);
            let net_index = *net_of_root.entry(root).or_insert_with(|| {
                nets.push(Net {
                    name: String::new(),
                    pins: Vec::new(),
                    wires: Vec::new(),
                    labels: Vec::new(),
                    no_connects: Vec::new(),
                    ports: Vec::new(),
                    supply_names: BTreeSet::new(),
                    label_names: BTreeSet::new(),
                    sheets: BTreeSet::new(),
                });
                global_names.push(BTreeSet::new());
                nets.len() - 1
            });
            let net = &mut nets[net_index];
            net.sheets.insert(node.sheet);
            let attachment = |id: ElementId| NetAttachment {
                sheet: node.sheet,
                id,
                at: node.at,
            };
            match node.element {
                Element::Pin(id, pin) => {
                    by_element.insert((node.sheet, id), net_index);
                    let Some((component, symbol)) = components.get(&id) else {
                        continue;
                    };
                    let definition = &symbol.pins[pin];
                    if symbol.kind == SymbolKind::SupplyPort && !component.value.is_empty() {
                        net.supply_names.insert(component.value.clone());
                    }
                    net.pins.push(PinNode {
                        sheet: node.sheet,
                        component: id,
                        reference: component.reference.clone(),
                        pin,
                        number: definition.number.clone(),
                        name: definition.name.clone(),
                        kind: definition.kind,
                        at: node.at,
                    });
                }
                Element::Wire(id) => {
                    by_element.insert((node.sheet, id), net_index);
                    net.wires.push(attachment(id));
                }
                Element::Label(id) => {
                    by_element.insert((node.sheet, id), net_index);
                    net.labels.push(attachment(id));
                    if let Some(label) = labels.get(&id) {
                        match label.kind {
                            LabelKind::Global => {
                                net.label_names.insert(label.name.clone());
                                global_names[net_index].insert(label.name.clone());
                            }
                            LabelKind::Local | LabelKind::Hierarchical => {
                                let sheet = sheet_names.get(&node.sheet).copied().unwrap_or("");
                                net.label_names.insert(format!("/{sheet}/{}", label.name));
                            }
                        }
                    }
                }
                Element::NoConnect(id) => {
                    by_element.insert((node.sheet, id), net_index);
                    net.no_connects.push(attachment(id));
                }
                Element::Port(id, port) => {
                    by_element.entry((node.sheet, id)).or_insert(net_index);
                    net.ports.push((attachment(id), port));
                }
                Element::Junction => {}
            }
        }

        let depth = sheet_depths(doc);
        for (net, global) in nets.iter_mut().zip(&global_names) {
            net.pins.sort_by(|a, b| {
                (&a.reference, natural_key(&a.number)).cmp(&(&b.reference, natural_key(&b.number)))
            });
            net.name = net_name(net, global, &depth, &sheet_names);
        }
        // Deterministic order, independent of how the document was edited.
        let mut order: Vec<usize> = (0..nets.len()).collect();
        order.sort_by(|&a, &b| nets[a].name.cmp(&nets[b].name));
        let mut new_position = vec![0; nets.len()];
        for (position, &old) in order.iter().enumerate() {
            new_position[old] = position;
        }
        for net_index in by_element.values_mut() {
            *net_index = new_position[*net_index];
        }
        let mut slots: Vec<Option<Net>> = nets.into_iter().map(Some).collect();
        let nets = order.iter().filter_map(|&old| slots[old].take()).collect();
        Self { nets, by_element }
    }

    /// The net holding an element. For components and blocks with several
    /// pins this is the net of one of them; use [`Connectivity::net_of_pin`].
    pub fn net_of(&self, sheet: ElementId, element: ElementId) -> Option<&Net> {
        self.by_element
            .get(&(sheet, element))
            .map(|&index| &self.nets[index])
    }

    pub fn net_of_pin(&self, component: ElementId, pin: usize) -> Option<&Net> {
        self.nets.iter().find(|net| {
            net.pins
                .iter()
                .any(|node| node.component == component && node.pin == pin)
        })
    }
}

fn sheet_depths(doc: &Schematic) -> HashMap<ElementId, usize> {
    doc.sheets_in_tree_order()
        .into_iter()
        .map(|(depth, sheet)| (sheet.id, depth))
        .collect()
}

/// Orders numeric pin numbers by value: 2 before 10.
fn natural_key(text: &str) -> (u64, String) {
    (text.parse().unwrap_or(u64::MAX), text.to_owned())
}

fn net_name(
    net: &Net,
    global: &BTreeSet<String>,
    depth: &HashMap<ElementId, usize>,
    sheets: &HashMap<ElementId, &str>,
) -> String {
    if let Some(name) = net.supply_names.iter().next() {
        return name.clone();
    }
    if let Some(name) = global.iter().next() {
        return name.clone();
    }
    // Local names of the highest sheet in the hierarchy win.
    let top = net
        .label_names
        .iter()
        .filter(|name| name.starts_with('/'))
        .min_by_key(|name| {
            let sheet = name.split('/').nth(1).unwrap_or("");
            let level = sheets
                .iter()
                .find(|(_, candidate)| **candidate == sheet)
                .and_then(|(id, _)| depth.get(id))
                .copied()
                .unwrap_or(usize::MAX);
            (level, (*name).clone())
        });
    if let Some(name) = top {
        return name.clone();
    }
    match net.pins.first() {
        Some(pin) => format!("Net-{}-{}", pin.reference, pin.number),
        None => match (net.wires.first(), net.labels.first()) {
            (Some(wire), _) => format!("Net-W{}", wire.id),
            (None, Some(label)) => format!("Net-L{}", label.id),
            _ => "Net-unnamed".to_owned(),
        },
    }
}

/// Points where a dot shows that wires are connected.
pub fn junction_points(doc: &Schematic, sheet: &Sheet) -> Vec<Point> {
    let index = SegmentIndex::new(&sheet.wires);
    let mut ends: HashMap<Point, usize> = HashMap::new();
    for wire in &sheet.wires {
        *ends.entry(wire.a).or_default() += 1;
        *ends.entry(wire.b).or_default() += 1;
    }
    for component in &sheet.components {
        for at in doc.pin_points(component) {
            if index.at(at).next().is_some() {
                *ends.entry(at).or_default() += 1;
            }
        }
    }
    let mut points: Vec<Point> = ends
        .into_iter()
        .filter_map(|(at, count)| {
            let through = index
                .at(at)
                .filter(|wire| wire.a != at && wire.b != at)
                .count();
            (count + 2 * through >= 3).then_some(at)
        })
        .chain(sheet.junctions.iter().map(|marker| marker.at))
        .collect();
    points.sort_unstable();
    points.dedup();
    points
}
