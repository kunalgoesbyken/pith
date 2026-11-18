#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod launch;

#[cfg(windows)]
fn main() {
    launch::launch();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Pith currently runs on Windows; macOS and Linux support is planned.");
}
