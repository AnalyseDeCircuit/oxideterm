//! Single-window browser host for the portable OxideTerm workspace.
#![cfg(target_family = "wasm")]

mod ai;
mod ble;
mod boot;
mod browser_file;
mod connections;
mod demo;
mod files;
mod host;
mod icon;
mod knowledge;
mod pages;
mod palette;
mod preferences;
mod terminal;
mod tools;
mod transport;
mod wallpaper;

#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    boot::run();
}
