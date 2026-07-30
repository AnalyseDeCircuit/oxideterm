use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

/// A concrete operation granted by a live runtime owner.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeCapability {
    TerminalObserve,
    TerminalRunCommand,
    TerminalSendInput,
    LocalShellRunCommand,
    NodeInspect,
    SftpRead,
    SftpWrite,
    SftpStartTransfer,
    IdeRead,
    IdeWrite,
    SurfaceFocus,
}

impl RuntimeCapability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TerminalObserve => "terminal.observe",
            Self::TerminalRunCommand => "terminal.run_command",
            Self::TerminalSendInput => "terminal.send_input",
            Self::LocalShellRunCommand => "local_shell.run_command",
            Self::NodeInspect => "node.inspect",
            Self::SftpRead => "sftp.read",
            Self::SftpWrite => "sftp.write",
            Self::SftpStartTransfer => "sftp.start_transfer",
            Self::IdeRead => "ide.read",
            Self::IdeWrite => "ide.write",
            Self::SurfaceFocus => "surface.focus",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "terminal.observe" => Self::TerminalObserve,
            "terminal.run_command" => Self::TerminalRunCommand,
            "terminal.send_input" => Self::TerminalSendInput,
            "local_shell.run_command" => Self::LocalShellRunCommand,
            "node.inspect" => Self::NodeInspect,
            "sftp.read" => Self::SftpRead,
            "sftp.write" => Self::SftpWrite,
            "sftp.start_transfer" => Self::SftpStartTransfer,
            "ide.read" => Self::IdeRead,
            "ide.write" => Self::IdeWrite,
            "surface.focus" => Self::SurfaceFocus,
            _ => return None,
        })
    }
}

impl Serialize for RuntimeCapability {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RuntimeCapability {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).ok_or_else(|| D::Error::custom("invalid runtime capability"))
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeCapability;

    #[test]
    fn capability_serialization_uses_stable_protocol_values() {
        let encoded = serde_json::to_string(&RuntimeCapability::TerminalRunCommand)
            .expect("runtime capability serializes");

        assert_eq!(encoded, "\"terminal.run_command\"");
    }

    #[test]
    fn unknown_capability_is_rejected() {
        let decoded = serde_json::from_str::<RuntimeCapability>("\"terminal.execute\"");

        assert!(decoded.is_err());
    }
}
