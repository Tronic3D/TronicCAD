//! A small stroke font for silkscreen text, drawn for this project.
//! Glyphs sit on a grid 4 wide and 6 high, with Y pointing down.

use tronic_schematic::Point;

/// Polylines per glyph, as coordinate pairs.
fn glyph(c: char) -> &'static [&'static [(i64, i64)]] {
    match c.to_ascii_uppercase() {
        'A' => &[&[(0, 6), (0, 2), (2, 0), (4, 2), (4, 6)], &[(0, 3), (4, 3)]],
        'B' => &[
            &[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)],
            &[(3, 3), (4, 2), (4, 1), (3, 0), (0, 0)],
        ],
        'C' => &[&[
            (4, 1),
            (3, 0),
            (1, 0),
            (0, 1),
            (0, 5),
            (1, 6),
            (3, 6),
            (4, 5),
        ]],
        'D' => &[&[(0, 0), (0, 6), (3, 6), (4, 5), (4, 1), (3, 0), (0, 0)]],
        'E' => &[&[(4, 0), (0, 0), (0, 6), (4, 6)], &[(0, 3), (3, 3)]],
        'F' => &[&[(4, 0), (0, 0), (0, 6)], &[(0, 3), (3, 3)]],
        'G' => &[&[
            (4, 1),
            (3, 0),
            (1, 0),
            (0, 1),
            (0, 5),
            (1, 6),
            (3, 6),
            (4, 5),
            (4, 3),
            (2, 3),
        ]],
        'H' => &[&[(0, 0), (0, 6)], &[(4, 0), (4, 6)], &[(0, 3), (4, 3)]],
        'I' => &[&[(1, 0), (3, 0)], &[(2, 0), (2, 6)], &[(1, 6), (3, 6)]],
        'J' => &[&[(4, 0), (4, 5), (3, 6), (1, 6), (0, 5)]],
        'K' => &[&[(0, 0), (0, 6)], &[(4, 0), (0, 3), (4, 6)]],
        'L' => &[&[(0, 0), (0, 6), (4, 6)]],
        'M' => &[&[(0, 6), (0, 0), (2, 3), (4, 0), (4, 6)]],
        'N' => &[&[(0, 6), (0, 0), (4, 6), (4, 0)]],
        'O' | '0' => &[&[
            (1, 0),
            (3, 0),
            (4, 1),
            (4, 5),
            (3, 6),
            (1, 6),
            (0, 5),
            (0, 1),
            (1, 0),
        ]],
        'P' => &[&[(0, 6), (0, 0), (3, 0), (4, 1), (4, 2), (3, 3), (0, 3)]],
        'Q' => &[
            &[
                (1, 0),
                (3, 0),
                (4, 1),
                (4, 5),
                (3, 6),
                (1, 6),
                (0, 5),
                (0, 1),
                (1, 0),
            ],
            &[(2, 4), (4, 6)],
        ],
        'R' => &[
            &[(0, 6), (0, 0), (3, 0), (4, 1), (4, 2), (3, 3), (0, 3)],
            &[(2, 3), (4, 6)],
        ],
        'S' => &[&[
            (4, 1),
            (3, 0),
            (1, 0),
            (0, 1),
            (0, 2),
            (1, 3),
            (3, 3),
            (4, 4),
            (4, 5),
            (3, 6),
            (1, 6),
            (0, 5),
        ]],
        'T' => &[&[(0, 0), (4, 0)], &[(2, 0), (2, 6)]],
        'U' => &[&[(0, 0), (0, 5), (1, 6), (3, 6), (4, 5), (4, 0)]],
        'V' => &[&[(0, 0), (2, 6), (4, 0)]],
        'W' => &[&[(0, 0), (1, 6), (2, 3), (3, 6), (4, 0)]],
        'X' => &[&[(0, 0), (4, 6)], &[(4, 0), (0, 6)]],
        'Y' => &[&[(0, 0), (2, 3), (4, 0)], &[(2, 3), (2, 6)]],
        'Z' => &[&[(0, 0), (4, 0), (0, 6), (4, 6)]],
        '1' => &[&[(1, 1), (2, 0), (2, 6)], &[(1, 6), (3, 6)]],
        '2' => &[&[(0, 1), (1, 0), (3, 0), (4, 1), (4, 2), (0, 6), (4, 6)]],
        '3' => &[
            &[
                (0, 1),
                (1, 0),
                (3, 0),
                (4, 1),
                (4, 2),
                (3, 3),
                (4, 4),
                (4, 5),
                (3, 6),
                (1, 6),
                (0, 5),
            ],
            &[(1, 3), (3, 3)],
        ],
        '4' => &[&[(3, 6), (3, 0), (0, 4), (4, 4)]],
        '5' => &[&[
            (4, 0),
            (0, 0),
            (0, 3),
            (3, 3),
            (4, 4),
            (4, 5),
            (3, 6),
            (1, 6),
            (0, 5),
        ]],
        '6' => &[&[
            (4, 1),
            (3, 0),
            (1, 0),
            (0, 1),
            (0, 5),
            (1, 6),
            (3, 6),
            (4, 5),
            (4, 4),
            (3, 3),
            (0, 3),
        ]],
        '7' => &[&[(0, 0), (4, 0), (1, 6)]],
        '8' => &[&[
            (1, 3),
            (0, 2),
            (0, 1),
            (1, 0),
            (3, 0),
            (4, 1),
            (4, 2),
            (3, 3),
            (1, 3),
            (0, 4),
            (0, 5),
            (1, 6),
            (3, 6),
            (4, 5),
            (4, 4),
            (3, 3),
        ]],
        '9' => &[&[
            (4, 3),
            (1, 3),
            (0, 2),
            (0, 1),
            (1, 0),
            (3, 0),
            (4, 1),
            (4, 5),
            (3, 6),
            (1, 6),
            (0, 5),
        ]],
        '-' => &[&[(1, 3), (3, 3)]],
        '_' => &[&[(0, 6), (4, 6)]],
        '+' => &[&[(0, 3), (4, 3)], &[(2, 1), (2, 5)]],
        '.' => &[&[(2, 6), (2, 5)]],
        // Characters without a glyph appear as a box, never silently vanish.
        _ => &[&[(0, 0), (4, 0), (4, 6), (0, 6), (0, 0)]],
    }
}

/// Strokes of a line of text, centred above `anchor`. Mirrored text reads
/// correctly when looking at the bottom side of the board.
pub fn text_strokes(text: &str, anchor: Point, height: i64, mirrored: bool) -> Vec<(Point, Point)> {
    let unit = height / 6;
    let advance = 6 * unit;
    let count = text.chars().count() as i64;
    let width = advance * count - 2 * unit;
    let mut strokes = Vec::new();
    for (index, c) in text.chars().enumerate() {
        let offset = -width / 2 + advance * index as i64;
        for line in glyph(c) {
            for pair in line.windows(2) {
                let place = |(x, y): (i64, i64)| {
                    let local = offset + x * unit;
                    Point::new(
                        anchor.x + if mirrored { -local } else { local },
                        anchor.y - height + y * unit,
                    )
                };
                strokes.push((place(pair[0]), place(pair[1])));
            }
        }
    }
    strokes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_stays_on_its_grid() {
        for c in ('A'..='Z')
            .chain('0'..='9')
            .chain(['-', '_', '+', '.', '#'])
        {
            for line in glyph(c) {
                assert!(line.len() >= 2, "{c}");
                for (x, y) in line.iter() {
                    assert!((0..=4).contains(x) && (0..=6).contains(y), "{c}");
                }
            }
        }
    }

    #[test]
    fn text_is_centred_on_the_anchor_and_mirrors() {
        let strokes = text_strokes("I", Point::new(0, 0), 6, false);
        let xs: Vec<i64> = strokes.iter().flat_map(|(a, b)| [a.x, b.x]).collect();
        assert_eq!(
            xs.iter().min().map(|x| x + xs.iter().max().unwrap_or(x)),
            Some(0)
        );

        let normal = text_strokes("L", Point::new(0, 0), 6, false);
        let mirrored = text_strokes("L", Point::new(0, 0), 6, true);
        assert_eq!(normal[0].0.x, -mirrored[0].0.x);
        assert_eq!(normal[0].0.y, mirrored[0].0.y);
    }

    #[test]
    fn unknown_characters_stay_visible() {
        assert!(!text_strokes("电", Point::new(0, 0), 6, false).is_empty());
    }
}
