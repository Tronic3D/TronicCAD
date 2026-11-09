//! The netlist handed to PCB, simulation and export.

use serde::Serialize;
use std::fmt::Write;

use crate::connectivity::Connectivity;
use crate::model::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetlistComponent {
    pub reference: String,
    pub value: String,
    pub symbol: String,
    pub sheet: String,
    pub footprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetlistNode {
    pub reference: String,
    pub pin: String,
    pub pin_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetlistNet {
    pub name: String,
    pub nodes: Vec<NetlistNode>,
}

/// Components and nets. Supply symbols only name nets and do not appear.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Netlist {
    pub components: Vec<NetlistComponent>,
    pub nets: Vec<NetlistNet>,
}

impl Netlist {
    pub fn build(doc: &Schematic, connectivity: &Connectivity) -> Self {
        let mut components = Vec::new();
        for sheet in &doc.sheets {
            for component in &sheet.components {
                let symbol = doc.symbols.get(&component.symbol);
                let is_supply = symbol.is_some_and(|symbol| symbol.kind == SymbolKind::SupplyPort);
                if !is_supply {
                    components.push(NetlistComponent {
                        reference: component.reference.clone(),
                        value: component.value.clone(),
                        symbol: component.symbol.clone(),
                        sheet: sheet.name.clone(),
                        footprint: component
                            .footprint
                            .clone()
                            .or_else(|| symbol.and_then(|s| s.part.footprint.clone())),
                    });
                }
            }
        }
        components.sort_by(|a, b| a.reference.cmp(&b.reference));

        let nets = connectivity
            .nets
            .iter()
            .filter_map(|net| {
                let nodes: Vec<NetlistNode> = net
                    .pins
                    .iter()
                    .filter(|pin| pin.kind != PinKind::SupplyPort)
                    .map(|pin| NetlistNode {
                        reference: pin.reference.clone(),
                        pin: pin.number.clone(),
                        pin_name: pin.name.clone(),
                    })
                    .collect();
                (!nodes.is_empty()).then(|| NetlistNet {
                    name: net.name.clone(),
                    nodes,
                })
            })
            .collect();
        Self { components, nets }
    }

    /// A readable text form, stable across runs.
    pub fn to_text(&self) -> String {
        let mut out = String::from("# TronicCAD netlist\n\n[components]\n");
        for component in &self.components {
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}",
                component.reference, component.value, component.symbol, component.sheet
            );
        }
        out.push_str("\n[nets]\n");
        for net in &self.nets {
            let nodes: Vec<String> = net
                .nodes
                .iter()
                .map(|node| format!("{}.{}", node.reference, node.pin))
                .collect();
            let _ = writeln!(out, "{}\t{}", net.name, nodes.join(" "));
        }
        out
    }
}
