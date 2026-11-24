//! AI workspace state, views, and task delivery.
#[cfg(not(target_family = "wasm"))]
pub mod acp;
pub mod background;
pub mod bridge;
pub mod conversation;
pub mod inline;
pub mod knowledge;
pub mod model_picker;
pub mod sidebar;
pub mod state;
pub mod stream;
pub mod transcript;
