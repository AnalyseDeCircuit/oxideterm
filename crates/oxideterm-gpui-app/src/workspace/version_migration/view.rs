// Hallmark · pre-emit critique: P4 H4 E4 S5 R5 V4
// Hallmark · macrostructure: Narrative Workflow · genre: modern-minimal · tone: technical and restrained · anchor: project theme accent

use super::*;
use crate::workspace::settings::{CLI_COMPANION_COMMAND_NAME, LEGACY_CLI_COMPANION_COMMAND_NAME};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_with,
};
use oxideterm_gpui_ui::scroll::ScrollableElement;
use oxideterm_gpui_ui::{
    StatusPillOptions, StatusTone, SurfaceKind, SurfaceOptions, SurfacePadding, semantic_surface,
    status_pill,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn render_version_migration_modal(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = window.viewport_size();
        let available_width = f32::from(viewport.width) - VERSION_MIGRATION_VIEWPORT_MARGIN * 2.0;
        let available_height = f32::from(viewport.height) - VERSION_MIGRATION_VIEWPORT_MARGIN * 2.0;
        let dialog_width = available_width
            .min(VERSION_MIGRATION_DIALOG_MAX_WIDTH)
            .max(280.0);
        let dialog_height = available_height
            .min(VERSION_MIGRATION_DIALOG_MAX_HEIGHT)
            .max(320.0);
        let compact = dialog_width < VERSION_MIGRATION_COMPACT_WIDTH;

        oxideterm_gpui_ui::modal::dialog_backdrop()
            .child(
                oxideterm_gpui_ui::modal_container(&self.tokens)
                    .w(px(dialog_width))
                    .h(px(dialog_height))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(self.version_migration_progress(compact, cx))
                    .child(
                        div()
                            .id("version-migration-body")
                            .flex_1()
                            .min_h(px(0.0))
                            .bg(rgb(self.tokens.ui.bg))
                            .overflow_y_scroll()
                            .track_scroll(&self.version_migration.scroll_handle)
                            .vertical_scrollbar(&self.version_migration.scroll_handle)
                            .child(self.version_migration_page(compact, cx)),
                    )
                    .child(self.version_migration_footer(cx)),
            )
            .into_any_element()
    }

    fn version_migration_progress(&self, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(self.tokens.spacing.three))
            .px(px(if compact { 16.0 } else { 24.0 }))
            .py(px(12.0))
            .border_b_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_panel));

        if !compact {
            row = row.child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(self.tokens.spacing.two))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text_heading))
                            .child("OxideTerm"),
                    )
                    .child(
                        div()
                            .font_family(settings_mono_font_family(self.settings_store.settings()))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.accent))
                            .child("2.0"),
                    ),
            );
        }

        let mut steps = div()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(self.tokens.spacing.one));
        for index in 0..VERSION_MIGRATION_TOTAL_STEPS {
            let selected = index == self.version_migration.step;
            let completed = index < self.version_migration.step;
            let color = if selected {
                self.tokens.ui.accent_text
            } else if completed {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text_muted
            };
            if index > 0 {
                steps = steps.child(div().w(px(if compact { 6.0 } else { 12.0 })).h(px(1.0)).bg(
                    rgb(if index <= self.version_migration.step {
                        self.tokens.ui.accent
                    } else {
                        self.tokens.ui.border
                    }),
                ));
            }
            steps = steps.child(
                div()
                    .size(px(VERSION_MIGRATION_PROGRESS_STEP_SIZE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1()
                    .border_color(rgb(if selected || completed {
                        self.tokens.ui.accent
                    } else {
                        self.tokens.ui.border
                    }))
                    .bg(if selected {
                        rgb(self.tokens.ui.accent)
                    } else if completed {
                        rgba((self.tokens.ui.accent << 8) | 0x24)
                    } else {
                        rgb(self.tokens.ui.bg_card)
                    })
                    .cursor(CursorStyle::PointingHand)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.version_migration_go_to_step(index, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(if completed {
                        Self::render_lucide_icon(LucideIcon::Check, 14.0, rgb(color))
                            .into_any_element()
                    } else {
                        div()
                            .font_family(settings_mono_font_family(self.settings_store.settings()))
                            .text_size(px(11.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(color))
                            .child(format!("{:02}", index + 1))
                            .into_any_element()
                    }),
            );
        }
        row.child(steps).into_any_element()
    }

    fn version_migration_page(&self, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        match self.version_migration.step {
            0 => self.version_migration_overview_page(compact),
            1 => self.version_migration_cli_page(compact, cx),
            2 => self.version_migration_gpui_page(compact),
            3 => self.version_migration_visual_page(compact),
            4 => self.version_migration_features_page(compact),
            _ => self.version_migration_internal_page(compact),
        }
    }

    fn version_migration_page_shell(
        &self,
        compact: bool,
        eyebrow_key: &str,
        title_key: &str,
        description_key: &str,
        body: AnyElement,
    ) -> AnyElement {
        let intro = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .when(!compact, |intro| {
                intro.w(px(VERSION_MIGRATION_PAGE_RAIL_WIDTH)).flex_none()
            })
            .child(
                div()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.accent))
                    .child(self.i18n.t(eyebrow_key)),
            )
            .child(
                div()
                    .text_size(px(if compact { 22.0 } else { 26.0 }))
                    .line_height(px(if compact { 28.0 } else { 32.0 }))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text_heading))
                    .child(self.i18n.t(title_key)),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .line_height(px(self.tokens.metrics.ui_text_sm + 7.0))
                    .text_color(rgb(self.tokens.ui.text))
                    .opacity(0.72)
                    .child(self.i18n.t(description_key)),
            );

        div()
            .w_full()
            .px(px(if compact { 20.0 } else { 28.0 }))
            .py(px(if compact { 20.0 } else { 24.0 }))
            .flex()
            .when(compact, |page| page.flex_col())
            .when(!compact, |page| page.flex_row().items_start())
            .gap(px(if compact { 22.0 } else { 32.0 }))
            .child(intro)
            .child(div().flex_1().min_w(px(0.0)).child(body))
            .into_any_element()
    }

    fn version_migration_overview_page(&self, compact: bool) -> AnyElement {
        let items = [
            (LucideIcon::Gauge, "migration.overview_native"),
            (LucideIcon::Terminal, "migration.overview_cli"),
            (LucideIcon::Image, "migration.overview_visual"),
            (LucideIcon::Rocket, "migration.overview_features"),
        ];
        let body = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.three))
            .child(self.version_migration_feature_grid(&items, true))
            .child(self.version_migration_notice(
                LucideIcon::Archive,
                "migration.backup_notice",
                self.tokens.ui.success,
            ));
        self.version_migration_page_shell(
            compact,
            "migration.overview_eyebrow",
            "migration.overview_title",
            "migration.overview_description",
            body.into_any_element(),
        )
    }

    fn version_migration_cli_page(&self, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let status = self.settings_page.cli_companion_status.as_ref();
        let loading = self.settings_page.cli_companion_loading;
        let new_installed = status.is_some_and(|status| status.installed);
        let new_ready = status.is_some_and(|status| status.installed && !status.needs_reinstall);
        let legacy_installed = status.is_some_and(|status| status.legacy_installed);
        let bundled = status.is_some_and(|status| status.bundled);
        let migration_ready = new_ready && !legacy_installed;

        let commands = div()
            .flex()
            .flex_col()
            .border_t_1()
            .border_b_1()
            .border_color(rgb(self.tokens.ui.border))
            .child(self.version_migration_cli_status_item(
                CLI_COMPANION_COMMAND_NAME,
                status.and_then(|status| status.install_path.as_deref()),
                if loading {
                    self.i18n.t("settings_view.general.cli_checking")
                } else if new_ready {
                    self.i18n.t("migration.cli_installed")
                } else if new_installed {
                    self.i18n.t("migration.cli_reinstall_required")
                } else {
                    self.i18n.t("migration.cli_not_installed")
                },
                if new_ready {
                    StatusTone::Success
                } else if new_installed {
                    StatusTone::Warning
                } else {
                    StatusTone::Neutral
                },
            ))
            .child(self.version_migration_cli_status_item(
                LEGACY_CLI_COMPANION_COMMAND_NAME,
                status.and_then(|status| status.legacy_install_path.as_deref()),
                if legacy_installed {
                    self.i18n.t("migration.cli_legacy_found")
                } else {
                    self.i18n.t("migration.cli_legacy_absent")
                },
                if legacy_installed {
                    StatusTone::Warning
                } else {
                    StatusTone::Success
                },
            ));

        let body = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.three))
            .child(
                semantic_surface(
                    &self.tokens,
                    SurfaceOptions::new(SurfaceKind::Inspector).padding(SurfacePadding::Normal),
                )
                .flex()
                .flex_col()
                .gap(px(self.tokens.spacing.three))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .justify_between()
                        .gap(px(self.tokens.spacing.two))
                        .child(self.version_migration_command_change())
                        .child(status_pill(
                            &self.tokens,
                            if migration_ready {
                                self.i18n.t("migration.cli_ready")
                            } else {
                                self.i18n.t("migration.cli_action_required")
                            },
                            StatusPillOptions::new(if migration_ready {
                                StatusTone::Success
                            } else {
                                StatusTone::Warning
                            })
                            .strong(),
                        )),
                )
                .child(commands)
                .when_some(
                    self.settings_page.cli_companion_error.clone(),
                    |card, error| card.child(self.version_migration_error(error)),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap(px(self.tokens.spacing.two))
                        .when(legacy_installed && bundled, |row| {
                            row.child(self.version_migration_button(
                                self.i18n.t("migration.cli_migrate"),
                                LucideIcon::ArrowRight,
                                ButtonVariant::Default,
                                loading,
                                |this, cx| this.migrate_cli_companion(cx),
                                cx,
                            ))
                        })
                        .when(!legacy_installed && bundled && !new_ready, |row| {
                            row.child(self.version_migration_button(
                                if new_installed {
                                    self.i18n.t("migration.cli_reinstall_new")
                                } else {
                                    self.i18n.t("migration.cli_install_new")
                                },
                                LucideIcon::Download,
                                ButtonVariant::Default,
                                loading,
                                |this, cx| this.install_cli_companion(cx),
                                cx,
                            ))
                        })
                        .when(legacy_installed, |row| {
                            row.child(self.version_migration_button(
                                self.i18n.t("migration.cli_uninstall_legacy"),
                                LucideIcon::Trash2,
                                ButtonVariant::Ghost,
                                loading,
                                |this, cx| this.uninstall_legacy_cli_companion(cx),
                                cx,
                            ))
                        })
                        .when(
                            status.is_none() || self.settings_page.cli_companion_error.is_some(),
                            |row| {
                                row.child(self.version_migration_button(
                                    self.i18n.t("migration.cli_retry"),
                                    LucideIcon::RefreshCw,
                                    ButtonVariant::Outline,
                                    loading,
                                    |this, cx| this.refresh_cli_companion_status(cx),
                                    cx,
                                ))
                            },
                        ),
                )
                .when(!bundled && status.is_some(), |card| {
                    card.child(self.version_migration_notice(
                        LucideIcon::Info,
                        "migration.cli_not_bundled",
                        self.tokens.ui.warning,
                    ))
                }),
            )
            .child(self.version_migration_notice(
                LucideIcon::AlertTriangle,
                "migration.cli_script_notice",
                self.tokens.ui.warning,
            ));

        self.version_migration_page_shell(
            compact,
            "migration.cli_eyebrow",
            "migration.cli_page_title",
            "migration.cli_description",
            body.into_any_element(),
        )
    }

    fn version_migration_gpui_page(&self, compact: bool) -> AnyElement {
        let items = [
            (LucideIcon::Monitor, "migration.gpui_no_webview"),
            (LucideIcon::Gauge, "migration.gpui_memory"),
            (LucideIcon::Zap, "migration.gpui_direct_path"),
            (LucideIcon::Network, "migration.gpui_shared_runtime"),
        ];
        self.version_migration_page_shell(
            compact,
            "migration.gpui_eyebrow",
            "migration.gpui_title",
            "migration.gpui_description",
            self.version_migration_feature_grid(&items, compact),
        )
    }

    fn version_migration_visual_page(&self, compact: bool) -> AnyElement {
        let items = [
            (LucideIcon::Activity, "migration.visual_motion"),
            (LucideIcon::Settings, "migration.visual_motion_control"),
            (LucideIcon::Sparkles, "migration.visual_language"),
            (LucideIcon::Image, "migration.visual_background"),
        ];
        let body = div()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(self.version_migration_feature_grid(&items, compact))
            .child(self.version_migration_notice(
                LucideIcon::Settings,
                "migration.visual_disable_hint",
                self.tokens.ui.accent,
            ));
        self.version_migration_page_shell(
            compact,
            "migration.visual_eyebrow",
            "migration.visual_title",
            "migration.visual_description",
            body.into_any_element(),
        )
    }

    fn version_migration_features_page(&self, compact: bool) -> AnyElement {
        let items = [
            (LucideIcon::Terminal, "migration.features_telnet"),
            (LucideIcon::Monitor, "migration.features_remote"),
            (LucideIcon::Activity, "migration.features_host_tools"),
            (LucideIcon::FolderOpen, "migration.features_sftp"),
            (LucideIcon::RefreshCw, "migration.features_reconnect"),
            (LucideIcon::FileCode, "migration.features_editor"),
        ];
        self.version_migration_page_shell(
            compact,
            "migration.features_eyebrow",
            "migration.features_title",
            "migration.features_description",
            self.version_migration_feature_grid(&items, compact),
        )
    }

    fn version_migration_internal_page(&self, compact: bool) -> AnyElement {
        let items = [
            (LucideIcon::Network, "migration.internal_ownership"),
            (LucideIcon::ShieldCheck, "migration.internal_security"),
            (LucideIcon::Puzzle, "migration.internal_plugins"),
            (LucideIcon::Cloud, "migration.internal_sync"),
            (LucideIcon::Download, "migration.internal_updates"),
            (LucideIcon::Cpu, "migration.internal_platform"),
        ];
        let body = div()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(self.version_migration_feature_grid(&items, compact))
            .child(self.version_migration_notice(
                LucideIcon::CheckCircle,
                "migration.internal_finish_notice",
                self.tokens.ui.success,
            ));
        self.version_migration_page_shell(
            compact,
            "migration.internal_eyebrow",
            "migration.internal_title",
            "migration.internal_description",
            body.into_any_element(),
        )
    }

    fn version_migration_feature_grid(
        &self,
        items: &[(LucideIcon, &str)],
        compact: bool,
    ) -> AnyElement {
        let mut grid = div()
            .grid()
            .grid_cols(if compact { 1 } else { 2 })
            .gap_x(px(self.tokens.spacing.three * 2.0))
            .gap_y(px(0.0));
        for (icon, key) in items {
            grid = grid.child(self.version_migration_feature_card(*icon, key));
        }
        grid.into_any_element()
    }

    fn version_migration_feature_card(&self, icon: LucideIcon, key: &str) -> AnyElement {
        div()
            .min_h(px(82.0))
            .flex()
            .items_start()
            .gap(px(self.tokens.spacing.three))
            .py(px(14.0))
            .border_t_1()
            .border_color(rgb(self.tokens.ui.border))
            .child(
                div()
                    .size(px(28.0))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1()
                    .border_color(rgba((self.tokens.ui.accent << 8) | 0x66))
                    .child(Self::render_lucide_icon(
                        icon,
                        14.0,
                        rgb(self.tokens.ui.accent),
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.spacing.one))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(&format!("{key}_title"))),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .line_height(px(self.tokens.metrics.ui_text_xs + 5.0))
                            .text_color(rgb(self.tokens.ui.text))
                            .opacity(0.68)
                            .child(self.i18n.t(&format!("{key}_description"))),
                    ),
            )
            .into_any_element()
    }

    fn version_migration_notice(&self, icon: LucideIcon, key: &str, color: u32) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_start()
            .gap(px(self.tokens.spacing.three))
            .p(px(12.0))
            .rounded(px(self.tokens.radii.sm))
            .border_l_2()
            .border_color(rgb(color))
            .bg(rgba((color << 8) | 0x10))
            .child(Self::render_lucide_icon(icon, 15.0, rgb(color)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(self.tokens.metrics.ui_text_xs + 5.0))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(key)),
            )
            .into_any_element()
    }

    fn version_migration_command_change(&self) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(9.0))
            .child(self.version_migration_command_label(
                LEGACY_CLI_COMPANION_COMMAND_NAME,
                self.tokens.ui.error,
            ))
            .child(Self::render_lucide_icon(
                LucideIcon::ArrowRight,
                16.0,
                rgb(self.tokens.ui.text_muted),
            ))
            .child(self.version_migration_command_label(
                CLI_COMPANION_COMMAND_NAME,
                self.tokens.ui.success,
            ))
            .into_any_element()
    }

    fn version_migration_command_label(&self, command: &'static str, color: u32) -> AnyElement {
        div()
            .px(px(9.0))
            .py(px(5.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((color << 8) | 0x66))
            .bg(rgba((color << 8) | 0x12))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(command)
            .into_any_element()
    }

    fn version_migration_cli_status_item(
        &self,
        command: &'static str,
        path: Option<&str>,
        status_label: String,
        tone: StatusTone,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(self.tokens.spacing.three))
            .py(px(10.0))
            .child(
                div()
                    .w(px(VERSION_MIGRATION_CLI_COMMAND_WIDTH))
                    .flex_none()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(command),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(VERSION_MIGRATION_CLI_PATH_MIN_WIDTH))
                    .truncate()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(11.0))
                    .text_color(rgb(self.tokens.ui.text))
                    .opacity(if path.is_some() { 0.62 } else { 0.0 })
                    // Preserve the path column while status is still loading.
                    .child(path.unwrap_or(" ").to_string()),
            )
            .child(status_pill(
                &self.tokens,
                status_label,
                StatusPillOptions::new(tone).compact(),
            ))
            .into_any_element()
    }

    fn version_migration_error(&self, error: String) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.error))
            .child(Self::render_lucide_icon(
                LucideIcon::AlertTriangle,
                14.0,
                rgb(self.tokens.ui.error),
            ))
            .child(div().flex_1().min_w(px(0.0)).child(error))
            .into_any_element()
    }

    fn version_migration_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let first = self.version_migration.step == 0;
        let last = self.version_migration.step + 1 == VERSION_MIGRATION_TOTAL_STEPS;
        oxideterm_gpui_ui::modal_footer(&self.tokens)
            .h_auto()
            .min_h(px(self.tokens.metrics.modal_footer_height))
            .px(px(24.0))
            .py(px(12.0))
            .flex_wrap()
            .justify_between()
            .child(if first {
                self.version_migration_button(
                    self.i18n.t("migration.skip_tour"),
                    LucideIcon::X,
                    ButtonVariant::Ghost,
                    false,
                    |this, cx| this.complete_version_migration_notice(cx),
                    cx,
                )
            } else {
                self.version_migration_button(
                    self.i18n.t("migration.back"),
                    LucideIcon::ChevronLeft,
                    ButtonVariant::Ghost,
                    false,
                    |this, cx| this.version_migration_back(cx),
                    cx,
                )
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .when_some(self.version_migration.error.clone(), |row, error| {
                        row.child(
                            div()
                                .max_w(px(360.0))
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.error))
                                .child(error),
                        )
                    })
                    .child(self.version_migration_button(
                        if last {
                            self.i18n.t("migration.continue_button")
                        } else {
                            self.i18n.t("migration.next")
                        },
                        if last {
                            LucideIcon::Check
                        } else {
                            LucideIcon::ChevronRight
                        },
                        ButtonVariant::Default,
                        false,
                        move |this, cx| {
                            if last {
                                this.complete_version_migration_notice(cx);
                            } else {
                                this.version_migration_next(cx);
                            }
                        },
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn version_migration_button(
        &self,
        label: String,
        icon: LucideIcon,
        variant: ButtonVariant,
        loading: bool,
        listener: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let icon_color = if variant == ButtonVariant::Default {
            self.tokens.ui.accent_text
        } else {
            self.tokens.ui.text
        };
        button_with(
            &self.tokens,
            label,
            ButtonOptions {
                variant,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled: loading,
            },
        )
        .gap(px(6.0))
        .child(Self::render_lucide_icon(
            if loading { LucideIcon::RefreshCw } else { icon },
            14.0,
            rgb(icon_color),
        ))
        .opacity(if loading { 0.55 } else { 1.0 })
        .cursor(if loading {
            CursorStyle::OperationNotAllowed
        } else {
            CursorStyle::PointingHand
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                if !loading {
                    listener(this, cx);
                }
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }
}
