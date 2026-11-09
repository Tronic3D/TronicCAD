//! Keeps wires tidy without changing what they connect.

use std::collections::HashMap;

use crate::geometry::Point;
use crate::model::Segment;

/// A segment along its line: start, end, id.
type LinePart = (i64, i64, u64);

/// Removes zero-length segments and merges overlapping or touching segments on
/// the same line. An end point is kept if a perpendicular segment passes
/// through it: there the end point forms a connection, a through line would not.
pub fn normalize_segments(segments: &mut Vec<Segment>) {
    segments.retain(|segment| segment.a != segment.b);

    // (horizontal, line coordinate) -> (start, end, id), start < end
    let mut lines: HashMap<(bool, i64), Vec<LinePart>> = HashMap::new();
    for segment in segments.iter() {
        let horizontal = segment.a.y == segment.b.y;
        let (line, s, e) = if horizontal {
            (segment.a.y, segment.a.x, segment.b.x)
        } else {
            (segment.a.x, segment.a.y, segment.b.y)
        };
        lines
            .entry((horizontal, line))
            .or_default()
            .push((s.min(e), s.max(e), segment.id));
    }

    let passes_through = |horizontal: bool, line: i64, coord: i64| {
        // Perpendicular segments on this coordinate, strictly containing the line.
        lines
            .get(&(!horizontal, coord))
            .is_some_and(|others| others.iter().any(|&(s, e, _)| s < line && line < e))
    };

    let mut merged_any = false;
    let mut result = Vec::with_capacity(segments.len());
    let mut keys: Vec<_> = lines.keys().copied().collect();
    keys.sort_unstable();
    for key in keys {
        let (horizontal, line) = key;
        let mut parts = lines[&key].clone();
        parts.sort_unstable();
        let mut iter = parts.into_iter();
        let Some(mut current) = iter.next() else {
            continue;
        };
        let emit = |(s, e, id): (i64, i64, u64), out: &mut Vec<Segment>| {
            let (a, b) = if horizontal {
                (Point::new(s, line), Point::new(e, line))
            } else {
                (Point::new(line, s), Point::new(line, e))
            };
            out.push(Segment { id, a, b });
        };
        for next in iter {
            let can_merge = next.0 <= current.1
                && !passes_through(horizontal, line, current.1.min(next.1))
                && !passes_through(horizontal, line, next.0);
            if can_merge {
                current.1 = current.1.max(next.1);
                current.2 = current.2.min(next.2);
                merged_any = true;
            } else {
                emit(current, &mut result);
                current = next;
            }
        }
        emit(current, &mut result);
    }
    if merged_any {
        result.sort_by_key(|segment| segment.id);
        *segments = result;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(id: u64, a: (i64, i64), b: (i64, i64)) -> Segment {
        Segment {
            id,
            a: Point::new(a.0, a.1),
            b: Point::new(b.0, b.1),
        }
    }

    #[test]
    fn merges_touching_and_overlapping_segments() {
        let mut wires = vec![
            seg(1, (0, 0), (5, 0)),
            seg(2, (5, 0), (9, 0)),
            seg(3, (8, 0), (12, 0)),
        ];
        normalize_segments(&mut wires);
        assert_eq!(wires, vec![seg(1, (0, 0), (12, 0))]);
    }

    #[test]
    fn removes_zero_length_segments() {
        let mut wires = vec![seg(1, (3, 3), (3, 3)), seg(2, (0, 0), (1, 0))];
        normalize_segments(&mut wires);
        assert_eq!(wires, vec![seg(2, (0, 0), (1, 0))]);
    }

    #[test]
    fn keeps_an_end_point_that_connects_to_a_crossing_wire() {
        let mut wires = vec![
            seg(1, (0, 0), (5, 0)),
            seg(2, (5, 0), (9, 0)),
            seg(3, (5, -3), (5, 3)),
        ];
        let before = wires.clone();
        normalize_segments(&mut wires);
        assert_eq!(wires, before);
    }

    #[test]
    fn merges_across_a_wire_that_only_ends_there() {
        let mut wires = vec![
            seg(1, (0, 0), (5, 0)),
            seg(2, (5, 0), (9, 0)),
            seg(3, (5, 0), (5, 3)),
        ];
        normalize_segments(&mut wires);
        assert_eq!(wires, vec![seg(1, (0, 0), (9, 0)), seg(3, (5, 0), (5, 3))]);
    }

    #[test]
    fn leaves_separate_segments_alone() {
        let mut wires = vec![seg(1, (0, 0), (5, 0)), seg(2, (6, 0), (9, 0))];
        let before = wires.clone();
        normalize_segments(&mut wires);
        assert_eq!(wires, before);
    }
}
