//! Authenticated MCP bridge for ACP agents to call OxideTerm-owned host tools.

mod protocol;
mod runtime;
mod types;

pub use runtime::{AcpHostToolsServer, start_acp_host_tools_server};
pub use types::{
    AcpHostToolCall, AcpHostToolCallReceiver, AcpHostToolDefinition, AcpHostToolResponse,
    AcpHostToolsError,
};

#[cfg(test)]
mod tests;
