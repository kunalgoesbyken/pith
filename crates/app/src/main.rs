#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
mod hooks;
mod platform;
mod setup;

#[cfg(windows)]
fn main() {
    platform::launch();
}

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    platform::launch()
}

#[cfg(not(any(windows, target_os = "linux")))]
fn main() {
    eprintln!("Pith runs on Windows and Linux; macOS support is planned.");
}
