use gpui::{ListAlignment, ListState, px};
use oxideterm_gpui_ui::virtual_list::{
    TauriVirtualListSpec, VirtualListSignatureCache, sync_tauri_variable_list_state_by_signatures,
};
use oxideterm_settings_model::{
    SettingsDynamicSectionCounts, SettingsSectionId, SettingsTab, settings_section_list_identity,
};
use std::cell::RefCell;

/// Shared by the surface and commands that scroll to a semantic settings section.
pub struct SettingsSectionList {
    state: ListState,
    cache: RefCell<VirtualListSignatureCache>,
}

impl Default for SettingsSectionList {
    fn default() -> Self {
        Self {
            state: ListState::new(
                4,
                ListAlignment::Top,
                Self::spec(SettingsTab::General).overdraw(),
            )
            .measure_all(),
            cache: RefCell::new(VirtualListSignatureCache::default()),
        }
    }
}

impl SettingsSectionList {
    pub fn scroll_to_section(
        &self,
        tab: SettingsTab,
        section: SettingsSectionId,
        counts: SettingsDynamicSectionCounts,
    ) {
        if let Some(item_ix) =
            oxideterm_settings_model::settings_section_list_item_index(tab, section, counts)
        {
            self.state.scroll_to(gpui::ListOffset {
                item_ix,
                offset_in_item: px(0.0),
            });
        }
    }

    pub fn state(&self) -> &ListState {
        &self.state
    }

    pub fn spec(tab: SettingsTab) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(px(if tab == SettingsTab::Ai { 360.0 } else { 260.0 }), 2)
    }

    pub fn sync(&self, tab: SettingsTab, counts: SettingsDynamicSectionCounts, signatures: &[u64]) {
        let identity = settings_section_list_identity(tab, counts.terminal_page, counts.ai_page);
        sync_tauri_variable_list_state_by_signatures(
            &self.state,
            &mut self.cache.borrow_mut(),
            &identity,
            signatures,
            Self::spec(tab),
        );
    }
}
