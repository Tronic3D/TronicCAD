//! The parts of a project that people work on separately, and how two states that
//! touched different parts come together again.
//!
//! The rule is deliberately coarse: an area changed on both sides is a conflict for
//! a person, never something to merge by guessing. Connections between schematic
//! and board are not assumed to fit afterwards but compared, and the result reports
//! the differences, as after any change of the schematic.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use tronic_pcb::{compare_with_netlist, Board, ImportedModel, Mechanics};
use tronic_schematic::{Connectivity, Netlist};

use crate::format::{decode, encode, Document};
use crate::ProjectError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Area {
    /// Name and description.
    Metadata,
    /// Electrical: parts, nets, values.
    Schematic,
    /// Placement, copper, design rules.
    Board,
    /// Mechanical: board thickness, height limits, imported 3D models.
    Mechanics,
}

impl Area {
    pub const ALL: [Self; 4] = [
        Self::Metadata,
        Self::Schematic,
        Self::Board,
        Self::Mechanics,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Schematic => "schematic",
            Self::Board => "board",
            Self::Mechanics => "mechanics",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|area| area.name() == text)
    }
}

type Models = BTreeMap<String, Option<Arc<ImportedModel>>>;

/// The board without what belongs to the mechanics area.
fn board_core(board: &Board) -> Board {
    let mut core = board.clone();
    core.mechanics = Mechanics::default();
    for definition in core.footprint_defs.values_mut() {
        if definition.model.is_some() {
            Arc::make_mut(definition).model = None;
        }
    }
    core
}

fn models_of(board: &Board) -> Models {
    board
        .footprint_defs
        .iter()
        .map(|(id, definition)| (id.clone(), definition.model.clone()))
        .collect()
}

fn same(area: Area, a: &Document, b: &Document) -> bool {
    match area {
        Area::Metadata => {
            a.project.name == b.project.name && a.project.description == b.project.description
        }
        Area::Schematic => a.schematic == b.schematic,
        Area::Board => board_core(&a.board) == board_core(&b.board),
        Area::Mechanics => {
            a.board.mechanics == b.board.mechanics && models_of(&a.board) == models_of(&b.board)
        }
    }
}

fn changed_between(a: &Document, b: &Document) -> BTreeSet<Area> {
    Area::ALL
        .into_iter()
        .filter(|area| !same(*area, a, b))
        .collect()
}

/// The areas that differ. A save without a change differs in nothing.
pub fn changed_areas(before: &[u8], after: &[u8]) -> Result<BTreeSet<Area>, ProjectError> {
    let path = Path::new("version");
    Ok(changed_between(
        &decode(before, path)?,
        &decode(after, path)?,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Merge {
    /// Together again. `deviations` counts the differences between schematic and
    /// board in the result, which a person should look at.
    Merged {
        bytes: Vec<u8>,
        from_ours: BTreeSet<Area>,
        from_theirs: BTreeSet<Area>,
        deviations: usize,
    },
    /// These areas changed on both sides, differently.
    Conflict { areas: BTreeSet<Area> },
}

fn deviations(document: &Document) -> usize {
    let netlist = Netlist::build(
        &document.schematic,
        &Connectivity::analyze(&document.schematic),
    );
    compare_with_netlist(&document.board, &netlist).len()
}

/// Brings two states with a common origin together, area by area.
pub fn merge(base: &[u8], ours: &[u8], theirs: &[u8]) -> Result<Merge, ProjectError> {
    let path = Path::new("version");
    let (base, ours, theirs) = (
        decode(base, path)?,
        decode(ours, path)?,
        decode(theirs, path)?,
    );
    let ours_changed = changed_between(&base, &ours);
    let theirs_changed = changed_between(&base, &theirs);
    let conflict: BTreeSet<Area> = ours_changed
        .intersection(&theirs_changed)
        .copied()
        .filter(|area| !same(*area, &ours, &theirs))
        .collect();
    if !conflict.is_empty() {
        return Ok(Merge::Conflict { areas: conflict });
    }
    let take_theirs = |area: Area| theirs_changed.contains(&area);

    let mut project = ours.project.clone();
    if take_theirs(Area::Metadata) {
        project.name.clone_from(&theirs.project.name);
        project.description.clone_from(&theirs.project.description);
    }
    project.modified_at = project.modified_at.max(theirs.project.modified_at);
    let schematic = if take_theirs(Area::Schematic) {
        theirs.schematic.clone()
    } else {
        ours.schematic.clone()
    };
    let mut board = if take_theirs(Area::Board) {
        theirs.board.clone()
    } else {
        ours.board.clone()
    };
    let mechanical = if take_theirs(Area::Mechanics) {
        &theirs.board
    } else {
        &ours.board
    };
    board.mechanics = mechanical.mechanics.clone();
    let models = models_of(mechanical);
    for (id, definition) in &mut board.footprint_defs {
        let model = models.get(id).cloned().flatten();
        if definition.model != model {
            Arc::make_mut(definition).model = model;
        }
    }
    let bytes = encode(&project, &schematic, &board, path)?;
    let document = Document {
        project,
        schematic,
        board,
    };
    Ok(Merge::Merged {
        deviations: deviations(&document),
        bytes,
        from_ours: ours_changed.difference(&theirs_changed).copied().collect(),
        from_theirs: theirs_changed.difference(&ours_changed).copied().collect(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use tronic_core::Project;
    use tronic_pcb::MM;
    use tronic_schematic::{Editor, Point, Rotation, Schematic, SymbolDef};

    fn document() -> Document {
        Document {
            project: Project::new("Name", "", time::OffsetDateTime::now_utc()).unwrap(),
            schematic: Schematic::default(),
            board: Board::default(),
        }
    }

    fn bytes(document: &Document) -> Vec<u8> {
        encode(
            &document.project,
            &document.schematic,
            &document.board,
            Path::new("x"),
        )
        .unwrap()
    }

    fn with_sheet_rename(document: &Document, name: &str) -> Document {
        let mut changed = document.clone();
        let mut editor = Editor::new(changed.schematic.clone());
        let sheet = editor.doc().sheets[0].id;
        editor
            .apply(tronic_schematic::EditCommand::RenameSheet {
                sheet,
                name: name.to_owned(),
            })
            .unwrap();
        changed.schematic = editor.doc().clone();
        changed
    }

    fn with_outline(document: &Document, size: i64) -> Document {
        let mut changed = document.clone();
        changed.board.outline = vec![
            Point::new(0, 0),
            Point::new(size * MM, 0),
            Point::new(size * MM, size * MM),
        ];
        changed
    }

    fn with_thickness(document: &Document, nm: i64) -> Document {
        let mut changed = document.clone();
        changed.board.mechanics.thickness = nm;
        changed
    }

    fn areas(names: &[Area]) -> BTreeSet<Area> {
        names.iter().copied().collect()
    }

    #[test]
    fn a_save_without_a_change_changes_nothing() {
        let base = document();
        let mut later = base.clone();
        later.project.modified_at += time::Duration::hours(1);
        assert_eq!(
            changed_areas(&bytes(&base), &bytes(&later)).unwrap(),
            areas(&[])
        );
    }

    #[test]
    fn each_change_lands_in_its_area() {
        let base = document();
        let before = bytes(&base);
        let schematic = with_sheet_rename(&base, "Other");
        assert_eq!(
            changed_areas(&before, &bytes(&schematic)).unwrap(),
            areas(&[Area::Schematic])
        );
        let board = with_outline(&base, 50);
        assert_eq!(
            changed_areas(&before, &bytes(&board)).unwrap(),
            areas(&[Area::Board])
        );
        let mechanics = with_thickness(&base, 1_000_000);
        assert_eq!(
            changed_areas(&before, &bytes(&mechanics)).unwrap(),
            areas(&[Area::Mechanics])
        );
        let mut metadata = base.clone();
        metadata.project.description = "new".to_owned();
        assert_eq!(
            changed_areas(&before, &bytes(&metadata)).unwrap(),
            areas(&[Area::Metadata])
        );
    }

    #[test]
    fn different_areas_come_together() {
        let base = document();
        let ours = with_sheet_rename(&base, "Ours");
        let theirs = with_outline(&base, 50);
        let Merge::Merged {
            bytes: merged,
            from_ours,
            from_theirs,
            ..
        } = merge(&bytes(&base), &bytes(&ours), &bytes(&theirs)).unwrap()
        else {
            panic!("expected a merge");
        };
        assert_eq!(from_ours, areas(&[Area::Schematic]));
        assert_eq!(from_theirs, areas(&[Area::Board]));
        let result = decode(&merged, Path::new("x")).unwrap();
        assert_eq!(result.schematic, ours.schematic);
        assert_eq!(result.board.outline, theirs.board.outline);
    }

    #[test]
    fn board_and_mechanics_of_one_board_come_together() {
        let base = document();
        let ours = with_outline(&base, 50);
        let theirs = with_thickness(&base, 800_000);
        let Merge::Merged { bytes: merged, .. } =
            merge(&bytes(&base), &bytes(&ours), &bytes(&theirs)).unwrap()
        else {
            panic!("expected a merge");
        };
        let result = decode(&merged, Path::new("x")).unwrap();
        assert_eq!(result.board.outline, ours.board.outline);
        assert_eq!(result.board.mechanics.thickness, 800_000);
    }

    #[test]
    fn the_same_area_changed_twice_is_for_a_person() {
        let base = document();
        let ours = with_outline(&base, 50);
        let theirs = with_outline(&base, 60);
        assert_eq!(
            merge(&bytes(&base), &bytes(&ours), &bytes(&theirs)).unwrap(),
            Merge::Conflict {
                areas: areas(&[Area::Board])
            }
        );
    }

    #[test]
    fn the_same_change_on_both_sides_is_no_conflict() {
        let base = document();
        let ours = with_outline(&base, 50);
        assert!(matches!(
            merge(&bytes(&base), &bytes(&ours), &bytes(&ours)).unwrap(),
            Merge::Merged { .. }
        ));
    }

    /// A part added to the schematic while the board is worked on: the merge succeeds
    /// and says that schematic and board now differ, instead of hiding it.
    #[test]
    fn differences_between_schematic_and_board_are_reported() {
        let base = document();
        let mut ours = base.clone();
        let mut editor = Editor::new(ours.schematic.clone());
        let resistor: SymbolDef = tronic_library::builtin()
            .into_iter()
            .find(|symbol| symbol.id == "device:resistor")
            .unwrap();
        editor.add_symbols([resistor]);
        let sheet = editor.doc().sheets[0].id;
        editor
            .apply(tronic_schematic::EditCommand::PlaceComponent {
                sheet,
                symbol: "device:resistor".to_owned(),
                at: Point::new(0, 0),
                rotation: Rotation::R0,
                mirror: false,
            })
            .unwrap();
        ours.schematic = editor.doc().clone();
        let theirs = with_outline(&base, 50);
        let Merge::Merged { deviations, .. } =
            merge(&bytes(&base), &bytes(&ours), &bytes(&theirs)).unwrap()
        else {
            panic!("expected a merge");
        };
        assert_eq!(deviations, 1);
    }

    #[test]
    fn area_names_read_back() {
        for area in Area::ALL {
            assert_eq!(Area::parse(area.name()), Some(area));
        }
        assert_eq!(Area::parse("erc"), None);
    }
}
