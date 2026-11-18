use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_i18n_error(&self, key: &str, error: &str) -> String {
        self.i18n.t(key).replace("{{error}}", error)
    }
}
