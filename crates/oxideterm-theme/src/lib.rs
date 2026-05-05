#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalTheme {
    pub background: u32,
    pub foreground: u32,
    pub cursor: u32,
    pub selection_background: &'static str,
    pub black: u32,
    pub red: u32,
    pub green: u32,
    pub yellow: u32,
    pub blue: u32,
    pub magenta: u32,
    pub cyan: u32,
    pub white: u32,
    pub bright_black: u32,
    pub bright_red: u32,
    pub bright_green: u32,
    pub bright_yellow: u32,
    pub bright_blue: u32,
    pub bright_magenta: u32,
    pub bright_cyan: u32,
    pub bright_white: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppUiColors {
    pub bg: u32,
    pub bg_panel: u32,
    pub bg_card: u32,
    pub bg_hover: u32,
    pub bg_active: u32,
    pub bg_secondary: u32,
    pub bg_elevated: u32,
    pub bg_sunken: u32,
    pub text: u32,
    pub text_muted: u32,
    pub text_secondary: u32,
    pub text_heading: u32,
    pub border: u32,
    pub border_strong: u32,
    pub divider: u32,
    pub accent: u32,
    pub accent_hover: u32,
    pub accent_text: u32,
    pub accent_secondary: u32,
    pub success: u32,
    pub warning: u32,
    pub error: u32,
    pub info: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuiltInTheme {
    pub id: &'static str,
    pub terminal: TerminalTheme,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiMetrics {
    pub font_family: &'static str,
    pub traffic_light_x: f32,
    pub traffic_light_y: f32,
    pub traffic_light_diameter: f32,
    pub traffic_light_gap: f32,
    pub titlebar_height: f32,
    pub titlebar_label_font_size: f32,
    pub window_min_width: f32,
    pub window_min_height: f32,
    pub tabbar_height: f32,
    pub tabbar_leading_offset: f32,
    pub tab_width: f32,
    pub tab_min_width: f32,
    pub tab_max_width: f32,
    pub tab_font_size: f32,
    pub tab_icon_size: f32,
    pub tab_close_button_size: f32,
    pub tab_close_icon_size: f32,
    pub tab_active_accent_height: f32,
    pub new_tab_button_width: f32,
    pub new_tab_button_height: f32,
    pub activity_bar_width: f32,
    pub activity_icon_size: f32,
    pub activity_icon_glyph_size: f32,
    pub activity_indicator_inset: f32,
    pub activity_indicator_width: f32,
    pub sidebar_default_width: f32,
    pub sidebar_min_width: f32,
    pub sidebar_max_width: f32,
    pub sidebar_header_height: f32,
    pub sidebar_resize_handle_width: f32,
    pub sidebar_action_size: f32,
    pub sidebar_action_icon_size: f32,
    pub sidebar_title_font_size: f32,
    pub divider_width: f32,
    pub divider_height: f32,
    pub split_handle_size: f32,
    pub min_pane_width: f32,
    pub min_pane_height: f32,
    pub min_main_width: f32,
    pub searchbar_height: f32,
    pub search_input_height: f32,
    pub searchbar_font_size: f32,
    pub empty_sidebar_top_padding: f32,
    pub empty_sidebar_height: f32,
    pub empty_sidebar_icon_size: f32,
    pub empty_sidebar_title_font_size: f32,
    pub empty_sidebar_subtitle_font_size: f32,
    pub empty_sidebar_padding_x: f32,
    pub empty_workspace_padding_x: f32,
    pub empty_workspace_padding_y: f32,
    pub titlebar_label_extra_offset: f32,
    pub modal_width: f32,
    pub modal_header_padding_x: f32,
    pub modal_header_padding_y: f32,
    pub modal_body_padding: f32,
    pub modal_body_gap: f32,
    pub modal_section_gap: f32,
    pub modal_field_gap: f32,
    pub modal_footer_height: f32,
    pub modal_footer_padding_x: f32,
    pub form_input_height: f32,
    pub form_input_padding_x: f32,
    pub form_button_height: f32,
    pub form_button_padding_x: f32,
    pub auth_tab_height: f32,
    pub auth_tab_padding: f32,
    pub form_port_width: f32,
    pub form_host_port_gap: f32,
    pub form_label_font_size: f32,
    pub form_text_font_size: f32,
    pub modal_title_font_size: f32,
    pub modal_description_font_size: f32,
    pub form_checkbox_size: f32,
    pub form_checkbox_glyph_size: f32,
    pub form_caret_width: f32,
    pub form_caret_height: f32,
    pub form_selection_padding_x: f32,
    pub ui_text_xs: f32,
    pub ui_text_sm: f32,
    pub ui_text_base: f32,
    pub ui_text_2xl: f32,
    pub ui_button_sm_height: f32,
    pub ui_button_default_height: f32,
    pub ui_button_lg_height: f32,
    pub ui_button_icon_size: f32,
    pub ui_button_sm_padding_x: f32,
    pub ui_button_default_padding_x: f32,
    pub ui_button_lg_padding_x: f32,
    pub ui_control_height: f32,
    pub ui_control_padding_x: f32,
    pub ui_checkbox_size: f32,
    pub ui_checkbox_icon_size: f32,
    pub ui_radio_size: f32,
    pub ui_radio_dot_size: f32,
    pub ui_tabs_list_height: f32,
    pub ui_tabs_list_padding: f32,
    pub ui_tabs_trigger_padding_x: f32,
    pub ui_tabs_trigger_padding_y: f32,
    pub ui_menu_min_width: f32,
    pub ui_menu_padding: f32,
    pub ui_menu_item_padding_x: f32,
    pub ui_menu_item_padding_y: f32,
    pub ui_menu_inset_padding_left: f32,
    pub ui_menu_indicator_size: f32,
    pub ui_menu_icon_size: f32,
    pub ui_select_max_height: f32,
    pub ui_select_min_width: f32,
    pub ui_select_check_size: f32,
    pub ui_select_shadow_alpha: f32,
    pub ui_progress_height: f32,
    pub ui_slider_track_height: f32,
    pub ui_slider_thumb_size: f32,
    pub ui_tooltip_padding_x: f32,
    pub ui_tooltip_padding_y: f32,
    pub ui_tooltip_shortcut_font_size: f32,
    pub ui_toast_width: f32,
    pub ui_toast_padding: f32,
    pub ui_toast_close_size: f32,
    pub ui_command_input_height: f32,
    pub ui_command_list_max_height: f32,
    pub ui_font_hud_padding_x: f32,
    pub ui_font_hud_padding_y: f32,
    pub settings_content_padding: f32,
    pub settings_row_gap: f32,
    pub settings_page_gap: f32,
    pub settings_card_padding: f32,
    pub settings_card_gap: f32,
    pub settings_card_title_nudge_y: f32,
    pub settings_select_width: f32,
    pub settings_select_narrow_width: f32,
    pub settings_select_popup_gap: f32,
    pub settings_number_input_width: f32,
    pub settings_font_size_input_width: f32,
    pub settings_slider_width: f32,
    pub settings_font_preview_padding: f32,
    pub settings_font_preview_margin_top: f32,
    pub settings_font_preview_label_margin_bottom: f32,
    pub settings_theme_preview_padding: f32,
    pub settings_theme_preview_dot_size: f32,
    pub settings_theme_preview_dot_gap: f32,
    pub settings_theme_preview_line_height: f32,
    pub settings_appearance_action_height: f32,
    pub settings_appearance_select_width: f32,
    pub settings_appearance_fit_select_width: f32,
    pub settings_background_thumb_height: f32,
    pub settings_background_tab_icon_size: f32,
    pub settings_background_badge_padding_x: f32,
    pub settings_background_badge_padding_y: f32,
    pub markdown_body_font_family: &'static str,
    pub markdown_code_font_family: &'static str,
    pub markdown_body_font_size: f32,
    pub markdown_heading_h1_scale: f32,
    pub markdown_heading_h2_scale: f32,
    pub markdown_heading_h3_scale: f32,
    pub markdown_heading_h4_scale: f32,
    pub markdown_heading_h5_scale: f32,
    pub markdown_heading_h6_scale: f32,
    pub markdown_code_font_scale: f32,
    pub markdown_code_label_font_scale: f32,
    pub markdown_footnote_font_scale: f32,
    pub markdown_block_gap: f32,
    pub markdown_list_indent: f32,
    pub markdown_code_block_padding: f32,
    pub markdown_max_image_width: f32,
    pub markdown_blockquote_border_width: f32,
}

impl UiMetrics {
    pub const fn tauri_default() -> Self {
        Self {
            font_family: "SF Pro Text",
            traffic_light_x: 14.0,
            traffic_light_y: 8.0,
            traffic_light_diameter: 14.0,
            traffic_light_gap: 10.0,
            titlebar_height: 30.0,
            titlebar_label_font_size: 13.0,
            window_min_width: 420.0,
            window_min_height: 280.0,
            tabbar_height: 36.0,
            tabbar_leading_offset: 0.0,
            tab_width: 190.0,
            tab_min_width: 120.0,
            tab_max_width: 240.0,
            tab_font_size: 14.0,
            tab_icon_size: 14.0,
            tab_close_button_size: 20.0,
            tab_close_icon_size: 12.0,
            tab_active_accent_height: 2.0,
            new_tab_button_width: 36.0,
            new_tab_button_height: 36.0,
            activity_bar_width: 48.0,
            activity_icon_size: 36.0,
            activity_icon_glyph_size: 20.0,
            activity_indicator_inset: 6.0,
            activity_indicator_width: 2.0,
            sidebar_default_width: 300.0,
            sidebar_min_width: 200.0,
            sidebar_max_width: 600.0,
            sidebar_header_height: 44.0,
            sidebar_resize_handle_width: 4.0,
            sidebar_action_size: 24.0,
            sidebar_action_icon_size: 12.0,
            sidebar_title_font_size: 12.0,
            divider_width: 24.0,
            divider_height: 1.0,
            split_handle_size: 5.0,
            min_pane_width: 80.0,
            min_pane_height: 60.0,
            min_main_width: 240.0,
            searchbar_height: 34.0,
            search_input_height: 24.0,
            searchbar_font_size: 12.0,
            empty_sidebar_top_padding: 66.0,
            empty_sidebar_height: 128.0,
            empty_sidebar_icon_size: 32.0,
            empty_sidebar_title_font_size: 14.0,
            empty_sidebar_subtitle_font_size: 12.0,
            empty_sidebar_padding_x: 16.0,
            empty_workspace_padding_x: 16.0,
            empty_workspace_padding_y: 8.0,
            titlebar_label_extra_offset: 18.0,
            modal_width: 512.0,
            modal_header_padding_x: 16.0,
            modal_header_padding_y: 12.0,
            modal_body_padding: 16.0,
            modal_body_gap: 24.0,
            modal_section_gap: 16.0,
            modal_field_gap: 8.0,
            modal_footer_height: 60.0,
            modal_footer_padding_x: 16.0,
            form_input_height: 36.0,
            form_input_padding_x: 12.0,
            form_button_height: 36.0,
            form_button_padding_x: 16.0,
            auth_tab_height: 36.0,
            auth_tab_padding: 4.0,
            form_port_width: 108.0,
            form_host_port_gap: 16.0,
            form_label_font_size: 14.0,
            form_text_font_size: 14.0,
            modal_title_font_size: 14.0,
            modal_description_font_size: 14.0,
            form_checkbox_size: 16.0,
            form_checkbox_glyph_size: 12.0,
            form_caret_width: 1.0,
            form_caret_height: 20.0,
            form_selection_padding_x: 2.0,
            ui_text_xs: 12.0,
            ui_text_sm: 14.0,
            ui_text_base: 16.0,
            ui_text_2xl: 24.0,
            ui_button_sm_height: 32.0,
            ui_button_default_height: 36.0,
            ui_button_lg_height: 40.0,
            ui_button_icon_size: 36.0,
            ui_button_sm_padding_x: 12.0,
            ui_button_default_padding_x: 16.0,
            ui_button_lg_padding_x: 32.0,
            ui_control_height: 36.0,
            ui_control_padding_x: 12.0,
            ui_checkbox_size: 16.0,
            ui_checkbox_icon_size: 12.0,
            ui_radio_size: 16.0,
            ui_radio_dot_size: 10.0,
            ui_tabs_list_height: 36.0,
            ui_tabs_list_padding: 4.0,
            ui_tabs_trigger_padding_x: 12.0,
            ui_tabs_trigger_padding_y: 6.0,
            ui_menu_min_width: 128.0,
            ui_menu_padding: 4.0,
            ui_menu_item_padding_x: 8.0,
            ui_menu_item_padding_y: 6.0,
            ui_menu_inset_padding_left: 32.0,
            ui_menu_indicator_size: 14.0,
            ui_menu_icon_size: 16.0,
            ui_select_max_height: 384.0,
            ui_select_min_width: 128.0,
            ui_select_check_size: 14.0,
            ui_select_shadow_alpha: 0.20,
            ui_progress_height: 8.0,
            ui_slider_track_height: 6.0,
            ui_slider_thumb_size: 16.0,
            ui_tooltip_padding_x: 8.0,
            ui_tooltip_padding_y: 4.0,
            ui_tooltip_shortcut_font_size: 10.0,
            ui_toast_width: 420.0,
            ui_toast_padding: 16.0,
            ui_toast_close_size: 16.0,
            ui_command_input_height: 40.0,
            ui_command_list_max_height: 400.0,
            ui_font_hud_padding_x: 20.0,
            ui_font_hud_padding_y: 12.0,
            settings_content_padding: 40.0,
            settings_row_gap: 24.0,
            settings_page_gap: 32.0,
            settings_card_padding: 20.0,
            settings_card_gap: 20.0,
            settings_card_title_nudge_y: -4.0,
            settings_select_width: 200.0,
            settings_select_narrow_width: 160.0,
            settings_select_popup_gap: 4.0,
            settings_number_input_width: 80.0,
            settings_font_size_input_width: 64.0,
            settings_slider_width: 128.0,
            settings_font_preview_padding: 16.0,
            settings_font_preview_margin_top: 4.0,
            settings_font_preview_label_margin_bottom: 8.0,
            settings_theme_preview_padding: 12.0,
            settings_theme_preview_dot_size: 12.0,
            settings_theme_preview_dot_gap: 8.0,
            settings_theme_preview_line_height: 18.0,
            settings_appearance_action_height: 28.0,
            settings_appearance_select_width: 180.0,
            settings_appearance_fit_select_width: 128.0,
            settings_background_thumb_height: 96.0,
            settings_background_tab_icon_size: 16.0,
            settings_background_badge_padding_x: 6.0,
            settings_background_badge_padding_y: 2.0,
            markdown_body_font_family: "SF Pro Text",
            markdown_code_font_family: "JetBrainsMono Nerd Font",
            markdown_body_font_size: 14.0,
            markdown_heading_h1_scale: 2.0,
            markdown_heading_h2_scale: 1.5,
            markdown_heading_h3_scale: 1.25,
            markdown_heading_h4_scale: 1.1,
            markdown_heading_h5_scale: 1.0,
            markdown_heading_h6_scale: 0.9,
            markdown_code_font_scale: 0.9,
            markdown_code_label_font_scale: 0.85,
            markdown_footnote_font_scale: 0.9,
            markdown_block_gap: 12.0,
            markdown_list_indent: 20.0,
            markdown_code_block_padding: 12.0,
            markdown_max_image_width: 400.0,
            markdown_blockquote_border_width: 3.0,
        }
    }

    pub fn titlebar_label_x(self) -> f32 {
        self.traffic_light_x
            + self.traffic_light_diameter * 3.0
            + self.traffic_light_gap * 2.0
            + self.titlebar_label_extra_offset
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiRadii {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub active_indicator: f32,
}

impl UiRadii {
    pub const fn tauri_default() -> Self {
        Self {
            xs: 2.0,
            sm: 4.0,
            md: 6.0,
            lg: 10.0,
            active_indicator: 2.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiSpacing {
    pub one: f32,
    pub two: f32,
    pub three: f32,
    pub icon_gap: f32,
}

impl UiSpacing {
    pub const fn tauri_default() -> Self {
        Self {
            one: 4.0,
            two: 8.0,
            three: 12.0,
            icon_gap: 8.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeTokens {
    pub terminal: TerminalTheme,
    pub ui: AppUiColors,
    pub metrics: UiMetrics,
    pub radii: UiRadii,
    pub spacing: UiSpacing,
}

impl ThemeTokens {
    pub fn from_builtin(theme: BuiltInTheme) -> Self {
        Self {
            terminal: theme.terminal,
            ui: derive_ui_colors_from_terminal(theme.terminal),
            metrics: UiMetrics::tauri_default(),
            radii: UiRadii::tauri_default(),
            spacing: UiSpacing::tauri_default(),
        }
    }
}

pub fn theme_by_id(id: &str) -> BuiltInTheme {
    BUILT_IN_THEMES
        .iter()
        .copied()
        .find(|theme| theme.id == id)
        .unwrap_or(DEFAULT_THEME)
}

pub fn default_tokens() -> ThemeTokens {
    ThemeTokens::from_builtin(DEFAULT_THEME)
}

pub fn derive_ui_colors_from_terminal(theme: TerminalTheme) -> AppUiColors {
    AppUiColors {
        bg: theme.background,
        bg_panel: shift(theme.background, 15),
        bg_card: shift(theme.background, 20),
        bg_hover: shift(theme.background, 30),
        bg_active: shift(theme.background, 40),
        bg_secondary: shift(theme.background, 10),
        bg_elevated: shift(theme.background, 22),
        bg_sunken: shift(theme.background, -10),
        text: theme.foreground,
        text_muted: theme.bright_black,
        text_secondary: mix(theme.foreground, theme.bright_black, 0.5),
        text_heading: shift(theme.foreground, 8),
        border: shift(theme.background, 30),
        border_strong: mix(theme.cursor, theme.foreground, 0.6),
        divider: shift(theme.background, 20),
        accent: theme.cursor,
        accent_hover: shift(theme.cursor, -20),
        accent_text: mix(theme.cursor, theme.background, 0.7),
        accent_secondary: theme.bright_black,
        success: theme.green,
        warning: theme.yellow,
        error: theme.red,
        info: theme.blue,
    }
}

fn shift(hex: u32, amount: i32) -> u32 {
    let r = clamp_channel(((hex >> 16) & 0xff) as i32 + amount);
    let g = clamp_channel(((hex >> 8) & 0xff) as i32 + amount);
    let b = clamp_channel((hex & 0xff) as i32 + amount);
    (r << 16) | (g << 8) | b
}

fn mix(c1: u32, c2: u32, ratio: f32) -> u32 {
    let inverse = 1.0 - ratio;
    let r =
        (((c1 >> 16) & 0xff) as f32 * ratio + ((c2 >> 16) & 0xff) as f32 * inverse).round() as u32;
    let g =
        (((c1 >> 8) & 0xff) as f32 * ratio + ((c2 >> 8) & 0xff) as f32 * inverse).round() as u32;
    let b = ((c1 & 0xff) as f32 * ratio + (c2 & 0xff) as f32 * inverse).round() as u32;
    (r.min(255) << 16) | (g.min(255) << 8) | b.min(255)
}

fn clamp_channel(value: i32) -> u32 {
    value.clamp(0, 255) as u32
}

include!("generated.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_all_tauri_builtin_themes() {
        assert_eq!(BUILT_IN_THEMES.len(), 31);
        assert_eq!(theme_by_id("default").terminal.background, 0x09090b);
        assert_eq!(theme_by_id("spring-green").terminal.cursor, 0x16a34a);
    }

    #[test]
    fn derives_ui_colors_like_tauri() {
        let ui = derive_ui_colors_from_terminal(theme_by_id("default").terminal);
        assert_eq!(ui.bg, 0x09090b);
        assert_eq!(ui.bg_panel, 0x18181a);
        assert_eq!(ui.bg_hover, 0x272729);
        assert_eq!(ui.accent, 0xea580c);
    }
}
