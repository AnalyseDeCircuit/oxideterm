#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForwardType {
    Local,
    Remote,
    Dynamic,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ForwardStatus {
    Starting,
    Active,
    Stopped,
    Error,
    Suspended,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForwardRule {
    pub id: String,
    pub forward_type: ForwardType,
    pub bind_address: String,
    pub bind_port: u16,
    pub target_host: String,
    pub target_port: u16,
    pub status: ForwardStatus,
    pub description: String,
}

impl ForwardRule {
    fn new(
        kind: ForwardType,
        bind: String,
        bind_port: u16,
        target: String,
        target_port: u16,
    ) -> Self {
        fn host(value: &str) -> String {
            match value.trim() {
                "" => "localhost".into(),
                value => value.into(),
            }
        }
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            forward_type: kind,
            bind_address: host(&bind),
            bind_port,
            target_host: if kind == ForwardType::Dynamic {
                String::new()
            } else {
                host(&target)
            },
            target_port,
            status: ForwardStatus::Starting,
            description: if kind == ForwardType::Dynamic {
                "SOCKS5 Proxy".into()
            } else {
                String::new()
            },
        }
    }

    pub fn local(
        bind: impl Into<String>,
        bind_port: u16,
        target: impl Into<String>,
        target_port: u16,
    ) -> Self {
        Self::new(
            ForwardType::Local,
            bind.into(),
            bind_port,
            target.into(),
            target_port,
        )
    }

    pub fn remote(
        bind: impl Into<String>,
        bind_port: u16,
        target: impl Into<String>,
        target_port: u16,
    ) -> Self {
        Self::new(
            ForwardType::Remote,
            bind.into(),
            bind_port,
            target.into(),
            target_port,
        )
    }

    pub fn dynamic(bind: impl Into<String>, port: u16) -> Self {
        Self::new(ForwardType::Dynamic, bind.into(), port, String::new(), 0)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ForwardUpdate {
    pub forward_type: Option<ForwardType>,
    pub bind_address: Option<String>,
    pub bind_port: Option<u16>,
    pub target_host: Option<String>,
    pub target_port: Option<u16>,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ForwardStats {
    pub connection_count: u64,
    pub active_connections: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetectedPort {
    pub port: u16,
    pub bind_addr: String,
    pub process_name: Option<String>,
    pub pid: Option<u32>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PortDetectionSnapshot {
    pub new_ports: Vec<DetectedPort>,
    pub closed_ports: Vec<DetectedPort>,
    pub all_ports: Vec<DetectedPort>,
    pub has_scanned: bool,
}
