//! The routing grid: which track centre or via may go where, for which net.
//!
//! Each node holds who may use it: everyone, one net (it lies in the keep-out of
//! that net's copper only) or nobody. A keep-out is as wide as the copper plus
//! clearance plus half the new copper's width, measured exactly.

use std::collections::BTreeMap;
use tronic_pcb::copper::pad_shape;
use tronic_pcb::geom::in_polygons;
use tronic_pcb::{Board, Shape, MM};
use tronic_schematic::Point;

use tronic_pcb::{NewTrack, NewVia};

const FREE: u32 = 0;
const BLOCKED: u32 = u32::MAX;
/// Upper limit for nodes times layers, about 128 MB of grid.
const MAX_CELLS: usize = 32_000_000;

#[derive(Debug, Clone)]
pub struct RouteGrid {
    pub x0: i64,
    pub y0: i64,
    pub pitch: i64,
    pub width: usize,
    pub height: usize,
    pub layers: usize,
    tracks: Vec<u32>,
    vias: Vec<u32>,
    track_margin: f64,
    via_margin: f64,
    hole_reach: i64,
    drill: i64,
    via_diameter: i64,
    nets: BTreeMap<String, u32>,
}

fn mark(cell: &mut u32, owner: u32) {
    if *cell == FREE {
        *cell = owner;
    } else if *cell != owner {
        *cell = BLOCKED;
    }
}

impl RouteGrid {
    /// Returns None if the board does not fit the grid limits.
    pub fn new(board: &Board, nets: &BTreeMap<String, u32>) -> Option<Self> {
        let rules = &board.rules;
        let min_x = board.outline.iter().map(|p| p.x).min()?;
        let max_x = board.outline.iter().map(|p| p.x).max()?;
        let min_y = board.outline.iter().map(|p| p.y).min()?;
        let max_y = board.outline.iter().map(|p| p.y).max()?;
        let layers = board.copper_layers as usize;

        // Fine enough that a keep-out always covers a grid step, also diagonally.
        let keepout = rules.track_width + 2 * rules.min_clearance;
        let mut pitch = (keepout / 2).clamp(MM / 20, MM / 4);
        let coarsest = keepout * 2 / 3;
        let cells = |pitch: i64| {
            let width = ((max_x - min_x) / pitch + 2) as usize;
            let height = ((max_y - min_y) / pitch + 2) as usize;
            width * height * layers
        };
        while cells(pitch) > MAX_CELLS && pitch < coarsest {
            pitch += MM / 20;
        }
        if cells(pitch) > MAX_CELLS {
            return None;
        }
        // The grid lies on multiples of its pitch, like most placement.
        let x0 = min_x.div_euclid(pitch) * pitch;
        let y0 = min_y.div_euclid(pitch) * pitch;
        let width = ((max_x - x0) / pitch + 2) as usize;
        let height = ((max_y - y0) / pitch + 2) as usize;

        let mut grid = Self {
            x0,
            y0,
            pitch,
            width,
            height,
            layers,
            tracks: vec![FREE; width * height * layers],
            vias: vec![FREE; width * height],
            track_margin: (rules.min_clearance as f64) + rules.track_width as f64 / 2.0,
            via_margin: (rules.min_clearance as f64) + rules.via_diameter as f64 / 2.0,
            hole_reach: rules.min_hole_spacing + rules.via_drill / 2,
            drill: rules.via_drill,
            via_diameter: rules.via_diameter,
            nets: nets.clone(),
        };
        grid.mark_board_area(board);

        for pad in board.resolved_pads() {
            let shape = pad_shape(&pad);
            let owner = match (&pad.net, pad.plated) {
                (Some(net), true) => nets.get(net).map_or(BLOCKED, |n| n + 1),
                _ => BLOCKED,
            };
            grid.mark_shape(&shape, owner, pad.layer);
            if let Some(drill) = pad.drill {
                grid.mark_hole(pad.at, drill);
            }
        }
        for track in board.tracks.iter() {
            let owner = nets.get(&track.net).map_or(BLOCKED, |n| n + 1);
            let shape = Shape::track(track.a, track.b, track.width);
            grid.mark_shape(&shape, owner, Some(track.layer));
        }
        for via in board.vias.iter() {
            let owner = nets.get(&via.net).map_or(BLOCKED, |n| n + 1);
            grid.mark_shape(&Shape::circle(via.at, via.diameter), owner, None);
            grid.mark_hole(via.at, via.drill);
        }
        // Zones are not obstacles: their filling gives way to new tracks.
        Some(grid)
    }

    fn mark_board_area(&mut self, board: &Board) {
        let contour: Vec<[f64; 2]> = board
            .outline
            .iter()
            .map(|p| [p.x as f64, p.y as f64])
            .collect();
        let contours = [contour];
        for y in 0..self.height {
            for x in 0..self.width {
                let at = self.position(x, y);
                if !in_polygons((at.x as f64, at.y as f64), &contours) {
                    let node = y * self.width + x;
                    self.vias[node] = BLOCKED;
                    for layer in 0..self.layers {
                        let index = self.index(layer, node);
                        self.tracks[index] = BLOCKED;
                    }
                }
            }
        }
        let edge = board.rules.edge_clearance as f64;
        let n = board.outline.len();
        for i in 0..n {
            let shape = Shape::track(board.outline[i], board.outline[(i + 1) % n], 0);
            self.mark_within(
                &shape,
                edge + board.rules.track_width as f64 / 2.0,
                None,
                false,
            );
            self.mark_within(
                &shape,
                edge + board.rules.via_diameter as f64 / 2.0,
                None,
                true,
            );
        }
    }

    fn index(&self, layer: usize, node: usize) -> usize {
        layer * self.width * self.height + node
    }

    pub fn position(&self, x: usize, y: usize) -> Point {
        Point::new(
            self.x0 + x as i64 * self.pitch,
            self.y0 + y as i64 * self.pitch,
        )
    }

    pub fn node_position(&self, node: usize) -> Point {
        self.position(node % self.width, node / self.width)
    }

    pub fn may_use(&self, layer: usize, node: usize, net_index: u32) -> bool {
        let cell = self.tracks[self.index(layer, node)];
        cell == FREE || cell == net_index + 1
    }

    pub fn may_place_via(&self, node: usize, net_index: u32) -> bool {
        let cell = self.vias[node];
        cell == FREE || cell == net_index + 1
    }

    fn nodes_near(
        &self,
        bounds: (f64, f64, f64, f64),
        margin: f64,
    ) -> impl Iterator<Item = (usize, usize)> {
        let pitch = self.pitch as f64;
        let clamp = |value: f64, limit: usize| value.max(0.0).min(limit as f64 - 1.0) as usize;
        let x0 = clamp(
            ((bounds.0 - margin - self.x0 as f64) / pitch).floor(),
            self.width,
        );
        let x1 = clamp(
            ((bounds.2 + margin - self.x0 as f64) / pitch).ceil(),
            self.width,
        );
        let y0 = clamp(
            ((bounds.1 - margin - self.y0 as f64) / pitch).floor(),
            self.height,
        );
        let y1 = clamp(
            ((bounds.3 + margin - self.y0 as f64) / pitch).ceil(),
            self.height,
        );
        (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| (x, y)))
    }

    fn mark_within(&mut self, shape: &Shape, margin: f64, layer: Option<u8>, via: bool) {
        let nodes: Vec<(usize, usize)> = self.nodes_near(shape.bounds(), margin).collect();
        for (x, y) in nodes {
            if shape.gap(&Shape::circle(self.position(x, y), 0)) >= margin {
                continue;
            }
            let node = y * self.width + x;
            if via {
                self.vias[node] = BLOCKED;
            } else {
                match layer {
                    Some(layer) => {
                        let index = self.index(layer as usize, node);
                        self.tracks[index] = BLOCKED;
                    }
                    None => {
                        for layer in 0..self.layers {
                            let index = self.index(layer, node);
                            self.tracks[index] = BLOCKED;
                        }
                    }
                }
            }
        }
    }

    /// Enters copper: keep-outs for tracks on its layers, for vias on all layers.
    fn mark_shape(&mut self, shape: &Shape, owner: u32, layer: Option<u8>) {
        let nodes: Vec<(usize, usize)> = self
            .nodes_near(shape.bounds(), self.track_margin.max(self.via_margin))
            .collect();
        for (x, y) in nodes {
            let gap = shape.gap(&Shape::circle(self.position(x, y), 0));
            let node = y * self.width + x;
            if gap < self.via_margin {
                mark(&mut self.vias[node], owner);
            }
            if gap < self.track_margin {
                match layer {
                    Some(layer) if (layer as usize) < self.layers => {
                        let index = self.index(layer as usize, node);
                        mark(&mut self.tracks[index], owner);
                    }
                    Some(_) => {}
                    None => {
                        for layer in 0..self.layers {
                            let index = self.index(layer, node);
                            mark(&mut self.tracks[index], owner);
                        }
                    }
                }
            }
        }
    }

    /// Holes keep their distance from all other holes, whatever the net.
    fn mark_hole(&mut self, at: Point, drill: i64) {
        let reach = (self.hole_reach + drill / 2) as f64;
        let nodes: Vec<(usize, usize)> = self
            .nodes_near((at.x as f64, at.y as f64, at.x as f64, at.y as f64), reach)
            .collect();
        for (x, y) in nodes {
            let p = self.position(x, y);
            if ((p.x - at.x) as f64).hypot((p.y - at.y) as f64) < reach {
                self.vias[y * self.width + x] = BLOCKED;
            }
        }
    }

    pub fn add_route(&mut self, tracks: &[NewTrack], vias: &[NewVia]) {
        for track in tracks {
            let owner = self.nets.get(&track.net).map_or(BLOCKED, |n| n + 1);
            let shape = Shape::track(track.a, track.b, track.width);
            self.mark_shape(&shape, owner, Some(track.layer));
        }
        for via in vias {
            let owner = self.nets.get(&via.net).map_or(BLOCKED, |n| n + 1);
            self.mark_shape(&Shape::circle(via.at, self.via_diameter), owner, None);
            self.mark_hole(via.at, self.drill);
        }
    }
}
