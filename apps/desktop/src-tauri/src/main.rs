// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = troniccad_desktop::run() {
        eprintln!("TronicCAD could not start: {error}");
        std::process::exit(1);
    }
}
