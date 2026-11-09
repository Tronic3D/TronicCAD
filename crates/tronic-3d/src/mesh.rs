//! Triangle meshes with flat normals.

use serde::Serialize;

pub type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn normal(a: V3, b: V3, c: V3) -> V3 {
    let (u, v) = (sub(b, a), sub(c, a));
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if length > 0.0 {
        [n[0] / length, n[1] / length, n[2] / length]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// Positions and normals as flat lists, three of each per corner.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct Mesh {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
}

impl Mesh {
    pub fn triangle_count(&self) -> usize {
        self.positions.len() / 9
    }

    pub fn triangle(&mut self, a: V3, b: V3, c: V3) {
        let n = normal(a, b, c);
        for p in [a, b, c] {
            self.positions.extend_from_slice(&p);
            self.normals.extend_from_slice(&n);
        }
    }

    pub fn quad(&mut self, a: V3, b: V3, c: V3, d: V3) {
        self.triangle(a, b, c);
        self.triangle(a, c, d);
    }

    /// A box between two corners, faces pointing outwards.
    pub fn cuboid(&mut self, min: V3, max: V3) {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        self.quad([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]);
        self.quad([x0, y1, z0], [x1, y1, z0], [x1, y0, z0], [x0, y0, z0]);
        self.quad([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]);
        self.quad([x1, y1, z0], [x0, y1, z0], [x0, y1, z1], [x1, y1, z1]);
        self.quad([x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]);
        self.quad([x0, y1, z0], [x0, y0, z0], [x0, y0, z1], [x0, y1, z1]);
    }

    /// A vertical cylinder.
    pub fn cylinder(&mut self, center: [f32; 2], radius: f32, z0: f32, z1: f32) {
        const SIDES: usize = 24;
        let point = |i: usize| {
            let angle = std::f32::consts::TAU * (i % SIDES) as f32 / SIDES as f32;
            [
                center[0] + radius * angle.cos(),
                center[1] + radius * angle.sin(),
            ]
        };
        for i in 0..SIDES {
            let (a, b) = (point(i), point(i + 1));
            self.quad(
                [a[0], a[1], z0],
                [b[0], b[1], z0],
                [b[0], b[1], z1],
                [a[0], a[1], z1],
            );
            self.triangle(
                [center[0], center[1], z1],
                [a[0], a[1], z1],
                [b[0], b[1], z1],
            );
            self.triangle(
                [center[0], center[1], z0],
                [b[0], b[1], z0],
                [a[0], a[1], z0],
            );
        }
    }

    /// (min, max) of all corners.
    pub fn bounds(&self) -> Option<(V3, V3)> {
        let (first, rest) = self.positions.as_chunks::<3>().0.split_first()?;
        let mut min = *first;
        let mut max = min;
        for p in rest {
            for axis in 0..3 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        Some((min, max))
    }
}

/// Triangulates a simple polygon by ear clipping. Returns corner index triples.
pub fn triangulate(polygon: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let area: f32 = (0..polygon.len())
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
            a[0] * b[1] - a[1] * b[0]
        })
        .sum();
    let mut remaining: Vec<usize> = (0..polygon.len()).collect();
    if area < 0.0 {
        remaining.reverse();
    }
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut triangles = Vec::with_capacity(polygon.len().saturating_sub(2));
    let mut guard = polygon.len() * polygon.len() + 8;
    while remaining.len() > 3 && guard > 0 {
        guard -= 1;
        let n = remaining.len();
        let mut clipped = false;
        for i in 0..n {
            let (ia, ib, ic) = (
                remaining[(i + n - 1) % n],
                remaining[i],
                remaining[(i + 1) % n],
            );
            let (a, b, c) = (polygon[ia], polygon[ib], polygon[ic]);
            if cross(a, b, c) <= 0.0 {
                continue;
            }
            let contains_other = remaining.iter().any(|&other| {
                if other == ia || other == ib || other == ic {
                    return false;
                }
                let p = polygon[other];
                cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0
            });
            if !contains_other {
                triangles.push([ia, ib, ic]);
                remaining.remove(i);
                clipped = true;
                break;
            }
        }
        if !clipped {
            break;
        }
    }
    if remaining.len() == 3 {
        triangles.push([remaining[0], remaining[1], remaining[2]]);
    }
    triangles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(polygon: &[[f32; 2]], triangles: &[[usize; 3]]) -> f32 {
        triangles
            .iter()
            .map(|t| {
                let (a, b, c) = (polygon[t[0]], polygon[t[1]], polygon[t[2]]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0
            })
            .sum()
    }

    #[test]
    fn cuboid_has_twelve_outward_triangles() {
        let mut mesh = Mesh::default();
        mesh.cuboid([0.0; 3], [1.0, 2.0, 3.0]);
        assert_eq!(mesh.triangle_count(), 12);
        // Every normal points away from the centre.
        let center = [0.5, 1.0, 1.5];
        for (p, n) in mesh
            .positions
            .as_chunks::<3>()
            .0
            .iter()
            .zip(mesh.normals.as_chunks::<3>().0)
        {
            let dot =
                (p[0] - center[0]) * n[0] + (p[1] - center[1]) * n[1] + (p[2] - center[2]) * n[2];
            assert!(dot > 0.0);
        }
        assert_eq!(mesh.bounds(), Some(([0.0; 3], [1.0, 2.0, 3.0])));
    }

    #[test]
    fn triangulates_convex_and_concave_polygons_completely() {
        let square = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let t = triangulate(&square);
        assert_eq!(t.len(), 2);
        assert_eq!(area(&square, &t), 16.0);

        let l_shape = [
            [0.0, 0.0],
            [4.0, 0.0],
            [4.0, 2.0],
            [2.0, 2.0],
            [2.0, 4.0],
            [0.0, 4.0],
        ];
        let t = triangulate(&l_shape);
        assert_eq!(t.len(), 4);
        assert_eq!(area(&l_shape, &t), 12.0);

        // The same with opposite winding.
        let mut reversed = l_shape;
        reversed.reverse();
        assert_eq!(area(&reversed, &triangulate(&reversed)), 12.0);
    }
}
