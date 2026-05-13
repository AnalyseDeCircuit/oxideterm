use std::collections::VecDeque;
use std::time::SystemTime;

pub const DEFAULT_EVENT_LOG_CAPACITY: usize = 500;
pub const DEFAULT_NOTIFICATION_CAPACITY: usize = 200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityView {
    Notifications,
    EventLog,
}

impl Default for ActivityView {
    fn default() -> Self {
        Self::Notifications
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventSeverity {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventCategory {
    Connection,
    Reconnect,
    Node,
}

#[derive(Clone, Debug)]
pub struct EventLogEntry {
    pub id: u64,
    pub timestamp: SystemTime,
    pub severity: EventSeverity,
    pub category: EventCategory,
    pub node_id: Option<String>,
    pub connection_id: Option<String>,
    pub title: String,
    pub detail: Option<String>,
    pub source: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventSeverityFilter {
    All,
    Error,
    Warn,
    Info,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventCategoryFilter {
    All,
    Connection,
    Reconnect,
    Node,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventFilter {
    pub severity: EventSeverityFilter,
    pub category: EventCategoryFilter,
}

impl Default for EventFilter {
    fn default() -> Self {
        Self {
            severity: EventSeverityFilter::All,
            category: EventCategoryFilter::All,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EventLogState {
    pub entries: VecDeque<EventLogEntry>,
    pub next_id: u64,
    pub unread_count: u32,
    pub unread_errors: u32,
    pub filter: EventFilter,
    pub dnd_enabled: bool,
    capacity: usize,
}

impl Default for EventLogState {
    fn default() -> Self {
        Self::new(DEFAULT_EVENT_LOG_CAPACITY)
    }
}

impl EventLogState {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            next_id: 1,
            unread_count: 0,
            unread_errors: 0,
            filter: EventFilter::default(),
            dnd_enabled: true,
            capacity,
        }
    }

    pub fn push(
        &mut self,
        severity: EventSeverity,
        category: EventCategory,
        node_id: Option<String>,
        connection_id: Option<String>,
        title: impl Into<String>,
        detail: Option<String>,
        source: &'static str,
    ) {
        self.entries.push_back(EventLogEntry {
            id: self.next_id,
            timestamp: SystemTime::now(),
            severity,
            category,
            node_id,
            connection_id,
            title: title.into(),
            detail,
            source,
        });
        self.next_id = self.next_id.saturating_add(1);
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
        }
        self.unread_count = self.unread_count.saturating_add(1);
        if severity == EventSeverity::Error {
            self.unread_errors = self.unread_errors.saturating_add(1);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.unread_count = 0;
        self.unread_errors = 0;
    }

    pub fn mark_read(&mut self) {
        self.unread_count = 0;
        self.unread_errors = 0;
    }

    pub fn toggle_dnd(&mut self) {
        self.dnd_enabled = !self.dnd_enabled;
    }

    pub fn cycle_severity_filter(&mut self) {
        self.filter.severity = match self.filter.severity {
            EventSeverityFilter::All => EventSeverityFilter::Error,
            EventSeverityFilter::Error => EventSeverityFilter::Warn,
            EventSeverityFilter::Warn => EventSeverityFilter::Info,
            EventSeverityFilter::Info => EventSeverityFilter::All,
        };
    }

    pub fn cycle_category_filter(&mut self) {
        self.filter.category = match self.filter.category {
            EventCategoryFilter::All => EventCategoryFilter::Connection,
            EventCategoryFilter::Connection => EventCategoryFilter::Reconnect,
            EventCategoryFilter::Reconnect => EventCategoryFilter::Node,
            EventCategoryFilter::Node => EventCategoryFilter::All,
        };
    }

    pub fn matches_filter(&self, entry: &EventLogEntry) -> bool {
        let severity_matches = match self.filter.severity {
            EventSeverityFilter::All => true,
            EventSeverityFilter::Error => entry.severity == EventSeverity::Error,
            EventSeverityFilter::Warn => entry.severity == EventSeverity::Warn,
            EventSeverityFilter::Info => entry.severity == EventSeverity::Info,
        };
        let category_matches = match self.filter.category {
            EventCategoryFilter::All => true,
            EventCategoryFilter::Connection => entry.category == EventCategory::Connection,
            EventCategoryFilter::Reconnect => entry.category == EventCategory::Reconnect,
            EventCategoryFilter::Node => entry.category == EventCategory::Node,
        };
        severity_matches && category_matches
    }

    pub fn filtered_counts(&self) -> (usize, usize, usize) {
        let mut info = 0;
        let mut warn = 0;
        let mut error = 0;
        for entry in self
            .entries
            .iter()
            .filter(|entry| self.matches_filter(entry))
        {
            match entry.severity {
                EventSeverity::Info => info += 1,
                EventSeverity::Warn => warn += 1,
                EventSeverity::Error => error += 1,
            }
        }
        (info, warn, error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationKind {
    Connection,
    Security,
    Transfer,
    Update,
    Health,
    Plugin,
    Agent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationStatus {
    Unread,
    Read,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NotificationScope {
    Global,
    Node(String),
    Connection(String),
}

#[derive(Clone, Debug)]
pub struct NotificationEntry {
    pub id: u64,
    pub created_at: SystemTime,
    pub kind: NotificationKind,
    pub severity: NotificationSeverity,
    pub title: String,
    pub body: Option<String>,
    pub status: NotificationStatus,
    pub scope: NotificationScope,
    pub dedupe_key: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationStatusFilter {
    All,
    Unread,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationSeverityFilter {
    All,
    Critical,
    Error,
    Warning,
    Info,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationKindFilter {
    All,
    Connection,
    Security,
    Transfer,
    Update,
    Health,
    Plugin,
    Agent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationFilter {
    pub status: NotificationStatusFilter,
    pub severity: NotificationSeverityFilter,
    pub kind: NotificationKindFilter,
}

impl Default for NotificationFilter {
    fn default() -> Self {
        Self {
            status: NotificationStatusFilter::All,
            severity: NotificationSeverityFilter::All,
            kind: NotificationKindFilter::All,
        }
    }
}

#[derive(Clone, Debug)]
pub struct NotificationState {
    pub entries: VecDeque<NotificationEntry>,
    pub next_id: u64,
    pub unread_count: u32,
    pub unread_critical_count: u32,
    pub filter: NotificationFilter,
    pub dnd_enabled: bool,
    capacity: usize,
}

impl Default for NotificationState {
    fn default() -> Self {
        Self::new(DEFAULT_NOTIFICATION_CAPACITY)
    }
}

impl NotificationState {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            next_id: 1,
            unread_count: 0,
            unread_critical_count: 0,
            filter: NotificationFilter::default(),
            dnd_enabled: true,
            capacity,
        }
    }

    pub fn push(
        &mut self,
        kind: NotificationKind,
        severity: NotificationSeverity,
        title: impl Into<String>,
        body: Option<String>,
        scope: NotificationScope,
        dedupe_key: Option<String>,
    ) {
        let title = title.into();
        if let Some(dedupe_key) = dedupe_key.as_ref()
            && let Some(existing) = self.entries.iter_mut().find(|entry| {
                entry.dedupe_key.as_ref() == Some(dedupe_key)
                    && entry.status != NotificationStatus::Read
            })
        {
            existing.created_at = SystemTime::now();
            existing.kind = kind;
            existing.severity = severity;
            existing.title = title;
            existing.body = body;
            existing.scope = scope;
            existing.status = NotificationStatus::Unread;
            self.recount();
            return;
        }

        self.entries.push_back(NotificationEntry {
            id: self.next_id,
            created_at: SystemTime::now(),
            kind,
            severity,
            title,
            body,
            status: NotificationStatus::Unread,
            scope,
            dedupe_key,
        });
        self.next_id = self.next_id.saturating_add(1);
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
        }
        self.recount();
    }

    pub fn resolve_connection_for_node(&mut self, node_id: &str) {
        self.entries.retain(|entry| {
            let scoped_to_node =
                matches!(&entry.scope, NotificationScope::Node(entry_node_id) if entry_node_id == node_id);
            let connection_kind = matches!(
                entry.kind,
                NotificationKind::Connection | NotificationKind::Security
            );
            !(scoped_to_node && connection_kind)
        });
        self.recount();
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.recount();
    }

    pub fn mark_all_read(&mut self) {
        for entry in &mut self.entries {
            entry.status = NotificationStatus::Read;
        }
        self.recount();
    }

    pub fn mark_read(&mut self, id: u64) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.status = NotificationStatus::Read;
        }
        self.recount();
    }

    pub fn remove(&mut self, id: u64) {
        self.entries.retain(|entry| entry.id != id);
        self.recount();
    }

    pub fn toggle_dnd(&mut self) {
        self.dnd_enabled = !self.dnd_enabled;
    }

    pub fn cycle_status_filter(&mut self) {
        self.filter.status = match self.filter.status {
            NotificationStatusFilter::All => NotificationStatusFilter::Unread,
            NotificationStatusFilter::Unread => NotificationStatusFilter::All,
        };
    }

    pub fn cycle_severity_filter(&mut self) {
        self.filter.severity = match self.filter.severity {
            NotificationSeverityFilter::All => NotificationSeverityFilter::Critical,
            NotificationSeverityFilter::Critical => NotificationSeverityFilter::Error,
            NotificationSeverityFilter::Error => NotificationSeverityFilter::Warning,
            NotificationSeverityFilter::Warning => NotificationSeverityFilter::Info,
            NotificationSeverityFilter::Info => NotificationSeverityFilter::All,
        };
    }

    pub fn cycle_kind_filter(&mut self) {
        self.filter.kind = match self.filter.kind {
            NotificationKindFilter::All => NotificationKindFilter::Connection,
            NotificationKindFilter::Connection => NotificationKindFilter::Security,
            NotificationKindFilter::Security => NotificationKindFilter::Transfer,
            NotificationKindFilter::Transfer => NotificationKindFilter::Update,
            NotificationKindFilter::Update => NotificationKindFilter::Health,
            NotificationKindFilter::Health => NotificationKindFilter::Plugin,
            NotificationKindFilter::Plugin => NotificationKindFilter::Agent,
            NotificationKindFilter::Agent => NotificationKindFilter::All,
        };
    }

    pub fn matches_filter(&self, entry: &NotificationEntry) -> bool {
        let status_matches = match self.filter.status {
            NotificationStatusFilter::All => true,
            NotificationStatusFilter::Unread => entry.status == NotificationStatus::Unread,
        };
        let severity_matches = match self.filter.severity {
            NotificationSeverityFilter::All => true,
            NotificationSeverityFilter::Critical => {
                entry.severity == NotificationSeverity::Critical
            }
            NotificationSeverityFilter::Error => entry.severity == NotificationSeverity::Error,
            NotificationSeverityFilter::Warning => entry.severity == NotificationSeverity::Warning,
            NotificationSeverityFilter::Info => entry.severity == NotificationSeverity::Info,
        };
        let kind_matches = match self.filter.kind {
            NotificationKindFilter::All => true,
            NotificationKindFilter::Connection => entry.kind == NotificationKind::Connection,
            NotificationKindFilter::Security => entry.kind == NotificationKind::Security,
            NotificationKindFilter::Transfer => entry.kind == NotificationKind::Transfer,
            NotificationKindFilter::Update => entry.kind == NotificationKind::Update,
            NotificationKindFilter::Health => entry.kind == NotificationKind::Health,
            NotificationKindFilter::Plugin => entry.kind == NotificationKind::Plugin,
            NotificationKindFilter::Agent => entry.kind == NotificationKind::Agent,
        };
        status_matches && severity_matches && kind_matches
    }

    pub fn recount(&mut self) {
        self.unread_count = 0;
        self.unread_critical_count = 0;
        for entry in &self.entries {
            if entry.status == NotificationStatus::Unread {
                self.unread_count = self.unread_count.saturating_add(1);
                if matches!(
                    entry.severity,
                    NotificationSeverity::Critical | NotificationSeverity::Error
                ) {
                    self.unread_critical_count = self.unread_critical_count.saturating_add(1);
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct NotificationCenterState {
    pub active_view: ActivityView,
    pub event_log: EventLogState,
    pub notifications: NotificationState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_dedupe_refreshes_unread_entry() {
        let mut state = NotificationState::default();

        state.push(
            NotificationKind::Connection,
            NotificationSeverity::Error,
            "Connection lost",
            Some("first".to_string()),
            NotificationScope::Node("node-a".to_string()),
            Some("connection-lost:node-a".to_string()),
        );
        state.push(
            NotificationKind::Connection,
            NotificationSeverity::Critical,
            "Connection lost again",
            Some("second".to_string()),
            NotificationScope::Node("node-a".to_string()),
            Some("connection-lost:node-a".to_string()),
        );

        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.unread_count, 1);
        assert_eq!(state.unread_critical_count, 1);
        assert_eq!(state.entries[0].body.as_deref(), Some("second"));
    }

    #[test]
    fn event_log_counts_respect_filters() {
        let mut state = EventLogState::default();
        state.push(
            EventSeverity::Info,
            EventCategory::Connection,
            None,
            None,
            "connected",
            None,
            "test",
        );
        state.push(
            EventSeverity::Error,
            EventCategory::Node,
            Some("node-a".to_string()),
            None,
            "node error",
            None,
            "test",
        );

        assert_eq!(state.filtered_counts(), (1, 0, 1));
        state.filter.category = EventCategoryFilter::Node;
        assert_eq!(state.filtered_counts(), (0, 0, 1));
    }

    #[test]
    fn resolving_node_connection_notifications_removes_security_and_connection_only() {
        let mut state = NotificationState::default();
        state.push(
            NotificationKind::Connection,
            NotificationSeverity::Error,
            "Connection lost",
            None,
            NotificationScope::Node("node-a".to_string()),
            None,
        );
        state.push(
            NotificationKind::Transfer,
            NotificationSeverity::Warning,
            "Transfer paused",
            None,
            NotificationScope::Node("node-a".to_string()),
            None,
        );

        state.resolve_connection_for_node("node-a");

        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.entries[0].kind, NotificationKind::Transfer);
    }
}
