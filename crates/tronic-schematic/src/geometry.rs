//! Integer geometry in nanometres. Y points down.

use serde::{Deserialize, Serialize};

/// Schematic grid: 1.27 mm.
pub const GRID: i64 = 1_270_000;

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

impl Point {
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }

    /// A point given in grid units.
    pub const fn grid(x: i64, y: i64) -> Self {
        Self {
            x: x * GRID,
            y: y * GRID,
        }
    }

    pub const fn offset(self, delta: Point) -> Self {
        Self {
            x: self.x + delta.x,
            y: self.y + delta.y,
        }
    }

    pub fn snapped(self) -> Self {
        let snap = |v: i64| (v as f64 / GRID as f64).round() as i64 * GRID;
        Self {
            x: snap(self.x),
            y: snap(self.y),
        }
    }
}

/// Clockwise rotation on screen, in steps of 90 degrees.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub enum Rotation {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

impl TryFrom<u16> for Rotation {
    type Error = String;
    fn try_from(value: u16) -> Result<Self, String> {
        match value {
            0 => Ok(Self::R0),
            90 => Ok(Self::R90),
            180 => Ok(Self::R180),
            270 => Ok(Self::R270),
            other => Err(format!("invalid rotation {other}")),
        }
    }
}

impl From<Rotation> for u16 {
    fn from(value: Rotation) -> u16 {
        match value {
            Rotation::R0 => 0,
            Rotation::R90 => 90,
            Rotation::R180 => 180,
            Rotation::R270 => 270,
        }
    }
}

impl Rotation {
    pub fn clockwise(self) -> Self {
        match self {
            Self::R0 => Self::R90,
            Self::R90 => Self::R180,
            Self::R180 => Self::R270,
            Self::R270 => Self::R0,
        }
    }

    pub fn negated(self) -> Self {
        match self {
            Self::R90 => Self::R270,
            Self::R270 => Self::R90,
            other => other,
        }
    }

    /// Rotates a vector around the origin.
    pub fn apply(self, p: Point) -> Point {
        match self {
            Self::R0 => p,
            Self::R90 => Point::new(-p.y, p.x),
            Self::R180 => Point::new(-p.x, -p.y),
            Self::R270 => Point::new(p.y, -p.x),
        }
    }
}

/// Symbol space to sheet space: mirror X, then rotate, then translate.
pub fn place(local: Point, at: Point, rotation: Rotation, mirror: bool) -> Point {
    let mirrored = if mirror {
        Point::new(-local.x, local.y)
    } else {
        local
    };
    rotation.apply(mirrored).offset(at)
}

/// A rigid transformation of selected elements.
#[derive(Debug, Clone, Copy)]
pub enum Transform {
    Translate(Point),
    RotateClockwise(Point),
    MirrorX(Point),
}

impl Transform {
    pub fn point(self, p: Point) -> Point {
        match self {
            Self::Translate(delta) => p.offset(delta),
            Self::RotateClockwise(pivot) => Rotation::R90
                .apply(Point::new(p.x - pivot.x, p.y - pivot.y))
                .offset(pivot),
            Self::MirrorX(pivot) => Point::new(2 * pivot.x - p.x, p.y),
        }
    }

    /// New orientation of an element placed by rotation and mirror.
    pub fn orientation(self, rotation: Rotation, mirror: bool) -> (Rotation, bool) {
        match self {
            Self::Translate(_) => (rotation, mirror),
            Self::RotateClockwise(_) => (rotation.clockwise(), mirror),
            // Mx R(a) = R(-a) Mx
            Self::MirrorX(_) => (rotation.negated(), !mirror),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_is_clockwise_on_a_y_down_screen() {
        // A point to the right moves downwards.
        assert_eq!(Rotation::R90.apply(Point::new(1, 0)), Point::new(0, 1));
        assert_eq!(Rotation::R180.apply(Point::new(1, 2)), Point::new(-1, -2));
        assert_eq!(Rotation::R270.apply(Point::new(1, 0)), Point::new(0, -1));
    }

    #[test]
    fn place_mirrors_before_rotating() {
        let local = Point::new(2, 1);
        assert_eq!(
            place(local, Point::new(10, 10), Rotation::R90, true),
            Point::new(9, 8)
        );
    }

    #[test]
    fn transforming_a_placed_point_matches_the_new_orientation() {
        let at = Point::grid(3, 4);
        let local = Point::grid(2, 1);
        let pivot = Point::grid(1, 1);
        for rotation in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
            for mirror in [false, true] {
                for transform in [Transform::RotateClockwise(pivot), Transform::MirrorX(pivot)] {
                    let before = place(local, at, rotation, mirror);
                    let (new_rotation, new_mirror) = transform.orientation(rotation, mirror);
                    let after = place(local, transform.point(at), new_rotation, new_mirror);
                    assert_eq!(transform.point(before), after, "{rotation:?} {mirror}");
                }
            }
        }
    }

    #[test]
    fn snapping_goes_to_the_nearest_grid_point() {
        assert_eq!(
            Point::new(GRID + GRID / 3, -GRID - GRID / 3).snapped(),
            Point::grid(1, -1)
        );
    }
}
