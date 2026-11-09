//! Change proposals from a model. A proposal is only data: it is read against a
//! closed list of change types, tried on a copy, and shown to the user together
//! with what the real ERC says before and after.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tronic_schematic::{EditCommand, Editor, ElementId, LabelKind, Point, Schematic, GRID};

use crate::context::{erc_counts, CheckCounts};

const BLOCK_MARK: &str = "```tronic-changes";
const PART_SPACING: i64 = 40;
const PARTS_PER_ROW: i64 = 6;
/// More would not be reviewable, and each change is reviewed by a person.
const MAX_CHANGES: usize = 500;
/// How much of an unreadable block the user gets to see.
const MAX_SHOWN_CHARS: usize = 600;

/// The complete list of what a model can suggest. There is deliberately no way
/// to express a check result here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    SetValue {
        reference: String,
        value: String,
    },
    SetReference {
        reference: String,
        new_reference: String,
    },
    Place {
        symbol: String,
        reference: String,
        #[serde(default)]
        value: Option<String>,
    },
    Connect {
        reference: String,
        pin: String,
        net: String,
    },
    Delete {
        reference: String,
    },
}

/// A change that was not accepted, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rejected {
    pub code: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preview {
    pub changes: Vec<Change>,
    pub rejected: Vec<Rejected>,
    pub erc_before: CheckCounts,
    pub erc_after: CheckCounts,
}

/// Splits an answer into its text and the content of its change block. Small
/// models write the block in more than one way. Being generous here is safe:
/// what is found goes through the same checks and the same approval.
pub fn extract_block(answer: &str) -> (String, Option<String>) {
    for (open, close) in [
        (BLOCK_MARK, "```"),
        ("<tronic-changes>", "</tronic-changes>"),
    ] {
        let Some(start) = answer.find(open) else {
            continue;
        };
        let rest = &answer[start + open.len()..];
        // An answer cut off by the length limit has no closing mark.
        let (inner, after) = match rest.find(close) {
            Some(end) => (&rest[..end], &rest[end + close.len()..]),
            None => (rest, ""),
        };
        let text = format!("{}{}", &answer[..start], after);
        return (text.trim().to_owned(), Some(inner.trim().to_owned()));
    }
    (answer.to_owned(), None)
}

/// The entries of a change block: an object with a list, a list, or one object per line.
fn entries_of(block: &str) -> Option<Vec<serde_json::Value>> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(block) {
        return match value {
            serde_json::Value::Array(list) => Some(list),
            serde_json::Value::Object(ref object) if object.contains_key("type") => {
                Some(vec![value])
            }
            serde_json::Value::Object(mut object) => match object.remove("changes") {
                Some(serde_json::Value::Array(list)) => Some(list),
                _ => None,
            },
            _ => None,
        };
    }
    block
        .lines()
        .map(|line| line.trim().trim_end_matches(','))
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Reads a change block. Each entry stands alone, so that one unknown change
/// does not hide the others.
pub fn parse_changes(block: &str) -> (Vec<Change>, Vec<Rejected>) {
    let reject = |code, detail: String| Rejected { code, detail };
    let Some(list) = entries_of(block) else {
        let shown: String = block.chars().take(MAX_SHOWN_CHARS).collect();
        return (Vec::new(), vec![reject("unreadable", shown)]);
    };
    if list.len() > MAX_CHANGES {
        return (Vec::new(), vec![reject("too_many", list.len().to_string())]);
    }
    let mut changes = Vec::new();
    let mut rejected = Vec::new();
    for entry in list {
        match serde_json::from_value::<Change>(entry.clone()) {
            Ok(change) => changes.push(change),
            Err(_) => rejected.push(reject("unknown_change", entry.to_string())),
        }
    }
    (changes, rejected)
}

fn find_component(doc: &Schematic, reference: &str) -> Option<(ElementId, ElementId)> {
    doc.sheets.iter().find_map(|sheet| {
        sheet
            .components
            .iter()
            .find(|component| component.reference == reference)
            .map(|component| (sheet.id, component.id))
    })
}

/// Where new parts go: in rows to the right of what is on the first sheet.
struct Placement {
    right: i64,
    count: i64,
}

impl Placement {
    fn new(doc: &Schematic) -> Self {
        let right = doc
            .sheets
            .first()
            .map(|sheet| sheet.components.iter().map(|c| c.at.x).max().unwrap_or(0))
            .unwrap_or(0);
        Self { right, count: 0 }
    }

    fn next(&mut self) -> Point {
        let column = self.count % PARTS_PER_ROW;
        let row = self.count / PARTS_PER_ROW;
        self.count += 1;
        Point::new(
            (self.right / GRID + PART_SPACING + column * PART_SPACING) * GRID,
            row * PART_SPACING * GRID,
        )
    }
}

fn run_change(
    editor: &mut Editor,
    change: &Change,
    placement: &mut Placement,
) -> Result<(), Rejected> {
    let fail = |code, detail: &str| Rejected {
        code,
        detail: detail.to_owned(),
    };
    let unknown = |reference: &str| fail("unknown_part", reference);
    let refused = |error: tronic_schematic::EditError| fail("refused", &error.to_string());
    match change {
        Change::SetValue { reference, value } => {
            let (sheet, id) =
                find_component(editor.doc(), reference).ok_or_else(|| unknown(reference))?;
            editor
                .apply(EditCommand::SetComponent {
                    sheet,
                    id,
                    reference: reference.clone(),
                    value: value.clone(),
                })
                .map_err(refused)?;
        }
        Change::SetReference {
            reference,
            new_reference,
        } => {
            let (sheet, id) =
                find_component(editor.doc(), reference).ok_or_else(|| unknown(reference))?;
            if find_component(editor.doc(), new_reference).is_some() {
                return Err(fail("duplicate_reference", new_reference));
            }
            let value = editor
                .doc()
                .sheet(sheet)
                .and_then(|s| s.components.iter().find(|c| c.id == id))
                .map(|c| c.value.clone())
                .unwrap_or_default();
            editor
                .apply(EditCommand::SetComponent {
                    sheet,
                    id,
                    reference: new_reference.clone(),
                    value,
                })
                .map_err(refused)?;
        }
        Change::Place {
            symbol,
            reference,
            value,
        } => {
            if find_component(editor.doc(), reference).is_some() {
                return Err(fail("duplicate_reference", reference));
            }
            if !editor.doc().symbols.contains_key(symbol) {
                let definition = tronic_library::builtin()
                    .into_iter()
                    .find(|candidate| &candidate.id == symbol)
                    .ok_or_else(|| fail("unknown_symbol", symbol))?;
                editor.add_symbols([definition]);
            }
            let sheet = editor
                .doc()
                .sheets
                .first()
                .map(|sheet| sheet.id)
                .ok_or_else(|| fail("refused", "no sheet"))?;
            let at = placement.next();
            let applied = editor
                .apply(EditCommand::PlaceComponent {
                    sheet,
                    symbol: symbol.clone(),
                    at,
                    rotation: Default::default(),
                    mirror: false,
                })
                .map_err(refused)?;
            let id = *applied
                .created
                .first()
                .ok_or_else(|| fail("refused", "nothing placed"))?;
            let default_value = editor
                .doc()
                .sheet(sheet)
                .and_then(|s| s.components.iter().find(|c| c.id == id))
                .map(|c| c.value.clone())
                .unwrap_or_default();
            editor
                .apply(EditCommand::SetComponent {
                    sheet,
                    id,
                    reference: reference.clone(),
                    value: value.clone().unwrap_or(default_value),
                })
                .map_err(refused)?;
        }
        Change::Connect {
            reference,
            pin,
            net,
        } => {
            let (sheet, id) =
                find_component(editor.doc(), reference).ok_or_else(|| unknown(reference))?;
            let doc = editor.doc();
            let component = doc
                .sheet(sheet)
                .and_then(|s| s.components.iter().find(|c| c.id == id))
                .ok_or_else(|| unknown(reference))?;
            let symbol = doc
                .symbols
                .get(&component.symbol)
                .ok_or_else(|| unknown(reference))?;
            let index = symbol
                .pins
                .iter()
                .position(|candidate| &candidate.number == pin)
                .ok_or_else(|| fail("unknown_pin", &format!("{reference}.{pin}")))?;
            let at = *doc
                .pin_points(component)
                .get(index)
                .ok_or_else(|| fail("unknown_pin", &format!("{reference}.{pin}")))?;
            editor
                .apply(EditCommand::AddLabel {
                    sheet,
                    at,
                    name: net.clone(),
                    kind: LabelKind::Global,
                })
                .map_err(refused)?;
        }
        Change::Delete { reference } => {
            let (sheet, id) =
                find_component(editor.doc(), reference).ok_or_else(|| unknown(reference))?;
            editor
                .apply(EditCommand::Delete {
                    sheet,
                    ids: vec![id],
                })
                .map_err(refused)?;
        }
    }
    Ok(())
}

/// Runs changes on a copy. Returns the result, what worked and what did not.
pub fn try_changes(doc: &Schematic, changes: &[Change]) -> (Schematic, Vec<Change>, Vec<Rejected>) {
    let mut editor = Editor::new(doc.clone());
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    let mut placement = Placement::new(doc);
    for change in changes {
        match run_change(&mut editor, change, &mut placement) {
            Ok(()) => accepted.push(change.clone()),
            Err(reason) => rejected.push(reason),
        }
    }
    (editor.doc().clone(), accepted, rejected)
}

/// What the user gets to see before deciding. Only changes that work can be chosen.
pub fn preview(doc: &Schematic, changes: Vec<Change>, mut rejected: Vec<Rejected>) -> Preview {
    let (after, accepted, failed) = try_changes(doc, &changes);
    rejected.extend(failed);
    Preview {
        changes: accepted,
        rejected,
        erc_before: erc_counts(doc),
        erc_after: erc_counts(&after),
    }
}

/// The document with the chosen changes, and those that no longer work because
/// a change they depend on was left out. Only this and an approval change the project.
pub fn apply_changes(doc: &Schematic, changes: &[Change]) -> (Schematic, Vec<Rejected>) {
    let (after, _, rejected) = try_changes(doc, changes);
    (after, rejected)
}

/// How many changes of each type a list holds.
pub fn counts_by_type(changes: &[Change]) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for change in changes {
        let name = match change {
            Change::SetValue { .. } => "set_value",
            Change::SetReference { .. } => "set_reference",
            Change::Place { .. } => "place",
            Change::Connect { .. } => "connect",
            Change::Delete { .. } => "delete",
        };
        *counts.entry(name).or_insert(0) += 1;
    }
    counts
}
