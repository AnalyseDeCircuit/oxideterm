fn tab_background_key(kind: &TabKind) -> &'static str {
    match kind {
        TabKind::LocalTerminal => "local_terminal",
        TabKind::SshTerminal => "terminal",
        TabKind::Sftp => "sftp",
        TabKind::Forwards => "forwards",
        TabKind::SessionManager => "session_manager",
        TabKind::Settings => "settings",
    }
}

fn terminal_background_fit(fit: BackgroundFit) -> TerminalBackgroundFit {
    match fit {
        BackgroundFit::Cover => TerminalBackgroundFit::Cover,
        BackgroundFit::Contain => TerminalBackgroundFit::Contain,
        BackgroundFit::Fill => TerminalBackgroundFit::Fill,
        BackgroundFit::Tile => TerminalBackgroundFit::Tile,
    }
}

fn sftp_runtime_settings_from_settings(
    settings: &PersistedSettings,
) -> SftpTransferRuntimeSettings {
    SftpTransferRuntimeSettings {
        max_concurrent_transfers: settings.sftp.max_concurrent_transfers.max(1) as usize,
        speed_limit_kbps: if settings.sftp.speed_limit_enabled {
            settings.sftp.speed_limit_kbps.max(0) as usize
        } else {
            0
        },
        directory_parallelism: settings.sftp.directory_parallelism.max(1) as usize,
    }
}

fn session_terminal_encoding(encoding: SettingsTerminalEncoding) -> SessionTerminalEncoding {
    match encoding {
        SettingsTerminalEncoding::Utf8 => SessionTerminalEncoding::Utf8,
        SettingsTerminalEncoding::Gbk => SessionTerminalEncoding::Gbk,
        SettingsTerminalEncoding::Gb18030 => SessionTerminalEncoding::Gb18030,
        SettingsTerminalEncoding::Big5 => SessionTerminalEncoding::Big5,
        SettingsTerminalEncoding::ShiftJis => SessionTerminalEncoding::ShiftJis,
        SettingsTerminalEncoding::EucJp => SessionTerminalEncoding::EucJp,
        SettingsTerminalEncoding::EucKr => SessionTerminalEncoding::EucKr,
        SettingsTerminalEncoding::Windows1252 => SessionTerminalEncoding::Windows1252,
    }
}

fn locale_from_settings(language: Language) -> Locale {
    match language {
        Language::De => Locale::De,
        Language::En => Locale::En,
        Language::EsEs => Locale::EsEs,
        Language::FrFr => Locale::FrFr,
        Language::It => Locale::It,
        Language::Ja => Locale::Ja,
        Language::Ko => Locale::Ko,
        Language::PtBr => Locale::PtBr,
        Language::Vi => Locale::Vi,
        Language::ZhCn => Locale::ZhCn,
        Language::ZhTw => Locale::ZhTw,
    }
}

fn settings_language_from_locale(locale: Locale) -> Language {
    match locale {
        Locale::De => Language::De,
        Locale::En => Language::En,
        Locale::EsEs => Language::EsEs,
        Locale::FrFr => Language::FrFr,
        Locale::It => Language::It,
        Locale::Ja => Language::Ja,
        Locale::Ko => Language::Ko,
        Locale::PtBr => Language::PtBr,
        Locale::Vi => Language::Vi,
        Locale::ZhCn => Language::ZhCn,
        Locale::ZhTw => Language::ZhTw,
    }
}

fn tokens_from_settings(settings: &PersistedSettings) -> ThemeTokens {
    let mut tokens = ThemeTokens::from_builtin(theme_by_id(&settings.terminal.theme));
    let radius = settings.appearance.border_radius as f32;
    tokens.radii = UiRadii {
        xs: (radius - 4.0).max(0.0),
        sm: (radius - 2.0).max(0.0),
        md: radius,
        lg: radius + 4.0,
        active_indicator: 2.0_f32.min(radius.max(1.0)),
    };
    tokens
}

fn native_vibrancy_mode(mode: FrostedGlassMode) -> NativeVibrancyMode {
    match mode {
        FrostedGlassMode::Off | FrostedGlassMode::Css => NativeVibrancyMode::Off,
        FrostedGlassMode::Native | FrostedGlassMode::System => NativeVibrancyMode::System,
        FrostedGlassMode::Mica => NativeVibrancyMode::Mica,
        FrostedGlassMode::Acrylic => NativeVibrancyMode::Acrylic,
    }
}

fn effective_vibrancy_mode(
    settings: &PersistedSettings,
    policy: &EffectiveRenderPolicy,
) -> NativeVibrancyMode {
    if policy.allow_vibrancy {
        native_vibrancy_mode(settings.appearance.frosted_glass)
    } else {
        NativeVibrancyMode::Off
    }
}

fn render_profile_from_env() -> Option<RenderProfile> {
    let value = std::env::var("OXIDETERM_RENDER_PROFILE").ok()?;
    let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "auto" => Some(RenderProfile::Auto),
        "quality" | "high-quality" | "high" => Some(RenderProfile::Quality),
        "low-power" | "lowpower" | "low" => Some(RenderProfile::LowPower),
        "compatibility" | "compat" | "safe" | "safe-mode" => Some(RenderProfile::Compatibility),
        _ => None,
    }
}

fn workspace_background(tokens: &ThemeTokens, mode: NativeVibrancyMode) -> Rgba {
    match mode {
        NativeVibrancyMode::Off => rgb(tokens.ui.bg),
        NativeVibrancyMode::System | NativeVibrancyMode::Mica | NativeVibrancyMode::Acrylic => {
            rgba((tokens.ui.bg << 8) | alpha_byte(tokens.metrics.window_vibrancy_tint_alpha))
        }
    }
}

fn alpha_byte(alpha: f32) -> u32 {
    (alpha.clamp(0.0, 1.0) * 255.0).round() as u32
}

fn settings_mono_font_family(settings: &PersistedSettings) -> SharedString {
    SharedString::from(
        settings
            .terminal
            .font_family
            .terminal_family_name(&settings.terminal.custom_font_family),
    )
}
