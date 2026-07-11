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
    pub tab_min_width: f32,
    pub tab_max_width: f32,
    pub tab_padding_x: f32,
    pub tab_gap: f32,
    pub tab_title_width_ratio: f32,
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
    pub modal_max_viewport_height_ratio: f32,
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
    pub settings_content_max_width: f32,
    pub settings_content_wide_max_width: f32,
    pub settings_nav_width: f32,
    pub settings_row_gap: f32,
    pub settings_page_gap: f32,
    pub settings_card_padding: f32,
    pub settings_card_gap: f32,
    pub settings_card_title_nudge_y: f32,
    pub settings_select_width: f32,
    pub settings_select_narrow_width: f32,
    pub settings_select_popup_gap: f32,
    pub settings_theme_select_popup_max_height: f32,
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
    pub window_vibrancy_tint_alpha: f32,
    pub panel_vibrancy_alpha: f32,
    pub terminal_vibrancy_alpha: f32,
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
            // Tauri declares the main window minimum size as 800x600.
            window_min_width: 800.0,
            window_min_height: 600.0,
            tabbar_height: 36.0,
            tabbar_leading_offset: 0.0,
            tab_min_width: 120.0,
            tab_max_width: 240.0,
            tab_padding_x: 12.0,
            tab_gap: 8.0,
            tab_title_width_ratio: 0.62,
            tab_font_size: 14.0,
            tab_icon_size: 14.0,
            tab_close_button_size: 16.0,
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
            modal_max_viewport_height_ratio: 0.9,
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
            settings_content_max_width: 896.0,
            settings_content_wide_max_width: 1280.0,
            settings_nav_width: 224.0,
            settings_row_gap: 24.0,
            settings_page_gap: 32.0,
            settings_card_padding: 20.0,
            settings_card_gap: 20.0,
            settings_card_title_nudge_y: -4.0,
            settings_select_width: 200.0,
            settings_select_narrow_width: 160.0,
            settings_select_popup_gap: 4.0,
            settings_theme_select_popup_max_height: 300.0,
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
            window_vibrancy_tint_alpha: 0.72,
            panel_vibrancy_alpha: 0.78,
            terminal_vibrancy_alpha: 0.92,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiDensityProfile {
    Compact,
    Comfortable,
    Spacious,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiMotionProfile {
    Off,
    Reduced,
    Normal,
    Fast,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiMotion {
    pub enabled: bool,
    pub spatial_enabled: bool,
    pub short_duration_ms: u64,
    pub normal_duration_ms: u64,
    pub long_duration_ms: u64,
}

impl UiMotion {
    pub const fn normal() -> Self {
        Self {
            enabled: true,
            spatial_enabled: true,
            short_duration_ms: 120,
            normal_duration_ms: 200,
            long_duration_ms: 300,
        }
    }

    pub const fn from_profile(profile: UiMotionProfile) -> Self {
        match profile {
            UiMotionProfile::Off => Self {
                enabled: false,
                spatial_enabled: false,
                short_duration_ms: 0,
                normal_duration_ms: 0,
                long_duration_ms: 0,
            },
            UiMotionProfile::Reduced => Self {
                enabled: true,
                spatial_enabled: false,
                short_duration_ms: 80,
                normal_duration_ms: 120,
                long_duration_ms: 150,
            },
            UiMotionProfile::Normal => Self::normal(),
            UiMotionProfile::Fast => Self {
                enabled: true,
                spatial_enabled: true,
                short_duration_ms: 70,
                normal_duration_ms: 110,
                long_duration_ms: 160,
            },
        }
    }

    pub fn scaled_duration_ms(self, baseline_ms: u64) -> u64 {
        if !self.enabled {
            return 0;
        }

        // Existing animations declare their normal-speed duration. Scaling
        // against the normal long duration preserves their relative cadence.
        baseline_ms.saturating_mul(self.long_duration_ms) / Self::normal().long_duration_ms
    }
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
    pub density: UiDensityProfile,
    pub motion: UiMotion,
}

impl ThemeTokens {
    pub fn from_builtin(theme: BuiltInTheme) -> Self {
        let mut ui = apply_tauri_ui_overrides(
            if theme.id == "default" {
                "neutral"
            } else {
                theme.id
            },
            derive_ui_colors_from_terminal(theme.terminal),
        );
        if color_contrast_ratio(ui.accent_text, ui.accent) < 4.5 {
            // Built-in themes must provide readable text on accent-filled
            // controls even when their source palette did not define one.
            ui.accent_text = highest_contrast_across(
                [ui.accent],
                [
                    ui.bg,
                    ui.text,
                    theme.terminal.bright_white,
                    theme.terminal.black,
                    0xffffff,
                    0x000000,
                ],
            );
        }
        if color_contrast_ratio(ui.text, ui.bg).min(color_contrast_ratio(ui.text, ui.bg_panel))
            < 4.5
        {
            // Prefer colors already present in the terminal palette before
            // falling back to absolute black or white.
            ui.text = highest_contrast_across(
                [ui.bg, ui.bg_panel],
                [
                    ui.text,
                    ui.text_heading,
                    theme.terminal.bright_white,
                    theme.terminal.black,
                ],
            );
        }
        ui.success = ensure_minimum_contrast(ui.success, ui.bg, 3.0);
        ui.warning = ensure_minimum_contrast(ui.warning, ui.bg, 3.0);
        ui.error = ensure_minimum_contrast(ui.error, ui.bg, 3.0);
        ui.info = ensure_minimum_contrast(ui.info, ui.bg, 3.0);
        Self {
            terminal: theme.terminal,
            ui,
            metrics: UiMetrics::tauri_default(),
            radii: UiRadii::tauri_default(),
            spacing: UiSpacing::tauri_default(),
            density: UiDensityProfile::Comfortable,
            motion: UiMotion::normal(),
        }
    }

    pub fn apply_density(&mut self, density: UiDensityProfile) {
        let scale = match density {
            UiDensityProfile::Compact => 0.82,
            UiDensityProfile::Comfortable => 1.0,
            UiDensityProfile::Spacious => 1.18,
        };
        self.density = density;
        self.spacing = UiSpacing {
            one: scaled_metric(UiSpacing::tauri_default().one, scale),
            two: scaled_metric(UiSpacing::tauri_default().two, scale),
            three: scaled_metric(UiSpacing::tauri_default().three, scale),
            icon_gap: scaled_metric(UiSpacing::tauri_default().icon_gap, scale),
        };
        apply_density_to_metrics(&mut self.metrics, scale);
    }

    pub fn apply_motion(&mut self, profile: UiMotionProfile) {
        self.motion = UiMotion::from_profile(profile);
    }
}

fn scaled_metric(value: f32, scale: f32) -> f32 {
    (value * scale * 2.0).round() / 2.0
}

fn apply_density_to_metrics(metrics: &mut UiMetrics, scale: f32) {
    // Density changes spatial rhythm and control hitboxes while preserving the
    // user's font sizes, window bounds, and terminal geometry preferences.
    for value in [
        &mut metrics.tabbar_height,
        &mut metrics.tab_padding_x,
        &mut metrics.tab_gap,
        &mut metrics.new_tab_button_width,
        &mut metrics.new_tab_button_height,
        &mut metrics.activity_icon_size,
        &mut metrics.activity_indicator_inset,
        &mut metrics.sidebar_header_height,
        &mut metrics.sidebar_action_size,
        &mut metrics.searchbar_height,
        &mut metrics.search_input_height,
        &mut metrics.modal_header_padding_x,
        &mut metrics.modal_header_padding_y,
        &mut metrics.modal_body_padding,
        &mut metrics.modal_body_gap,
        &mut metrics.modal_section_gap,
        &mut metrics.modal_field_gap,
        &mut metrics.modal_footer_height,
        &mut metrics.modal_footer_padding_x,
        &mut metrics.form_input_height,
        &mut metrics.form_input_padding_x,
        &mut metrics.form_button_height,
        &mut metrics.form_button_padding_x,
        &mut metrics.auth_tab_height,
        &mut metrics.auth_tab_padding,
        &mut metrics.ui_button_sm_height,
        &mut metrics.ui_button_default_height,
        &mut metrics.ui_button_lg_height,
        &mut metrics.ui_button_sm_padding_x,
        &mut metrics.ui_button_default_padding_x,
        &mut metrics.ui_button_lg_padding_x,
        &mut metrics.ui_control_height,
        &mut metrics.ui_control_padding_x,
        &mut metrics.ui_tabs_list_height,
        &mut metrics.ui_tabs_list_padding,
        &mut metrics.ui_tabs_trigger_padding_x,
        &mut metrics.ui_tabs_trigger_padding_y,
        &mut metrics.ui_menu_padding,
        &mut metrics.ui_menu_item_padding_x,
        &mut metrics.ui_menu_item_padding_y,
        &mut metrics.ui_tooltip_padding_x,
        &mut metrics.ui_tooltip_padding_y,
        &mut metrics.ui_toast_padding,
        &mut metrics.ui_command_input_height,
        &mut metrics.ui_command_list_max_height,
        &mut metrics.settings_content_padding,
        &mut metrics.settings_row_gap,
        &mut metrics.settings_page_gap,
        &mut metrics.settings_card_padding,
        &mut metrics.settings_card_gap,
    ] {
        *value = scaled_metric(*value, scale);
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

fn color_relative_luminance(color: u32) -> f32 {
    let channel = |shift: u32| {
        let value = ((color >> shift) & 0xff_u32) as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

fn color_contrast_ratio(foreground: u32, background: u32) -> f32 {
    let foreground = color_relative_luminance(foreground);
    let background = color_relative_luminance(background);
    let (lighter, darker) = if foreground >= background {
        (foreground, background)
    } else {
        (background, foreground)
    };
    (lighter + 0.05) / (darker + 0.05)
}

fn highest_contrast_across<const B: usize, const C: usize>(
    backgrounds: [u32; B],
    candidates: [u32; C],
) -> u32 {
    candidates
        .into_iter()
        .max_by(|left, right| {
            let left_ratio = backgrounds
                .iter()
                .map(|background| color_contrast_ratio(*left, *background))
                .fold(f32::INFINITY, f32::min);
            let right_ratio = backgrounds
                .iter()
                .map(|background| color_contrast_ratio(*right, *background))
                .fold(f32::INFINITY, f32::min);
            left_ratio.total_cmp(&right_ratio)
        })
        .unwrap_or(0xffffff)
}

fn ensure_minimum_contrast(color: u32, background: u32, minimum: f32) -> u32 {
    if color_contrast_ratio(color, background) >= minimum {
        return color;
    }
    let target = highest_contrast_across([background], [0x000000, 0xffffff]);
    for step in 1..=20 {
        let adjusted = mix(target, color, step as f32 / 20.0);
        if color_contrast_ratio(adjusted, background) >= minimum {
            return adjusted;
        }
    }
    target
}

fn clamp_channel(value: i32) -> u32 {
    value.clamp(0, 255) as u32
}

include!("generated.rs");
include!("generated_ui.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn has_all_tauri_builtin_themes() {
        const EXPECTED_THEME_IDS: [&str; 31] = [
            "default",
            "oxide",
            "dracula",
            "nord",
            "solarized-dark",
            "one-dark",
            "monokai",
            "catppuccin-mocha",
            "github-dark",
            "verdigris",
            "silver-oxide",
            "cuprite",
            "chromium-oxide",
            "paper-oxide",
            "magnetite",
            "cobalt",
            "ochre",
            "tokyo-night",
            "gruvbox-dark",
            "rose-pine",
            "kanagawa",
            "synthwave-84",
            "azurite",
            "malachite",
            "hematite",
            "bismuth",
            "fairy-floss",
            "sakura",
            "hot-pink",
            "spring-rice",
            "spring-green",
        ];
        assert_eq!(BUILT_IN_THEMES.len(), EXPECTED_THEME_IDS.len());
        assert_eq!(
            BUILT_IN_THEMES
                .iter()
                .map(|theme| theme.id)
                .collect::<Vec<_>>(),
            EXPECTED_THEME_IDS
        );
        let unique_ids = BUILT_IN_THEMES
            .iter()
            .map(|theme| theme.id)
            .collect::<HashSet<_>>();
        assert_eq!(unique_ids.len(), BUILT_IN_THEMES.len());
        assert_eq!(theme_by_id("default").terminal.background, 0x09090b);
        assert_eq!(theme_by_id("spring-green").terminal.cursor, 0x16a34a);
    }

    #[test]
    fn derives_ui_colors_like_tauri() {
        let ui = ThemeTokens::from_builtin(theme_by_id("default")).ui;
        assert_eq!(ui.bg, 0x09090b);
        assert_eq!(ui.bg_panel, 0x18181b);
        assert_eq!(ui.bg_hover, 0x27272a);
        assert_eq!(ui.accent, 0xea580c);
    }

    #[test]
    fn applies_tauri_css_theme_overrides() {
        let oxide = ThemeTokens::from_builtin(theme_by_id("oxide")).ui;
        assert_eq!(oxide.bg_panel, 0x291c16);
        assert_eq!(oxide.bg_card, 0x33231b);
        assert_eq!(oxide.border, 0x493126);
        assert_eq!(oxide.text_muted, 0x9d887b);

        let github = ThemeTokens::from_builtin(theme_by_id("github-dark")).ui;
        assert_eq!(github.bg_panel, 0x161b22);
        assert_eq!(github.bg_elevated, 0x1c2332);
        assert_eq!(github.accent, 0x58a6ff);
    }

    #[test]
    fn density_profiles_change_spatial_metrics_without_changing_type_or_color() {
        let mut compact = default_tokens();
        let comfortable = default_tokens();
        compact.apply_density(UiDensityProfile::Compact);
        let mut spacious = default_tokens();
        spacious.apply_density(UiDensityProfile::Spacious);

        assert!(compact.metrics.ui_control_height < comfortable.metrics.ui_control_height);
        assert!(spacious.metrics.ui_control_height > comfortable.metrics.ui_control_height);
        assert!(compact.spacing.two < comfortable.spacing.two);
        assert_eq!(compact.metrics.ui_text_sm, comfortable.metrics.ui_text_sm);
        assert_eq!(compact.ui, comfortable.ui);
    }

    #[test]
    fn every_builtin_theme_preserves_colors_across_density_profiles() {
        for theme in BUILT_IN_THEMES {
            let comfortable = ThemeTokens::from_builtin(*theme);
            let mut compact = comfortable;
            compact.apply_density(UiDensityProfile::Compact);
            let mut spacious = comfortable;
            spacious.apply_density(UiDensityProfile::Spacious);

            assert_eq!(compact.ui, comfortable.ui, "{} compact colors", theme.id);
            assert_eq!(spacious.ui, comfortable.ui, "{} spacious colors", theme.id);
            assert!(
                compact.metrics.ui_control_height < comfortable.metrics.ui_control_height,
                "{} compact controls",
                theme.id
            );
            assert!(
                spacious.metrics.ui_control_height > comfortable.metrics.ui_control_height,
                "{} spacious controls",
                theme.id
            );
        }
    }

    #[test]
    fn every_builtin_theme_meets_core_text_and_status_contrast() {
        for theme in BUILT_IN_THEMES {
            let ui = ThemeTokens::from_builtin(*theme).ui;
            for (label, foreground, background, minimum) in [
                ("text/background", ui.text, ui.bg, 4.5),
                ("text/panel", ui.text, ui.bg_panel, 4.5),
                ("accent text/accent", ui.accent_text, ui.accent, 4.5),
                ("success/background", ui.success, ui.bg, 3.0),
                ("warning/background", ui.warning, ui.bg, 3.0),
                ("error/background", ui.error, ui.bg, 3.0),
                ("info/background", ui.info, ui.bg, 3.0),
            ] {
                let ratio = color_contrast_ratio(foreground, background);
                assert!(
                    ratio >= minimum,
                    "{} {label} contrast {ratio:.2} is below {minimum:.1}",
                    theme.id
                );
            }
            if [
                "oxide",
                "verdigris",
                "silver-oxide",
                "cuprite",
                "chromium-oxide",
                "paper-oxide",
                "magnetite",
                "cobalt",
                "ochre",
                "azurite",
                "malachite",
                "hematite",
                "bismuth",
            ]
            .contains(&theme.id)
            {
                for (label, foreground, background, minimum) in [
                    ("text/card", ui.text, ui.bg_card, 4.5),
                    ("heading/background", ui.text_heading, ui.bg, 4.5),
                    ("heading/card", ui.text_heading, ui.bg_card, 4.5),
                    ("accent/background", ui.accent, ui.bg, 3.0),
                ] {
                    let ratio = color_contrast_ratio(foreground, background);
                    assert!(
                        ratio >= minimum,
                        "{} {label} contrast {ratio:.2} is below {minimum:.1}",
                        theme.id
                    );
                }
                for (label, background) in [
                    ("background", ui.bg),
                    ("panel", ui.bg_panel),
                    ("card", ui.bg_card),
                ] {
                    let ratio = color_contrast_ratio(ui.text_muted, background);
                    assert!(
                        ratio >= 3.0,
                        "{} muted/{label} contrast {ratio:.2} is below 3.0",
                        theme.id
                    );
                }
            }
        }
    }

    #[test]
    fn material_theme_accents_preserve_their_named_color_identity() {
        let accent = |id| ThemeTokens::from_builtin(theme_by_id(id)).ui.accent;
        let channels = |color: u32| {
            (
                ((color >> 16) & 0xff) as i32,
                ((color >> 8) & 0xff) as i32,
                (color & 0xff) as i32,
            )
        };

        let (oxide_r, oxide_g, oxide_b) = channels(accent("oxide"));
        assert!(oxide_r > oxide_g && oxide_g > oxide_b);
        let (verdigris_r, verdigris_g, verdigris_b) = channels(accent("verdigris"));
        assert!(verdigris_g > verdigris_b && verdigris_b > verdigris_r);
        let (silver_r, silver_g, silver_b) = channels(accent("silver-oxide"));
        assert!(
            [silver_r, silver_g, silver_b].iter().max().unwrap()
                - [silver_r, silver_g, silver_b].iter().min().unwrap()
                <= 5
        );

        for id in ["cuprite", "hematite"] {
            let (red, green, blue) = channels(accent(id));
            assert!(
                red > green && red > blue,
                "{id} keeps an iron/copper red accent"
            );
        }
        for id in ["chromium-oxide", "malachite"] {
            let (red, green, blue) = channels(accent(id));
            assert!(
                green > red && green > blue,
                "{id} keeps a mineral green accent"
            );
        }
        for id in ["cobalt", "azurite"] {
            let (red, green, blue) = channels(accent(id));
            assert!(
                blue > green && green > red,
                "{id} keeps a mineral blue accent"
            );
        }
        let (ochre_r, ochre_g, ochre_b) = channels(accent("ochre"));
        assert!(ochre_r > ochre_g && ochre_g > ochre_b);
        let (bismuth_r, bismuth_g, bismuth_b) = channels(accent("bismuth"));
        assert!(bismuth_r > bismuth_g && bismuth_b > bismuth_g);

        let paper = ThemeTokens::from_builtin(theme_by_id("paper-oxide")).ui;
        assert!(color_relative_luminance(paper.bg) > 0.75);
        let magnetite = ThemeTokens::from_builtin(theme_by_id("magnetite")).ui;
        assert!(color_relative_luminance(magnetite.bg) < 0.03);
    }

    #[test]
    fn motion_profiles_provide_explicit_reduced_and_disabled_timings() {
        let mut tokens = default_tokens();
        tokens.apply_motion(UiMotionProfile::Reduced);
        assert!(tokens.motion.enabled);
        assert!(!tokens.motion.spatial_enabled);
        assert!(tokens.motion.normal_duration_ms < UiMotion::normal().normal_duration_ms);

        tokens.apply_motion(UiMotionProfile::Off);
        assert!(!tokens.motion.enabled);
        assert!(!tokens.motion.spatial_enabled);
        assert_eq!(tokens.motion.long_duration_ms, 0);
        assert_eq!(tokens.motion.scaled_duration_ms(840), 0);

        tokens.apply_motion(UiMotionProfile::Fast);
        assert!(tokens.motion.spatial_enabled);
        assert!(tokens.motion.scaled_duration_ms(840) < 840);
    }
}
