//! Copper as electrical items: what touches what, and what is still open.

use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use tronic_schematic::{ElementId, Point};

use crate::geom::{in_polygons, Grid, Shape};
use crate::model::*;
use crate::zones::{fill_zones, ZoneFill};

const GRID_CELL: f64 = 5_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ItemKind {
    Pad { instance: ElementId, pad: usize },
    Track(ElementId),
    Via(ElementId),
}

/// One piece of copper.
#[derive(Debug, Clone, PartialEq)]
pub struct CopperItem {
    pub kind: ItemKind,
    pub net: Option<String>,
    /// None means all layers.
    pub layer: Option<u8>,
    pub shape: Shape,
    pub at: Point,
}

impl CopperItem {
    pub fn shares_layer(&self, other: &CopperItem) -> bool {
        match (self.layer, other.layer) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
    }

    pub fn element(&self) -> ElementId {
        match self.kind {
            ItemKind::Pad { instance, .. } => instance,
            ItemKind::Track(id) | ItemKind::Via(id) => id,
        }
    }
}

pub fn pad_shape(pad: &ResolvedPad) -> Shape {
    // What matters about a bare hole is the hole itself.
    if let (false, Some(drill)) = (pad.plated, pad.drill) {
        return Shape::circle(pad.at, drill);
    }
    match pad.shape {
        PadShape::Rect => Shape::rect(pad.at, pad.size),
        PadShape::Round => Shape::circle(pad.at, pad.size.x),
        PadShape::Oblong => Shape::oblong(pad.at, pad.size),
    }
}

/// All copper of a board apart from zones.
pub fn copper_items(board: &Board, pads: &[ResolvedPad]) -> Vec<CopperItem> {
    let mut items = Vec::with_capacity(pads.len() + board.tracks.len() + board.vias.len());
    for (pad_index, pad) in pads.iter().enumerate() {
        if !pad.plated {
            continue; // A bare mounting hole carries no copper.
        }
        items.push(CopperItem {
            kind: ItemKind::Pad {
                instance: pad.instance,
                pad: pad_index,
            },
            net: pad.net.clone(),
            layer: pad.layer,
            shape: pad_shape(pad),
            at: pad.at,
        });
    }
    for track in board.tracks.iter() {
        items.push(CopperItem {
            kind: ItemKind::Track(track.id),
            net: Some(track.net.clone()),
            layer: Some(track.layer),
            shape: Shape::track(track.a, track.b, track.width),
            at: track.a,
        });
    }
    for via in board.vias.iter() {
        items.push(CopperItem {
            kind: ItemKind::Via(via.id),
            net: Some(via.net.clone()),
            layer: None,
            shape: Shape::circle(via.at, via.diameter),
            at: via.at,
        });
    }
    items
}

/// A missing connection, shown as a straight line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Airwire {
    pub net: String,
    pub a: Point,
    pub b: Point,
}

#[derive(Debug, Clone)]
pub struct CopperAnalysis {
    pub items: Vec<CopperItem>,
    pub fills: Vec<ZoneFill>,
    pub airwires: Vec<Airwire>,
    /// Sum of track lengths per net, in nanometres.
    pub net_lengths: BTreeMap<String, i64>,
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

/// Finds connected copper per net and the connections that are still missing.
pub fn analyze(board: &Board) -> CopperAnalysis {
    let pads = board.resolved_pads();
    let items = copper_items(board, &pads);
    let fills = fill_zones(board);

    let mut groups = UnionFind((0..items.len()).collect());
    let mut grid = Grid::new(GRID_CELL);
    for (index, item) in items.iter().enumerate() {
        grid.insert(index, item.shape.bounds());
    }
    for (index, item) in items.iter().enumerate() {
        if item.net.is_none() {
            continue;
        }
        for other in grid.query(item.shape.bounds()) {
            if other <= index {
                continue;
            }
            let candidate = &items[other];
            if candidate.net == item.net
                && item.shares_layer(candidate)
                && item.shape.gap(&candidate.shape) <= 0.0
            {
                groups.union(index, other);
            }
        }
    }

    // Zones join everything of their net that lies on them.
    for fill in &fills {
        for shape in &fill.shapes {
            let contours: Vec<Vec<[f64; 2]>> = shape
                .iter()
                .map(|contour| contour.iter().map(|p| [p.x as f64, p.y as f64]).collect())
                .collect();
            let mut first = None;
            for (index, item) in items.iter().enumerate() {
                let on_layer = item.layer.is_none_or(|layer| layer == fill.layer);
                if on_layer
                    && item.net.as_deref() == Some(fill.net.as_str())
                    && in_polygons(item.shape.center(), &contours)
                {
                    match first {
                        None => first = Some(index),
                        Some(anchor) => groups.union(anchor, index),
                    }
                }
            }
        }
    }

    let mut by_net: BTreeMap<&str, HashMap<usize, Vec<Point>>> = BTreeMap::new();
    for (index, item) in items.iter().enumerate() {
        let Some(net) = item.net.as_deref() else {
            continue;
        };
        let root = groups.find(index);
        let points = by_net.entry(net).or_default().entry(root).or_default();
        points.push(item.at);
        if let ItemKind::Track(id) = item.kind {
            if let Some(track) = board.tracks.iter().find(|t| t.id == id) {
                points.push(track.b);
            }
        }
    }

    let mut airwires = Vec::new();
    for (net, clusters) in &by_net {
        airwires.extend(connect_clusters(net, clusters.values().collect()));
    }

    let mut net_lengths: BTreeMap<String, i64> = BTreeMap::new();
    for track in board.tracks.iter() {
        let length = ((track.b.x - track.a.x) as f64).hypot((track.b.y - track.a.y) as f64);
        *net_lengths.entry(track.net.clone()).or_default() += length.round() as i64;
    }

    CopperAnalysis {
        items,
        fills,
        airwires,
        net_lengths,
    }
}

/// Minimal spanning connection between clusters, nearest points first.
fn connect_clusters(net: &str, mut clusters: Vec<&Vec<Point>>) -> Vec<Airwire> {
    let mut wires = Vec::new();
    if clusters.len() < 2 {
        return wires;
    }
    // Stable result, independent of hash order.
    clusters.sort_by_key(|points| points.iter().min().copied());
    let mut connected: Vec<Point> = clusters.remove(0).clone();
    while !clusters.is_empty() {
        let mut best: Option<(f64, usize, Point, Point)> = None;
        for (index, cluster) in clusters.iter().enumerate() {
            for b in cluster.iter() {
                for a in &connected {
                    let distance = ((a.x - b.x) as f64).hypot((a.y - b.y) as f64);
                    if best.as_ref().is_none_or(|current| distance < current.0) {
                        best = Some((distance, index, *a, *b));
                    }
                }
            }
        }
        let Some((_, index, a, b)) = best else {
            break;
        };
        wires.push(Airwire {
            net: net.to_owned(),
            a,
            b,
        });
        connected.extend(clusters.remove(index).iter().copied());
    }
    wires
}
