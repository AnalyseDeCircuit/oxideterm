use std::fmt;

#[derive(Clone, Debug)]
pub struct DesktopPresenceMenu {
    pub app_name: String,
    pub status_title: String,
    pub status_icon: Option<DesktopPresenceIcon>,
    pub show_main_window: String,
    pub hide_main_window: String,
    pub new_connection: String,
    pub settings: String,
    pub check_for_updates: String,
    pub quit: String,
}

#[derive(Clone, Copy)]
pub struct DesktopPresenceIcon {
    pub template_png_bytes: &'static [u8],
    pub point_size: f64,
}

impl fmt::Debug for DesktopPresenceIcon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DesktopPresenceIcon")
            .field("template_png_bytes_len", &self.template_png_bytes.len())
            .field("point_size", &self.point_size)
            .finish()
    }
}

impl DesktopPresenceMenu {
    pub fn fallback() -> Self {
        Self {
            app_name: "OxideTerm".to_string(),
            status_title: "Ox".to_string(),
            status_icon: None,
            show_main_window: "Show Main Window".to_string(),
            hide_main_window: "Hide Main Window".to_string(),
            new_connection: "New Connection".to_string(),
            settings: "Settings".to_string(),
            check_for_updates: "Check for Updates".to_string(),
            quit: "Quit OxideTerm".to_string(),
        }
    }
}
