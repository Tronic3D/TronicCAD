//! 3D scene of a board, model import and mechanical checks.
//! Specification: `docs/specs/view3d.md`.
//!
//! All lengths here are millimetres as f32 or f64. The scene's X and Y follow the
//! board with Y turned upwards, Z points out of the top side.

pub mod collision;
pub mod import;
pub mod mesh;
pub mod scene;

pub use collision::{check_mechanics, Finding, MechanicsReport};
pub use import::{parse_model, ImportError};
pub use mesh::Mesh;
pub use scene::{build_scene, Scene};
