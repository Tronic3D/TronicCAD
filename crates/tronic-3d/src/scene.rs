//! Builds what the viewer shows: the board body and the parts.

use serde::Serialize;
use tronic_pcb::{Board, BodyKind, BodyPart, FootprintInstance, Mechanics, MM};
use tronic_schematic::{ElementId, Point, Rotation};

use crate::collision::{check_mechanics, MechanicsReport};
use crate::mesh::{triangulate, Mesh};

const FALLBACK_HEIGHT: f64 = 1.0;
const FALLBACK_COLOR: [u8; 3] = [150, 150, 150];

/// A coloured group of triangles.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ColoredMesh {
    pub color: [u8; 3],
    pub mesh: Mesh,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScenePart {
    pub id: ElementId,
    pub reference: String,
    pub footprint: String,
    /// File name of an imported model.
    pub model_name: Option<String>,
    /// False if the footprint has no model and a plain block stands in.
    pub has_model: bool,
    pub meshes: Vec<ColoredMesh>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scene {
    pub thickness: f32,
    pub rules: Mechanics,
    /// Top face of the board, outline corners in scene coordinates.
    pub outline: Vec<[f32; 2]>,
    pub board: Mesh,
    pub parts: Vec<ScenePart>,
    pub mechanics: MechanicsReport,
}

fn mm(nm: i64) -> f64 {
    nm as f64 / MM as f64
}

/// A body in footprint space, in millimetres, Z above the mounting surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldBox {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

/// Where a footprint point lands, Y turned upwards, Z on the board's axis.
fn place(instance: &FootprintInstance, x: f64, y: f64) -> (f64, f64) {
    let local = Point::new(
        (x * MM as f64).round() as i64,
        (y * MM as f64).round() as i64,
    );
    let p = tronic_pcb::place_on_board(local, instance.at, instance.rotation, instance.bottom);
    (mm(p.x), -mm(p.y))
}

fn z_range(instance: &FootprintInstance, thickness: f64, z0: f64, z1: f64) -> (f64, f64) {
    if instance.bottom {
        (-thickness - z1, -thickness - z0)
    } else {
        (z0, z1)
    }
}

/// The bounding box of a body part on the board.
pub fn world_box(instance: &FootprintInstance, thickness: f64, body: &BodyPart) -> WorldBox {
    let (cx, cy) = (mm(body.center.x), mm(body.center.y));
    let (hx, hy) = (mm(body.size.x) / 2.0, mm(body.size.y) / 2.0);
    let (ax, ay) = place(instance, cx - hx, cy - hy);
    let (bx, by) = place(instance, cx + hx, cy + hy);
    let (z0, z1) = z_range(instance, thickness, mm(body.z), mm(body.z + body.height));
    WorldBox {
        min: [ax.min(bx), ay.min(by), z0],
        max: [ax.max(bx), ay.max(by), z1],
    }
}

/// The block standing in for a footprint without a model.
pub fn fallback_box(
    board: &Board,
    instance: &FootprintInstance,
    thickness: f64,
) -> Option<WorldBox> {
    let (min, max) = board.courtyard(instance)?;
    let (z0, z1) = z_range(instance, thickness, 0.0, FALLBACK_HEIGHT);
    Some(WorldBox {
        min: [mm(min.x), -mm(max.y), z0],
        max: [mm(max.x), -mm(min.y), z1],
    })
}

fn body_mesh(instance: &FootprintInstance, thickness: f64, body: &BodyPart) -> Mesh {
    let world = world_box(instance, thickness, body);
    let mut mesh = Mesh::default();
    let to = |values: [f64; 3]| values.map(|v| v as f32);
    match body.kind {
        BodyKind::Box => mesh.cuboid(to(world.min), to(world.max)),
        BodyKind::Cylinder => mesh.cylinder(
            [
                ((world.min[0] + world.max[0]) / 2.0) as f32,
                ((world.min[1] + world.max[1]) / 2.0) as f32,
            ],
            ((world.max[0] - world.min[0]) / 2.0) as f32,
            world.min[2] as f32,
            world.max[2] as f32,
        ),
    }
    mesh
}

/// Moves an imported model onto its part: same rule as footprint geometry.
fn placed_model(
    instance: &FootprintInstance,
    thickness: f64,
    source: &tronic_pcb::ImportedModel,
) -> Mesh {
    let mut mesh = Mesh::default();
    let turn = |x: f64, y: f64| match instance.rotation {
        Rotation::R0 => (x, y),
        Rotation::R90 => (-y, x),
        Rotation::R180 => (-x, -y),
        Rotation::R270 => (y, -x),
    };
    let (ox, oy) = (mm(instance.at.x), mm(instance.at.y));
    let scale = f64::from(source.scale);
    let corner = |p: &[f32]| -> [f32; 3] {
        // Model Y points up, footprint Y down; the bottom side mirrors X and Z.
        let x = f64::from(p[0]) * scale;
        let y = -f64::from(p[1]) * scale;
        let z = f64::from(p[2]) * scale + f64::from(source.z_offset);
        let (mx, my) = if instance.bottom { (-x, y) } else { (x, y) };
        let (tx, ty) = turn(mx, my);
        let world_z = if instance.bottom { -thickness - z } else { z };
        [(ox + tx) as f32, -(oy + ty) as f32, world_z as f32]
    };
    for triangle in source.positions.as_chunks::<9>().0 {
        let a = corner(&triangle[0..3]);
        let b = corner(&triangle[3..6]);
        let c = corner(&triangle[6..9]);
        // A mirrored model must keep its faces pointing outwards.
        if instance.bottom {
            mesh.triangle(a, c, b);
        } else {
            mesh.triangle(a, b, c);
        }
    }
    mesh
}

fn board_mesh(outline: &[[f32; 2]], thickness: f32) -> Mesh {
    let mut mesh = Mesh::default();
    for [a, b, c] in triangulate(outline) {
        let (pa, pb, pc) = (outline[a], outline[b], outline[c]);
        mesh.triangle(
            [pa[0], pa[1], 0.0],
            [pb[0], pb[1], 0.0],
            [pc[0], pc[1], 0.0],
        );
        mesh.triangle(
            [pa[0], pa[1], -thickness],
            [pc[0], pc[1], -thickness],
            [pb[0], pb[1], -thickness],
        );
    }
    let area: f32 = (0..outline.len())
        .map(|i| {
            let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
            a[0] * b[1] - a[1] * b[0]
        })
        .sum();
    for i in 0..outline.len() {
        let (mut a, mut b) = (outline[i], outline[(i + 1) % outline.len()]);
        if area < 0.0 {
            std::mem::swap(&mut a, &mut b);
        }
        mesh.quad(
            [a[0], a[1], -thickness],
            [b[0], b[1], -thickness],
            [b[0], b[1], 0.0],
            [a[0], a[1], 0.0],
        );
    }
    mesh
}

/// Builds the 3D scene of a board.
pub fn build_scene(board: &Board) -> Scene {
    let thickness = mm(board.mechanics.thickness);
    let outline: Vec<[f32; 2]> = board
        .outline
        .iter()
        .map(|p| [mm(p.x) as f32, -mm(p.y) as f32])
        .collect();
    let mut parts = Vec::with_capacity(board.footprints.len());
    for instance in board.footprints.iter() {
        let Some(definition) = board.footprint_defs.get(&instance.footprint) else {
            continue;
        };
        let mut meshes = Vec::new();
        let has_model = definition.model.is_some() || !definition.body.is_empty();
        if let Some(model) = &definition.model {
            meshes.push(ColoredMesh {
                color: [170, 170, 180],
                mesh: placed_model(instance, thickness, model),
            });
        } else if definition.body.is_empty() {
            if let Some(world) = fallback_box(board, instance, thickness) {
                let mut mesh = Mesh::default();
                mesh.cuboid(world.min.map(|v| v as f32), world.max.map(|v| v as f32));
                meshes.push(ColoredMesh {
                    color: FALLBACK_COLOR,
                    mesh,
                });
            }
        } else {
            for body in &definition.body {
                meshes.push(ColoredMesh {
                    color: body.color,
                    mesh: body_mesh(instance, thickness, body),
                });
            }
        }
        parts.push(ScenePart {
            id: instance.id,
            reference: instance.reference.clone(),
            footprint: instance.footprint.clone(),
            model_name: definition.model.as_ref().map(|model| model.name.clone()),
            has_model,
            meshes,
        });
    }
    Scene {
        thickness: thickness as f32,
        rules: board.mechanics.clone(),
        board: if outline.len() >= 3 {
            board_mesh(&outline, thickness as f32)
        } else {
            Mesh::default()
        },
        outline,
        parts,
        mechanics: check_mechanics(board),
    }
}
