#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
#[path = "../desktop.rs"]
mod desktop;

fn main() {
    #[cfg(windows)]
    desktop::run();
    #[cfg(not(windows))]
    eprintln!("Monochrome Capture desktop runs on Windows.");
}
