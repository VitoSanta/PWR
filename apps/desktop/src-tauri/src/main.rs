// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = pwr_desktop_lib::engine_maintenance_cli() {
        std::process::exit(code);
    }
    pwr_desktop_lib::run();
}
