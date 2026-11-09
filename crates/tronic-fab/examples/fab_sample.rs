//! Writes the sample board's manufacturing files, for `scripts/verify_gerber.py`.
//! Usage: `cargo run -p tronic-fab --example fab_sample -- <directory> [layers]`

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let directory = args.next().unwrap_or_else(|| ".".to_owned());
    let layers = args.next().and_then(|n| n.parse().ok()).unwrap_or(2);
    std::fs::create_dir_all(&directory)?;
    let board = tronic_fab::sample_board(layers);
    for file in tronic_fab::generate(&board, "sample") {
        std::fs::write(format!("{directory}/{}", file.name), file.content)?;
    }
    Ok(())
}
