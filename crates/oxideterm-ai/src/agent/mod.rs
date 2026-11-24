//! Conversation-owned agent runs. UI projections never own execution or authority.

mod checkpoint;
#[cfg(not(target_family = "wasm"))]
mod execution;
mod model;
mod operation;
mod owned;
#[cfg(not(target_family = "wasm"))]
mod process;
mod progress;
#[cfg(not(target_family = "wasm"))]
mod request;
#[cfg(not(target_family = "wasm"))]
mod resources;
#[cfg(not(target_family = "wasm"))]
mod runtime;
#[cfg(not(target_family = "wasm"))]
mod scheduler;
mod tools;

pub use checkpoint::*;
#[cfg(not(target_family = "wasm"))]
pub use execution::AgentExecution;
pub use model::*;
pub use operation::*;
pub use owned::*;
#[cfg(not(target_family = "wasm"))]
pub use process::AgentProcess;
pub use progress::*;
#[cfg(not(target_family = "wasm"))]
pub use request::AgentModelRequest;
#[cfg(not(target_family = "wasm"))]
pub use resources::*;
#[cfg(not(target_family = "wasm"))]
pub use runtime::*;
#[cfg(not(target_family = "wasm"))]
pub use scheduler::{AgentPermit, DEFAULT_AGENT_CONCURRENCY, MAX_AGENT_CONCURRENCY};
pub use tools::{CHILD_AGENT_TOOLS, PARENT_AGENT_TOOLS, agent_tool_definitions, is_agent_tool};

#[cfg(test)]
mod tests;
