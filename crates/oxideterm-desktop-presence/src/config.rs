#[derive(Clone, Debug)]
pub struct DesktopPresenceMenu {
    pub app_name: String,
    pub show_main_window: String,
    pub hide_main_window: String,
    pub new_connection: String,
    pub settings: String,
    pub check_for_updates: String,
    pub quit: String,
}

impl DesktopPresenceMenu {
    pub fn fallback() -> Self {
        Self {
            app_name: "OxideTerm".to_string(),
            show_main_window: "Show Main Window".to_string(),
            hide_main_window: "Hide Main Window".to_string(),
            new_connection: "New Connection".to_string(),
            settings: "Settings".to_string(),
            check_for_updates: "Check for Updates".to_string(),
            quit: "Quit OxideTerm".to_string(),
        }
    }
}
