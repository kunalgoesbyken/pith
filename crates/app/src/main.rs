#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
mod hooks;
#[cfg(windows)]
mod launch;
#[cfg(target_os = "linux")]
mod linux;
mod setup;

#[cfg(windows)]
fn main() {
    launch::launch();
}

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    linux::launch()
}

#[cfg(not(any(windows, target_os = "linux")))]
fn main() {
    eprintln!("Pith runs on Windows and Linux; macOS support is planned.");
}
