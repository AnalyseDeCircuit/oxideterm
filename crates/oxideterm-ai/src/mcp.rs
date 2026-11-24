include!("mcp/model.rs");
#[cfg(not(target_family = "wasm"))]
include!("mcp/types.rs");
#[cfg(not(target_family = "wasm"))]
include!("mcp/protocol.rs");
#[cfg(not(target_family = "wasm"))]
include!("mcp/registry_public.rs");
#[cfg(not(target_family = "wasm"))]
include!("mcp/registry_connect.rs");
#[cfg(not(target_family = "wasm"))]
include!("mcp/registry_protocol.rs");
#[cfg(not(target_family = "wasm"))]
include!("mcp/process.rs");
#[cfg(not(target_family = "wasm"))]
include!("mcp/helpers.rs");
#[cfg(not(target_family = "wasm"))]
include!("mcp/tests.rs");
