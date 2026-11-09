//! Exact distances between copper shapes, in nanometres as f64.

use std::collections::{HashMap, HashSet};
use tronic_schematic::Point;

type P = (f64, f64);

fn to_f(p: Point) -> P {
    (p.x as f64, p.y as f64)
}

/// Tracks and round pads are capsules, rectangular pads axis-parallel boxes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Capsule { a: P, b: P, r: f64 },
    Box { min: P, max: P },
}

fn dot(a: P, b: P) -> f64 {
    a.0 * b.0 + a.1 * b.1
}

fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}

fn norm(a: P) -> f64 {
    a.0.hypot(a.1)
}

fn point_segment(p: P, a: P, b: P) -> f64 {
    let ab = sub(b, a);
    let len = dot(ab, ab);
    if len == 0.0 {
        return norm(sub(p, a));
    }
    let t = (dot(sub(p, a), ab) / len).clamp(0.0, 1.0);
    norm(sub(p, (a.0 + ab.0 * t, a.1 + ab.1 * t)))
}

fn cross(a: P, b: P) -> f64 {
    a.0 * b.1 - a.1 * b.0
}

/// Whether two segments properly cross or touch.
pub fn segments_intersect(a: P, b: P, c: P, d: P) -> bool {
    let d1 = cross(sub(d, c), sub(a, c));
    let d2 = cross(sub(d, c), sub(b, c));
    let d3 = cross(sub(b, a), sub(c, a));
    let d4 = cross(sub(b, a), sub(d, a));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    (d1 == 0.0 && point_segment(a, c, d) == 0.0)
        || (d2 == 0.0 && point_segment(b, c, d) == 0.0)
        || (d3 == 0.0 && point_segment(c, a, b) == 0.0)
        || (d4 == 0.0 && point_segment(d, a, b) == 0.0)
}

fn segment_segment(a: P, b: P, c: P, d: P) -> f64 {
    if segments_intersect(a, b, c, d) {
        return 0.0;
    }
    point_segment(a, c, d)
        .min(point_segment(b, c, d))
        .min(point_segment(c, a, b))
        .min(point_segment(d, a, b))
}

fn point_box(p: P, min: P, max: P) -> f64 {
    let dx = (min.0 - p.0).max(p.0 - max.0).max(0.0);
    let dy = (min.1 - p.1).max(p.1 - max.1).max(0.0);
    dx.hypot(dy)
}

fn segment_box(a: P, b: P, min: P, max: P) -> f64 {
    let inside = |p: P| p.0 >= min.0 && p.0 <= max.0 && p.1 >= min.1 && p.1 <= max.1;
    if inside(a) || inside(b) {
        return 0.0;
    }
    let corners = [min, (max.0, min.1), max, (min.0, max.1)];
    let mut best = point_box(a, min, max).min(point_box(b, min, max));
    for i in 0..4 {
        best = best.min(segment_segment(a, b, corners[i], corners[(i + 1) % 4]));
    }
    best
}

impl Shape {
    pub fn track(a: Point, b: Point, width: i64) -> Self {
        Self::Capsule {
            a: to_f(a),
            b: to_f(b),
            r: width as f64 / 2.0,
        }
    }

    pub fn circle(at: Point, diameter: i64) -> Self {
        Self::track(at, at, diameter)
    }

    /// An oblong pad: a capsule along its longer side.
    pub fn oblong(at: Point, size: Point) -> Self {
        let c = to_f(at);
        let (w, h) = (size.x as f64, size.y as f64);
        if w >= h {
            let half = (w - h) / 2.0;
            Self::Capsule {
                a: (c.0 - half, c.1),
                b: (c.0 + half, c.1),
                r: h / 2.0,
            }
        } else {
            let half = (h - w) / 2.0;
            Self::Capsule {
                a: (c.0, c.1 - half),
                b: (c.0, c.1 + half),
                r: w / 2.0,
            }
        }
    }

    pub fn rect(at: Point, size: Point) -> Self {
        let c = to_f(at);
        let (w, h) = (size.x as f64 / 2.0, size.y as f64 / 2.0);
        Self::Box {
            min: (c.0 - w, c.1 - h),
            max: (c.0 + w, c.1 + h),
        }
    }

    /// Gap between two shapes; zero or negative means they touch or overlap.
    pub fn gap(&self, other: &Shape) -> f64 {
        match (self, other) {
            (
                Self::Capsule { a, b, r },
                Self::Capsule {
                    a: c,
                    b: d,
                    r: other_r,
                },
            ) => segment_segment(*a, *b, *c, *d) - r - other_r,
            (Self::Capsule { a, b, r }, Self::Box { min, max })
            | (Self::Box { min, max }, Self::Capsule { a, b, r }) => {
                segment_box(*a, *b, *min, *max) - r
            }
            (Self::Box { min, max }, Self::Box { min: c, max: d }) => {
                let dx = (c.0 - max.0).max(min.0 - d.0);
                let dy = (c.1 - max.1).max(min.1 - d.1);
                if dx <= 0.0 && dy <= 0.0 {
                    dx.max(dy)
                } else {
                    dx.max(0.0).hypot(dy.max(0.0))
                }
            }
        }
    }

    pub fn distance_to_segment(&self, a: Point, b: Point) -> f64 {
        self.gap(&Shape::track(a, b, 0))
    }

    /// (min x, min y, max x, max y)
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        match self {
            Self::Capsule { a, b, r } => (
                a.0.min(b.0) - r,
                a.1.min(b.1) - r,
                a.0.max(b.0) + r,
                a.1.max(b.1) + r,
            ),
            Self::Box { min, max } => (min.0, min.1, max.0, max.1),
        }
    }

    /// A point on the centre of the shape.
    pub fn center(&self) -> P {
        match self {
            Self::Capsule { a, b, .. } => ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0),
            Self::Box { min, max } => ((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0),
        }
    }

    /// An outline polygon around the shape, grown by `margin`, counter-clockwise
    /// and never smaller than the true shape.
    pub fn polygon(&self, margin: f64) -> Vec<[f64; 2]> {
        match self {
            Self::Box { min, max } => vec![
                [min.0 - margin, min.1 - margin],
                [max.0 + margin, min.1 - margin],
                [max.0 + margin, max.1 + margin],
                [min.0 - margin, max.1 + margin],
            ],
            Self::Capsule { a, b, r } => {
                let radius = r + margin;
                let angle = (b.1 - a.1).atan2(b.0 - a.0);
                const STEPS: usize = 8;
                // Circumscribed, so that the polygon contains the round end.
                let outer = radius / (std::f64::consts::PI / (2.0 * STEPS as f64)).cos();
                let mut points = Vec::with_capacity(2 * STEPS + 2);
                for i in 0..=STEPS {
                    let t = angle - std::f64::consts::FRAC_PI_2
                        + std::f64::consts::PI * i as f64 / STEPS as f64;
                    points.push([b.0 + outer * t.cos(), b.1 + outer * t.sin()]);
                }
                for i in 0..=STEPS {
                    let t = angle
                        + std::f64::consts::FRAC_PI_2
                        + std::f64::consts::PI * i as f64 / STEPS as f64;
                    points.push([a.0 + outer * t.cos(), a.1 + outer * t.sin()]);
                }
                points
            }
        }
    }
}

/// Point in polygon set, even-odd over all contours, so holes count.
pub fn in_polygons(p: P, contours: &[Vec<[f64; 2]>]) -> bool {
    let mut inside = false;
    for contour in contours {
        let mut j = contour.len().wrapping_sub(1);
        for i in 0..contour.len() {
            let (a, b) = (contour[i], contour[j]);
            if (a[1] > p.1) != (b[1] > p.1)
                && p.0 < (b[0] - a[0]) * (p.1 - a[1]) / (b[1] - a[1]) + a[0]
            {
                inside = !inside;
            }
            j = i;
        }
    }
    inside
}

/// Whether a closed polygon crosses itself or is degenerate.
pub fn polygon_is_invalid(points: &[Point]) -> bool {
    if points.len() < 3 {
        return true;
    }
    let n = points.len();
    for i in 0..n {
        let (a, b) = (to_f(points[i]), to_f(points[(i + 1) % n]));
        for j in i + 1..n {
            // Neighbouring edges share a point by construction.
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let (c, d) = (to_f(points[j]), to_f(points[(j + 1) % n]));
            if segments_intersect(a, b, c, d) {
                return true;
            }
        }
    }
    let area: f64 = (0..n)
        .map(|i| cross(to_f(points[i]), to_f(points[(i + 1) % n])))
        .sum();
    area == 0.0
}

/// Spatial index over bounding boxes.
pub struct Grid {
    cell: f64,
    cells: HashMap<(i64, i64), Vec<usize>>,
}

impl Grid {
    pub fn new(cell: f64) -> Self {
        Self {
            cell,
            cells: HashMap::new(),
        }
    }

    fn range(&self, bounds: (f64, f64, f64, f64)) -> impl Iterator<Item = (i64, i64)> {
        let c = self.cell;
        let (x0, y0) = ((bounds.0 / c).floor() as i64, (bounds.1 / c).floor() as i64);
        let (x1, y1) = ((bounds.2 / c).floor() as i64, (bounds.3 / c).floor() as i64);
        (x0..=x1).flat_map(move |x| (y0..=y1).map(move |y| (x, y)))
    }

    pub fn insert(&mut self, index: usize, bounds: (f64, f64, f64, f64)) {
        for key in self.range(bounds).collect::<Vec<_>>() {
            self.cells.entry(key).or_default().push(index);
        }
    }

    /// Indices whose cells overlap the bounds, each once.
    pub fn query(&self, bounds: (f64, f64, f64, f64)) -> Vec<usize> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for key in self.range(bounds) {
            if let Some(list) = self.cells.get(&key) {
                for &index in list {
                    if seen.insert(index) {
                        out.push(index);
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MM: i64 = 1_000_000;

    fn p(x: i64, y: i64) -> Point {
        Point::new(x * MM, y * MM)
    }

    #[test]
    fn parallel_tracks_measure_edge_to_edge() {
        let a = Shape::track(p(0, 0), p(10, 0), MM);
        let b = Shape::track(p(0, 3), p(10, 3), MM);
        assert_eq!(a.gap(&b), 2.0 * MM as f64);
    }

    #[test]
    fn crossing_tracks_overlap() {
        let a = Shape::track(p(0, 0), p(10, 0), MM);
        let b = Shape::track(p(5, -5), p(5, 5), MM);
        assert!(a.gap(&b) < 0.0);
    }

    #[test]
    fn diagonal_distance_is_exact() {
        let a = Shape::circle(p(0, 0), 0);
        let b = Shape::circle(p(3, 4), 0);
        assert_eq!(a.gap(&b), 5.0 * MM as f64);
    }

    #[test]
    fn box_gaps_cover_all_cases() {
        let a = Shape::rect(p(0, 0), p(2, 2));
        assert_eq!(a.gap(&Shape::rect(p(5, 0), p(2, 2))), 3.0 * MM as f64);
        let diagonal = a.gap(&Shape::rect(p(4, 4), p(2, 2)));
        assert!((diagonal - 2f64.hypot(2.0) * MM as f64).abs() < 1.0);
        assert!(a.gap(&Shape::rect(p(1, 0), p(2, 2))) < 0.0);
        assert_eq!(a.gap(&Shape::circle(p(4, 0), 2 * MM)), 2.0 * MM as f64);
        assert!(a.gap(&Shape::track(p(-5, 0), p(5, 0), 0)) <= 0.0);
    }

    #[test]
    fn oblong_pads_are_capsules_along_the_long_side() {
        let shape = Shape::oblong(p(0, 0), p(4, 2));
        assert_eq!(shape.bounds(), (-2.0e6, -1.0e6, 2.0e6, 1.0e6));
        let tall = Shape::oblong(p(0, 0), p(2, 4));
        assert_eq!(tall.bounds(), (-1.0e6, -2.0e6, 1.0e6, 2.0e6));
    }

    #[test]
    fn capsule_polygon_contains_the_shape() {
        let shape = Shape::track(p(0, 0), p(10, 5), 2 * MM);
        let polygon = shape.polygon(0.0);
        for t in 0..64 {
            let angle = t as f64 / 64.0 * std::f64::consts::TAU;
            let q = (1.99e6 / 2.0 * angle.cos(), 1.99e6 / 2.0 * angle.sin());
            assert!(in_polygons(q, std::slice::from_ref(&polygon)), "{q:?}");
        }
    }

    #[test]
    fn point_in_polygon_respects_holes() {
        let outer = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let hole = vec![[4.0, 4.0], [6.0, 4.0], [6.0, 6.0], [4.0, 6.0]];
        let shape = [outer, hole];
        assert!(in_polygons((1.0, 1.0), &shape));
        assert!(!in_polygons((5.0, 5.0), &shape));
        assert!(!in_polygons((20.0, 5.0), &shape));
    }

    #[test]
    fn detects_self_crossing_outlines() {
        let square = [p(0, 0), p(10, 0), p(10, 10), p(0, 10)];
        assert!(!polygon_is_invalid(&square));
        let bow_tie = [p(0, 0), p(10, 10), p(10, 0), p(0, 10)];
        assert!(polygon_is_invalid(&bow_tie));
        assert!(polygon_is_invalid(&[p(0, 0), p(1, 1)]));
        assert!(polygon_is_invalid(&[p(0, 0), p(5, 0), p(10, 0)]));
    }

    #[test]
    fn grid_finds_overlapping_items_once() {
        let mut grid = Grid::new(10.0);
        grid.insert(1, (0.0, 0.0, 25.0, 5.0));
        grid.insert(2, (100.0, 100.0, 101.0, 101.0));
        assert_eq!(grid.query((5.0, 1.0, 22.0, 2.0)), vec![1]);
        assert!(grid.query((50.0, 50.0, 51.0, 51.0)).is_empty());
    }
}
