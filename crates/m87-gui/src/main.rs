//! `m87-gui` — window version of the M87 Pro configuration tool.
//! Same pages as the terminal version, in English or Korean.

#![windows_subsystem = "windows"]

mod app;
#[cfg(debug_assertions)]
mod capture;
mod i18n;
mod kbview;
mod theme;
mod ui;

fn main() {
    if let Err(e) = ui::run() {
        ui::fatal(&format!("{e}"));
    }
}
