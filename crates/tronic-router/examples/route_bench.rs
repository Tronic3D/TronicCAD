//! Measures the router on the reference boards. Run in release mode:
//! `cargo run --release -p tronic-router --example route_bench`

#[path = "../tests/support/mod.rs"]
mod support;

use std::sync::atomic::AtomicBool;
use std::time::Duration;
use support::*;
use tronic_pcb::Board;
use tronic_router::{route_board, RouteOptions};

static NEVER: AtomicBool = AtomicBool::new(false);

fn run(name: &str, board: &Board) {
    let result = route_board(
        board,
        &RouteOptions {
            time_limit: Duration::from_secs(120),
            cancel: &NEVER,
            progress: &|_, _| {},
        },
    );
    match result {
        Ok(result) => {
            let done = result.total - result.remaining.len().min(result.total);
            println!(
                "| {name} | {} | {done} | {:.1} % | {} | {} | {:.1} s |",
                result.total,
                100.0 * done as f64 / result.total.max(1) as f64,
                result.vias.len(),
                result.rip_ups,
                result.elapsed_ms as f64 / 1000.0,
            );
        }
        Err(error) => println!("| {name} | {error} | | | | | |"),
    }
}

/// A connector row against a row of chips, all pins crossing over: a classic
/// worst case for two layers.
fn crossing(pins: i64, width: i64, height: i64, layers: u8) -> Board {
    let chips = pins / 8;
    let mut parts = Vec::new();
    for i in 0..chips {
        parts.push(part(
            &format!("A{}", i + 1),
            "soic:8",
            width * (2 * i + 1) / (2 * chips),
            8,
        ));
        parts.push(part(
            &format!("B{}", i + 1),
            "soic:8",
            width * (2 * i + 1) / (2 * chips),
            height - 8,
        ));
    }
    let mut nets = Vec::new();
    for pin in 0..pins {
        let (chip, number) = (pin / 8 + 1, pin % 8 + 1);
        let reversed = pins - 1 - pin;
        nets.push(net(
            &format!("X{pin}"),
            &[
                (&format!("A{chip}"), &number.to_string()),
                (
                    &format!("B{}", reversed / 8 + 1),
                    &(reversed % 8 + 1).to_string(),
                ),
            ],
        ));
    }
    board(width, height, layers, &parts, &nets)
}

fn main() {
    println!("| Platine | Verbindungen | Verlegt | Anteil | Vias | Umlegungen | Zeit |");
    println!("|---|---|---|---|---|---|---|");
    run("LED, 2 Lagen", &led_board());
    run("555-Blinker, 2 Lagen", &flasher_board());
    run("Bus mit 8 ICs, 2 Lagen", &bus_board());
    run(
        "Zufall 9 ICs, 60 x 60 mm, 2 Lagen",
        &random_board(7, 9, 60, 60, 2),
    );
    run(
        "Zufall 16 ICs, 80 x 80 mm, 2 Lagen",
        &random_board(11, 16, 80, 80, 2),
    );
    run(
        "Zufall 16 ICs, 80 x 80 mm, 4 Lagen",
        &random_board(11, 16, 80, 80, 4),
    );
    run(
        "Zufall 25 ICs, 90 x 90 mm, 2 Lagen",
        &random_board(5, 25, 90, 90, 2),
    );
    run(
        "Zufall 25 ICs, 90 x 90 mm, 4 Lagen",
        &random_board(5, 25, 90, 90, 4),
    );
    run(
        "Zufall 36 ICs, 100 x 100 mm, 4 Lagen",
        &random_board(9, 36, 100, 100, 4),
    );
    run(
        "Eng: 16 ICs, 45 x 45 mm, 2 Lagen",
        &random_board(13, 16, 45, 45, 2),
    );
    run(
        "Eng: 16 ICs, 45 x 45 mm, 4 Lagen",
        &random_board(13, 16, 45, 45, 4),
    );
    run(
        "Eng: 25 ICs, 55 x 55 mm, 2 Lagen",
        &random_board(17, 25, 55, 55, 2),
    );
    run(
        "Eng: 25 ICs, 55 x 55 mm, 4 Lagen",
        &random_board(17, 25, 55, 55, 4),
    );
    run(
        "Kreuzung 48 Leitungen, 80 x 40 mm, 2 Lagen",
        &crossing(48, 80, 40, 2),
    );
    run(
        "Kreuzung 48 Leitungen, 80 x 40 mm, 4 Lagen",
        &crossing(48, 80, 40, 4),
    );
}
