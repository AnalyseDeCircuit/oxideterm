// The SSH crate owns the pure connection-plan state machine. GPUI imports the
// domain types here while retaining dialogs, channels, and connection effects.
pub(in crate::workspace) use oxideterm_ssh::{
    NativeSessionTreeConnectAction, NativeSessionTreeConnectEndpoint, NativeSessionTreeConnectPlan,
    NativeSessionTreeConnectStep,
};
