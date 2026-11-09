//! Compares two saved states of a project, for looking at versions.

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use tronic_schematic::{Connectivity, Netlist};

use crate::format::{decode, Document};
use crate::ProjectError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValueChange {
    pub reference: String,
    pub before: String,
    pub after: String,
}

/// What differs, from `before` to `after`. Geometry of the schematic that does not
/// change the circuit is not listed.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Comparison {
    pub parts_added: Vec<String>,
    pub parts_removed: Vec<String>,
    pub values_changed: Vec<ValueChange>,
    pub nets_added: Vec<String>,
    pub nets_removed: Vec<String>,
    pub nets_changed: Vec<String>,
    pub footprints: (usize, usize),
    pub tracks: (usize, usize),
    pub vias: (usize, usize),
    pub board_changed: bool,
    pub identical: bool,
}

fn netlist(document: &Document) -> Netlist {
    Netlist::build(
        &document.schematic,
        &Connectivity::analyze(&document.schematic),
    )
}

pub fn compare(before: &[u8], after: &[u8]) -> Result<Comparison, ProjectError> {
    let path = Path::new("version");
    let (old, new) = (decode(before, path)?, decode(after, path)?);
    let (old_list, new_list) = (netlist(&old), netlist(&new));
    let values = |list: &Netlist| -> BTreeMap<String, String> {
        list.components
            .iter()
            .map(|part| (part.reference.clone(), part.value.clone()))
            .collect()
    };
    let (old_parts, new_parts) = (values(&old_list), values(&new_list));
    let mut result = Comparison {
        parts_added: new_parts
            .keys()
            .filter(|reference| !old_parts.contains_key(*reference))
            .cloned()
            .collect(),
        parts_removed: old_parts
            .keys()
            .filter(|reference| !new_parts.contains_key(*reference))
            .cloned()
            .collect(),
        values_changed: new_parts
            .iter()
            .filter_map(|(reference, value)| {
                let before = old_parts.get(reference)?;
                (before != value).then(|| ValueChange {
                    reference: reference.clone(),
                    before: before.clone(),
                    after: value.clone(),
                })
            })
            .collect(),
        footprints: (old.board.footprints.len(), new.board.footprints.len()),
        tracks: (old.board.tracks.len(), new.board.tracks.len()),
        vias: (old.board.vias.len(), new.board.vias.len()),
        board_changed: old.board != new.board,
        ..Comparison::default()
    };
    let nets = |list: &Netlist| -> BTreeMap<String, Vec<String>> {
        list.nets
            .iter()
            .map(|net| {
                let mut nodes: Vec<String> = net
                    .nodes
                    .iter()
                    .map(|node| format!("{}.{}", node.reference, node.pin))
                    .collect();
                nodes.sort();
                (net.name.clone(), nodes)
            })
            .collect()
    };
    let (old_nets, new_nets) = (nets(&old_list), nets(&new_list));
    for (name, nodes) in &new_nets {
        match old_nets.get(name) {
            None => result.nets_added.push(name.clone()),
            Some(before) if before != nodes => result.nets_changed.push(name.clone()),
            Some(_) => {}
        }
    }
    result.nets_removed = old_nets
        .keys()
        .filter(|name| !new_nets.contains_key(*name))
        .cloned()
        .collect();
    result.identical = result.parts_added.is_empty()
        && result.parts_removed.is_empty()
        && result.values_changed.is_empty()
        && result.nets_added.is_empty()
        && result.nets_removed.is_empty()
        && result.nets_changed.is_empty()
        && !result.board_changed
        && old.schematic == new.schematic;
    Ok(result)
}
