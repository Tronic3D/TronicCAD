//! Reads 3D models: STL (binary and text), Wavefront OBJ and VRML 2 (WRL).
//!
//! STEP is not supported: reading it reliably needs a CAD kernel whose licence
//! has not been cleared (see IP_COMPLIANCE.md).

use thiserror::Error;

use crate::mesh::{Mesh, V3};

/// Upper limit, to keep projects and the viewer usable.
pub const MAX_TRIANGLES: usize = 200_000;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ImportError {
    #[error("the file format is not supported")]
    UnsupportedFormat,
    #[error("STEP files are not supported")]
    StepNotSupported,
    #[error("the file is damaged or not a valid model")]
    Invalid,
    #[error("the model contains no triangles")]
    Empty,
    #[error("the model has too many triangles")]
    TooLarge,
}

fn checked(mesh: Mesh) -> Result<Mesh, ImportError> {
    if mesh.positions.is_empty() {
        return Err(ImportError::Empty);
    }
    if mesh.triangle_count() > MAX_TRIANGLES {
        return Err(ImportError::TooLarge);
    }
    if mesh.positions.iter().any(|value| !value.is_finite()) {
        return Err(ImportError::Invalid);
    }
    Ok(mesh)
}

fn parse_stl(bytes: &[u8]) -> Result<Mesh, ImportError> {
    // A binary file's length follows from its triangle count, which tells it
    // apart from text files that also begin with "solid".
    if bytes.len() >= 84 {
        let count = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
        if count.checked_mul(50).and_then(|n| n.checked_add(84)) == Some(bytes.len()) {
            if count > MAX_TRIANGLES {
                return Err(ImportError::TooLarge);
            }
            let mut mesh = Mesh::default();
            for record in bytes[84..].as_chunks::<50>().0 {
                let corner = |offset: usize| -> V3 {
                    let value = |at: usize| {
                        f32::from_le_bytes([
                            record[at],
                            record[at + 1],
                            record[at + 2],
                            record[at + 3],
                        ])
                    };
                    [value(offset), value(offset + 4), value(offset + 8)]
                };
                mesh.triangle(corner(12), corner(24), corner(36));
            }
            return checked(mesh);
        }
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ImportError::Invalid)?;
    if !text.trim_start().to_ascii_lowercase().starts_with("solid") {
        return Err(ImportError::Invalid);
    }
    let mut mesh = Mesh::default();
    let mut corners: Vec<V3> = Vec::with_capacity(3);
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if parts
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("vertex"))
        {
            let values: Vec<f32> = parts.filter_map(|p| p.parse().ok()).collect();
            let [x, y, z] = values[..] else {
                return Err(ImportError::Invalid);
            };
            corners.push([x, y, z]);
            if corners.len() == 3 {
                mesh.triangle(corners[0], corners[1], corners[2]);
                corners.clear();
                if mesh.triangle_count() > MAX_TRIANGLES {
                    return Err(ImportError::TooLarge);
                }
            }
        }
    }
    checked(mesh)
}

fn parse_obj(bytes: &[u8]) -> Result<Mesh, ImportError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ImportError::Invalid)?;
    let mut vertices: Vec<V3> = Vec::new();
    let mut mesh = Mesh::default();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("");
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("v") => {
                let values: Vec<f32> = parts.take(3).filter_map(|p| p.parse().ok()).collect();
                let [x, y, z] = values[..] else {
                    return Err(ImportError::Invalid);
                };
                vertices.push([x, y, z]);
            }
            Some("f") => {
                let mut corners = Vec::new();
                for part in parts {
                    let index: i64 = part
                        .split('/')
                        .next()
                        .and_then(|p| p.parse().ok())
                        .ok_or(ImportError::Invalid)?;
                    // Indices count from one; negative ones count from the end.
                    let resolved = if index < 0 {
                        vertices.len() as i64 + index
                    } else {
                        index - 1
                    };
                    let corner = usize::try_from(resolved)
                        .ok()
                        .and_then(|i| vertices.get(i))
                        .ok_or(ImportError::Invalid)?;
                    corners.push(*corner);
                }
                for i in 1..corners.len().saturating_sub(1) {
                    mesh.triangle(corners[0], corners[i], corners[i + 1]);
                }
                if mesh.triangle_count() > MAX_TRIANGLES {
                    return Err(ImportError::TooLarge);
                }
            }
            _ => {}
        }
    }
    checked(mesh)
}

/// Reads `IndexedFaceSet` nodes. Transforms and other geometry nodes are not
/// evaluated, which is fine for most models exported per part.
fn parse_wrl(bytes: &[u8]) -> Result<Mesh, ImportError> {
    let text = String::from_utf8_lossy(bytes);
    if !text.trim_start().starts_with("#VRML") {
        return Err(ImportError::Invalid);
    }
    let mut mesh = Mesh::default();
    let mut rest: &str = &text;
    let list_after = |source: &str, keyword: &str| -> Option<(String, usize)> {
        let at = source.find(keyword)?;
        let open = at + source[at..].find('[')?;
        let close = open + source[open..].find(']')?;
        Some((source[open + 1..close].to_owned(), close + 1))
    };
    while let Some(start) = rest.find("IndexedFaceSet") {
        rest = &rest[start..];
        let (points, _) = list_after(rest, "point").ok_or(ImportError::Invalid)?;
        let (indices, consumed) = list_after(rest, "coordIndex").ok_or(ImportError::Invalid)?;
        let numbers: Vec<f32> = points
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|p| !p.is_empty())
            .map(|p| p.parse().map_err(|_| ImportError::Invalid))
            .collect::<Result<_, _>>()?;
        let vertices: Vec<V3> = numbers.as_chunks::<3>().0.to_vec();
        let mut face: Vec<V3> = Vec::new();
        for part in indices
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|p| !p.is_empty())
        {
            let index: i64 = part.parse().map_err(|_| ImportError::Invalid)?;
            if index < 0 {
                for i in 1..face.len().saturating_sub(1) {
                    mesh.triangle(face[0], face[i], face[i + 1]);
                }
                face.clear();
                if mesh.triangle_count() > MAX_TRIANGLES {
                    return Err(ImportError::TooLarge);
                }
            } else {
                face.push(*vertices.get(index as usize).ok_or(ImportError::Invalid)?);
            }
        }
        rest = &rest[consumed..];
    }
    checked(mesh)
}

/// Reads a model; the file extension selects the format.
pub fn parse_model(extension: &str, bytes: &[u8]) -> Result<Mesh, ImportError> {
    match extension.to_ascii_lowercase().as_str() {
        "stl" => parse_stl(bytes),
        "obj" => parse_obj(bytes),
        "wrl" | "vrml" => parse_wrl(bytes),
        "step" | "stp" => Err(ImportError::StepNotSupported),
        _ => Err(ImportError::UnsupportedFormat),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const TEXT_STL: &str = "solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid t\n";

    fn binary_stl(triangles: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; 80];
        bytes.extend_from_slice(&triangles.to_le_bytes());
        for _ in 0..triangles {
            bytes.extend_from_slice(&[0u8; 12]);
            for value in [0f32, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 0.0] {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            bytes.extend_from_slice(&[0u8; 2]);
        }
        bytes
    }

    #[test]
    fn reads_text_stl() {
        let mesh = parse_model("stl", TEXT_STL.as_bytes()).unwrap();
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.bounds(), Some(([0.0; 3], [1.0, 1.0, 0.0])));
    }

    #[test]
    fn reads_binary_stl() {
        let mesh = parse_model("STL", &binary_stl(3)).unwrap();
        assert_eq!(mesh.triangle_count(), 3);
        assert_eq!(mesh.bounds().unwrap().1, [2.0, 2.0, 0.0]);
    }

    #[test]
    fn binary_stl_beginning_with_solid_is_still_binary() {
        let mut bytes = binary_stl(1);
        bytes[..5].copy_from_slice(b"solid");
        assert_eq!(parse_model("stl", &bytes).unwrap().triangle_count(), 1);
    }

    #[test]
    fn reads_obj_with_faces_of_any_size_and_index_forms() {
        let obj = "# comment\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1/1/1 2/2/2 3/3/3 4/4/4\nf -4 -3 -2\n";
        assert_eq!(
            parse_model("obj", obj.as_bytes()).unwrap().triangle_count(),
            3
        );
    }

    #[test]
    fn reads_vrml_face_sets() {
        let wrl = "#VRML V2.0 utf8\nShape { geometry IndexedFaceSet { coord Coordinate { point [ 0 0 0, 1 0 0, 1 1 0, 0 1 0 ] } coordIndex [ 0, 1, 2, 3, -1 ] } }\nShape { geometry IndexedFaceSet { coord Coordinate { point [ 0 0 1, 1 0 1, 1 1 1 ] } coordIndex [ 0 1 2 -1 ] } }\n";
        assert_eq!(
            parse_model("wrl", wrl.as_bytes()).unwrap().triangle_count(),
            3
        );
    }

    #[test]
    fn reports_step_as_not_supported() {
        for extension in ["step", "stp", "STEP"] {
            assert_eq!(
                parse_model(extension, b"ISO-10303-21;"),
                Err(ImportError::StepNotSupported)
            );
        }
        assert_eq!(parse_model("dxf", b""), Err(ImportError::UnsupportedFormat));
    }

    #[test]
    fn rejects_damaged_files() {
        for (extension, bytes) in [
            ("stl", &b"not a model"[..]),
            ("stl", b"solid x\nvertex 1 2\n"),
            ("obj", b"v 1 2\n"),
            ("obj", b"v 0 0 0\nf 1 2 3\n"),
            ("obj", b"f x y z\n"),
            ("wrl", b"no header"),
            ("wrl", b"#VRML V2.0 utf8\nIndexedFaceSet { coord Coordinate { point [ 0 0 0 ] } coordIndex [ 5 -1 ] }"),
            ("wrl", b"#VRML V2.0 utf8\nIndexedFaceSet { }"),
        ] {
            assert_eq!(
                parse_model(extension, bytes),
                Err(ImportError::Invalid),
                "{extension}: {}",
                String::from_utf8_lossy(bytes)
            );
        }
    }

    #[test]
    fn rejects_empty_models_and_broken_numbers() {
        assert_eq!(parse_model("obj", b"# nothing\n"), Err(ImportError::Empty));
        assert_eq!(
            parse_model("stl", b"solid x\nendsolid x\n"),
            Err(ImportError::Empty)
        );
        assert_eq!(
            parse_model(
                "stl",
                b"solid x\nvertex NaN 0 0\nvertex 0 0 0\nvertex 0 1 0\n"
            ),
            Err(ImportError::Invalid)
        );
    }

    #[test]
    fn a_lying_triangle_count_cannot_exhaust_memory() {
        let mut bytes = vec![0u8; 80];
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse_model("stl", &bytes), Err(ImportError::Invalid));
    }
}
