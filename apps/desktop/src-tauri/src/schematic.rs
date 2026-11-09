//! Tauri commands of the schematic editor and the library.

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tronic_erc::Violation;
use tronic_library::{builtin, decode_library, encode_library, rectangular_part, CustomPartSpec};
use tronic_project::{store, ProjectSession};
use tronic_schematic::connectivity::{junction_points, Connectivity};
use tronic_schematic::{
    validate_symbol, EditCommand, ElementId, LabelKind, Netlist, Point, Sheet, SymbolDef,
};

use crate::error::CommandError;
use crate::AppState;

#[derive(Serialize)]
pub struct SheetEntry {
    id: ElementId,
    name: String,
    parent: Option<ElementId>,
    depth: usize,
}

#[derive(Serialize)]
pub struct CrossReference {
    label: ElementId,
    sheets: Vec<String>,
}

/// Everything the editor draws, as of the current document state.
#[derive(Serialize)]
pub struct SchematicView {
    sheets: Vec<SheetEntry>,
    sheet: Arc<Sheet>,
    junctions: Vec<Point>,
    cross_references: Vec<CrossReference>,
    symbols: BTreeMap<String, Arc<SymbolDef>>,
    can_undo: bool,
    can_redo: bool,
    dirty: bool,
}

#[derive(Serialize)]
pub struct ApplyResult {
    view: SchematicView,
    created: Vec<ElementId>,
}

fn view(session: &ProjectSession, sheet: ElementId) -> Result<SchematicView, CommandError> {
    let doc = session.schematic();
    // A deleted sheet falls back to the first one.
    let current = doc
        .sheet(sheet)
        .or_else(|| doc.sheets.first())
        .ok_or_else(|| CommandError::new("schematic.unknownSheet"))?
        .clone();
    let has_global = current
        .labels
        .iter()
        .any(|label| label.kind == LabelKind::Global);
    let cross_references = if has_global {
        let connectivity = Connectivity::analyze(doc);
        current
            .labels
            .iter()
            .filter(|label| label.kind == LabelKind::Global)
            .filter_map(|label| {
                let net = connectivity.net_of(current.id, label.id)?;
                let sheets: Vec<String> = net
                    .sheets
                    .iter()
                    .filter(|id| **id != current.id)
                    .filter_map(|id| doc.sheet(*id).map(|s| s.name.clone()))
                    .collect();
                Some(CrossReference {
                    label: label.id,
                    sheets,
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(SchematicView {
        sheets: doc
            .sheets_in_tree_order()
            .into_iter()
            .map(|(depth, entry)| SheetEntry {
                id: entry.id,
                name: entry.name.clone(),
                parent: entry.parent,
                depth,
            })
            .collect(),
        junctions: junction_points(doc, &current),
        sheet: current,
        cross_references,
        symbols: doc.symbols.clone(),
        can_undo: session.can_undo(),
        can_redo: session.can_redo(),
        dirty: session.is_dirty(),
    })
}

#[tauri::command(async)]
pub fn schematic_view(
    state: State<'_, AppState>,
    sheet: ElementId,
) -> Result<SchematicView, CommandError> {
    state.with_session(|session| view(session, sheet))
}

#[tauri::command(async)]
pub fn schematic_apply(
    state: State<'_, AppState>,
    view_sheet: ElementId,
    command: EditCommand,
) -> Result<ApplyResult, CommandError> {
    state.with_session(|session| {
        // Library symbols enter the project when first used.
        if let EditCommand::PlaceComponent { symbol, .. } = &command {
            if !session.schematic().symbols.contains_key(symbol) {
                if let Some(definition) = builtin().into_iter().find(|s| s.id == *symbol) {
                    session.add_symbols([definition]);
                }
            }
        }
        let applied = session.apply(command)?;
        Ok(ApplyResult {
            view: view(session, view_sheet)?,
            created: applied.created,
        })
    })
}

#[tauri::command(async)]
pub fn schematic_undo(
    state: State<'_, AppState>,
    sheet: ElementId,
) -> Result<SchematicView, CommandError> {
    state.with_session(|session| {
        session.undo()?;
        view(session, sheet)
    })
}

#[tauri::command(async)]
pub fn schematic_redo(
    state: State<'_, AppState>,
    sheet: ElementId,
) -> Result<SchematicView, CommandError> {
    state.with_session(|session| {
        session.redo()?;
        view(session, sheet)
    })
}

#[tauri::command(async)]
pub fn schematic_erc(state: State<'_, AppState>) -> Result<Vec<Violation>, CommandError> {
    state.with_session(|session| Ok(tronic_erc::check(session.schematic())))
}

#[derive(Serialize)]
pub struct NetlistView {
    netlist: Netlist,
    text: String,
}

fn netlist(session: &ProjectSession) -> Netlist {
    let doc = session.schematic();
    Netlist::build(doc, &Connectivity::analyze(doc))
}

#[tauri::command(async)]
pub fn schematic_netlist(state: State<'_, AppState>) -> Result<NetlistView, CommandError> {
    state.with_session(|session| {
        let netlist = netlist(session);
        let text = netlist.to_text();
        Ok(NetlistView { netlist, text })
    })
}

#[tauri::command(async)]
pub fn schematic_export_netlist(
    state: State<'_, AppState>,
    path: PathBuf,
) -> Result<(), CommandError> {
    state.with_session(|session| {
        let text = netlist(session).to_text();
        store::write_atomic(&path, text.as_bytes())?;
        Ok(())
    })
}

/// A net as drawn on one sheet, for highlighting.
#[derive(Serialize)]
pub struct NetHighlight {
    name: String,
    elements: Vec<ElementId>,
    pins: Vec<Point>,
}

#[tauri::command(async)]
pub fn schematic_net_of(
    state: State<'_, AppState>,
    sheet: ElementId,
    element: ElementId,
) -> Result<Option<NetHighlight>, CommandError> {
    state.with_session(|session| {
        let connectivity = Connectivity::analyze(session.schematic());
        Ok(connectivity.net_of(sheet, element).map(|net| {
            let mut elements: Vec<ElementId> = Vec::new();
            for item in net.wires.iter().chain(&net.labels).chain(&net.no_connects) {
                if item.sheet == sheet {
                    elements.push(item.id);
                }
            }
            NetHighlight {
                name: net.name.clone(),
                elements,
                pins: net
                    .pins
                    .iter()
                    .filter(|pin| pin.sheet == sheet)
                    .map(|pin| pin.at)
                    .collect(),
            }
        }))
    })
}

#[tauri::command(async)]
pub fn library_list() -> Vec<SymbolDef> {
    builtin()
}

#[tauri::command(async)]
pub fn library_create_part(
    state: State<'_, AppState>,
    sheet: ElementId,
    spec: CustomPartSpec,
) -> Result<SchematicView, CommandError> {
    if spec.name.trim().is_empty() || spec.reference_prefix.trim().is_empty() {
        return Err(CommandError::new("library.partIncomplete"));
    }
    let mut spec = spec;
    spec.id = format!("project:{}", spec.name.trim());
    spec.category = "custom".to_owned();
    let mut part = rectangular_part(&spec);
    part.provenance.source = "User project".to_owned();
    part.provenance.author = "User".to_owned();
    part.provenance.license = "Unknown, set by the user".to_owned();
    validate_symbol(&part).map_err(|detail| CommandError::invalid_library(&detail))?;
    state.with_session(|session| {
        session.add_symbols([part]);
        view(session, sheet)
    })
}

#[tauri::command(async)]
pub fn library_import(
    state: State<'_, AppState>,
    sheet: ElementId,
    path: PathBuf,
) -> Result<SchematicView, CommandError> {
    let bytes = store::read_file(&path)?;
    let symbols = decode_library(&bytes)?;
    state.with_session(|session| {
        session.add_symbols(symbols);
        view(session, sheet)
    })
}

/// Writes all symbols of this project that are not part of the built-in
/// library, so that they can be used in other projects.
#[tauri::command(async)]
pub fn library_export(state: State<'_, AppState>, path: PathBuf) -> Result<usize, CommandError> {
    let known: Vec<String> = builtin().into_iter().map(|symbol| symbol.id).collect();
    state.with_session(|session| {
        let own: Vec<SymbolDef> = session
            .schematic()
            .symbols
            .values()
            .filter(|symbol| !known.contains(&symbol.id))
            .map(|symbol| (**symbol).clone())
            .collect();
        let bytes = encode_library(&own)?;
        store::write_atomic(&path, &bytes)?;
        Ok(own.len())
    })
}
