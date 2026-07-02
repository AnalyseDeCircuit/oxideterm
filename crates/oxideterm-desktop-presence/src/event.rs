#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopPresenceEvent {
    ShowMainWindow,
    HideMainWindow,
    NewConnection,
    OpenSettings,
    CheckForUpdates,
    Quit,
}
