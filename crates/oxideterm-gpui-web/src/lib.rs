//! Single-window browser host for the portable OxideTerm workspace.
#![cfg(target_family = "wasm")]

mod ai;
mod boot;
mod connections;
mod demo;
mod files;
mod host;
mod palette;
mod terminal;
mod tools;
mod transport;

#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    boot::run();
}
