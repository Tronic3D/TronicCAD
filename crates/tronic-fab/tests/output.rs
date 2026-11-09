//! Structure of the generated files. Independent parsing happens in
//! `scripts/verify_gerber.py`, on files written by the example `fab_sample`.
#![allow(clippy::unwrap_used, clippy::panic)]

use tronic_fab::{generate, sample_board, FabFile};
use tronic_pcb::*;

fn file<'a>(files: &'a [FabFile], suffix: &str) -> &'a str {
    &files
        .iter()
        .find(|f| f.name.ends_with(suffix))
        .unwrap_or_else(|| panic!("no file {suffix}"))
        .content
}

#[test]
fn every_copper_layer_gets_a_file_for_each_supported_count() {
    for layers in ALLOWED_LAYER_COUNTS {
        let board = sample_board(layers);
        let files = generate(&board, "Test");
        let copper = files.iter().filter(|f| f.name.contains("copper-")).count();
        assert_eq!(copper, layers as usize);
        assert!(file(&files, "copper-l1-top.gbr").contains("%TF.FileFunction,Copper,L1,Top*%"));
        if layers > 1 {
            let bottom = format!("copper-l{layers}-bottom.gbr");
            let function = format!("%TF.FileFunction,Copper,L{layers},Bot*%");
            assert!(file(&files, &bottom).contains(&function));
        }
        if layers > 2 {
            assert!(file(&files, "copper-l2-inner.gbr").contains("Copper,L2,Inr"));
        }
    }
}

#[test]
fn gerber_files_are_complete_and_well_formed() {
    let files = generate(&sample_board(2), "Test");
    for f in files.iter().filter(|f| f.name.ends_with(".gbr")) {
        assert!(f.content.contains("%FSLAX46Y46*%"), "{}", f.name);
        assert!(f.content.contains("%MOMM*%"), "{}", f.name);
        assert!(f.content.trim_end().ends_with("M02*"), "{}", f.name);
        for line in f.content.lines() {
            assert!(
                line.ends_with('*') || line.ends_with("*%"),
                "{}: {line}",
                f.name
            );
        }
    }
}

#[test]
fn the_y_axis_points_up_in_gerber() {
    let mut board = PcbEditor::default();
    board
        .apply(PcbCommand::AddTracks {
            net: "A".to_owned(),
            layer: 0,
            points: vec![
                tronic_schematic::Point::new(MM, 2 * MM),
                tronic_schematic::Point::new(3 * MM, 2 * MM),
            ],
            width: 250_000,
        })
        .unwrap();
    let files = generate(board.board(), "T");
    let copper = file(&files, "copper-l1-top.gbr");
    assert!(copper.contains("X1000000Y-2000000D02*"), "{copper}");
    assert!(copper.contains("X3000000Y-2000000D01*"), "{copper}");
}

#[test]
fn vias_appear_on_all_layers_and_in_the_plated_drill_file() {
    let board = sample_board(4);
    let files = generate(&board, "T");
    let drill = file(&files, "plated.drl");
    assert!(drill.starts_with("M48\n"));
    assert!(drill.contains("METRIC"));
    assert!(drill.trim_end().ends_with("M30"));
    assert!(drill.contains("T1C0.300"));
    for name in [
        "copper-l1-top.gbr",
        "copper-l2-inner.gbr",
        "copper-l3-inner.gbr",
        "copper-l4-bottom.gbr",
    ] {
        assert!(file(&files, name).contains("D03*"), "{name}");
    }
}

#[test]
fn mounting_holes_go_to_the_non_plated_drill_file_and_no_copper_layer() {
    let board = sample_board(2);
    let files = generate(&board, "T");
    assert!(file(&files, "nonplated.drl").contains("T1C3.200"));
    assert!(!file(&files, "plated.drl").contains("C3.200"));
    assert!(!file(&files, "copper-l1-top.gbr").contains("C,3.200000"));
}

#[test]
fn zones_are_written_as_regions() {
    let files = generate(&sample_board(2), "T");
    let bottom = file(&files, "copper-l2-bottom.gbr");
    assert!(bottom.contains("G36*"));
    assert!(bottom.contains("G37*"));
    assert!(bottom.contains("%LPC*%"), "the zone has cut-outs");
}

#[test]
fn solder_mask_openings_are_larger_than_the_pads_and_negative() {
    let files = generate(&sample_board(2), "T");
    let mask = file(&files, "mask-top.gbr");
    assert!(mask.contains("%TF.FilePolarity,Negative*%"));
    // 0805 pads are 1.0 x 1.3 mm, plus 0.05 mm margin per side.
    assert!(mask.contains("R,1.100000X1.400000*%"), "{mask}");
}

#[test]
fn paste_has_surface_pads_only() {
    let files = generate(&sample_board(2), "T");
    let paste = file(&files, "paste-top.gbr");
    assert!(paste.contains("R,1.000000X1.300000*%"));
    assert!(!paste.contains("C,1.700000"), "no through-hole pads");
}

#[test]
fn the_outline_is_closed() {
    let files = generate(&sample_board(2), "T");
    let outline = file(&files, "outline.gbr");
    assert!(outline.contains("Profile,NP"));
    let d01: Vec<&str> = outline.lines().filter(|l| l.ends_with("D01*")).collect();
    let d02: Vec<&str> = outline.lines().filter(|l| l.ends_with("D02*")).collect();
    assert_eq!(d01.len(), 4);
    assert_eq!(d02.len(), 1);
    assert_eq!(
        d01.last().unwrap().trim_end_matches("D01*"),
        d02[0].trim_end_matches("D02*")
    );
}

#[test]
fn silk_has_references_and_bottom_parts_go_to_the_bottom_side() {
    let board = sample_board(2);
    let files = generate(&board, "T");
    let top = file(&files, "silk-top.gbr");
    assert!(top.contains("D01*"));
    let bottom = file(&files, "silk-bottom.gbr");
    assert!(
        bottom.contains("D01*"),
        "the sample has a part on the bottom"
    );
    let paste_bottom = file(&files, "paste-bottom.gbr");
    assert!(paste_bottom.contains("D03*"));
}

#[test]
fn lists_contain_every_part() {
    let board = sample_board(2);
    let files = generate(&board, "T");
    let placement = file(&files, "placement.csv");
    for part in board.footprints.iter() {
        assert!(
            placement.contains(&format!("{},", part.reference)),
            "{}",
            part.reference
        );
    }
    assert!(placement.contains(",bottom"));
    let bom = file(&files, "bom.csv");
    let total: usize = bom
        .lines()
        .skip(1)
        .map(|line| line.split(',').next().unwrap().parse::<usize>().unwrap())
        .sum();
    assert_eq!(total, board.footprints.len());
}

#[test]
fn file_names_survive_any_project_name() {
    let files = generate(&sample_board(1), "Größe / 电路: *");
    for f in &files {
        assert!(!f.name.contains(['/', ':', '*', ' ']), "{}", f.name);
    }
}

#[test]
fn output_is_deterministic() {
    assert_eq!(
        generate(&sample_board(2), "T"),
        generate(&sample_board(2), "T")
    );
}
