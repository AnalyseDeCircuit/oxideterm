// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Startup coordination for Linux WebView compatibility fallbacks.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::State;

#[cfg(target_os = "linux")]
use std::ffi::OsString;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const LINUX_WEBVIEW_PROFILE_ENV: &str = "OXIDETERM_LINUX_WEBVIEW_PROFILE";
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const LINUX_WEBVIEW_FALLBACK_GUARD_ENV: &str = "OXIDETERM_INTERNAL_LINUX_WEBVIEW_SAFE_RELAUNCH";
const FRONTEND_READY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxWebviewProfile {
    Accelerated,
    Safe,
}

impl LinuxWebviewProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accelerated => "accelerated",
            Self::Safe => "safe",
        }
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "accelerated" => Some(Self::Accelerated),
            "safe" => Some(Self::Safe),
            _ => None,
        }
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinuxWebviewProfileSource {
    EnvOverride,
    PersistedBootstrap,
    Heuristic,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LinuxWebviewContext {
    env_override: Option<LinuxWebviewProfile>,
    persisted_profile: Option<LinuxWebviewProfile>,
    is_appimage: bool,
    is_wayland: bool,
    fallback_guard_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinuxWebviewDecision {
    profile: LinuxWebviewProfile,
    source: LinuxWebviewProfileSource,
    is_appimage: bool,
    is_wayland: bool,
    fallback_guard_active: bool,
}

impl LinuxWebviewDecision {
    pub fn profile(self) -> LinuxWebviewProfile {
        self.profile
    }

    pub fn can_auto_relaunch(self) -> bool {
        self.profile == LinuxWebviewProfile::Accelerated
            && self.source != LinuxWebviewProfileSource::EnvOverride
            && !self.fallback_guard_active
    }

    pub fn should_persist_on_ready(self) -> bool {
        self.source != LinuxWebviewProfileSource::EnvOverride || self.fallback_guard_active
    }

    fn session_type(self) -> &'static str {
        if self.is_wayland {
            "wayland"
        } else {
            "x11-or-unknown"
        }
    }
}

#[derive(Clone)]
pub struct LinuxStartupRecoveryState {
    inner: Arc<LinuxStartupRecoveryInner>,
}

struct LinuxStartupRecoveryInner {
    decision: Option<LinuxWebviewDecision>,
    frontend_ready: AtomicBool,
}

impl LinuxStartupRecoveryState {
    pub fn disabled() -> Self {
        Self {
            inner: Arc::new(LinuxStartupRecoveryInner {
                decision: None,
                frontend_ready: AtomicBool::new(false),
            }),
        }
    }

    pub fn enabled(decision: LinuxWebviewDecision) -> Self {
        Self {
            inner: Arc::new(LinuxStartupRecoveryInner {
                decision: Some(decision),
                frontend_ready: AtomicBool::new(false),
            }),
        }
    }

    pub fn spawn_frontend_ready_watchdog(&self) {
        let Some(decision) = self.inner.decision else {
            return;
        };
        if !decision.can_auto_relaunch() {
            tracing::info!(
                "Linux startup watchdog disabled: profile={}, source={:?}, guard_active={}",
                decision.profile().as_str(),
                decision.source,
                decision.fallback_guard_active
            );
            return;
        }

        let state = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(FRONTEND_READY_TIMEOUT).await;
            if state.inner.frontend_ready.load(Ordering::SeqCst) {
                tracing::info!(
                    "Linux startup watchdog satisfied within {:?} for profile={}",
                    FRONTEND_READY_TIMEOUT,
                    decision.profile().as_str()
                );
                return;
            }

            tracing::warn!(
                "Frontend did not report ready within {:?}; relaunching in safe Linux WebView mode",
                FRONTEND_READY_TIMEOUT
            );
            match relaunch_in_safe_mode() {
                Ok(()) => std::process::exit(0),
                Err(err) => {
                    tracing::error!(
                        "Failed to relaunch OxideTerm in safe Linux WebView mode: {}",
                        err
                    );
                }
            }
        });
    }

    pub fn diagnostics_summary(&self) -> Option<String> {
        self.inner.decision.map(|decision| {
            format!(
                "Linux WebView startup profile: profile={}, source={:?}, session={}, appimage={}, auto_relaunch={}",
                decision.profile().as_str(),
                decision.source,
                decision.session_type(),
                decision.is_appimage,
                decision.can_auto_relaunch()
            )
        })
    }

    async fn mark_frontend_ready(&self) -> Result<(), String> {
        let already_ready = self.inner.frontend_ready.swap(true, Ordering::SeqCst);
        if already_ready {
            return Ok(());
        }

        let Some(decision) = self.inner.decision else {
            return Ok(());
        };

        tracing::info!(
            "Frontend reported ready: profile={}, source={:?}, session={}, appimage={}",
            decision.profile().as_str(),
            decision.source,
            decision.session_type(),
            decision.is_appimage
        );

        if !decision.should_persist_on_ready() {
            return Ok(());
        }

        let profile = decision.profile();
        tauri::async_runtime::spawn_blocking(move || persist_last_known_good_profile(profile))
            .await
            .map_err(|err| err.to_string())??;

        Ok(())
    }
}

#[tauri::command]
pub async fn frontend_ready(state: State<'_, LinuxStartupRecoveryState>) -> Result<(), String> {
    state.mark_frontend_ready().await
}

#[tauri::command]
pub fn get_linux_webview_profile(
    state: State<'_, LinuxStartupRecoveryState>,
) -> Result<Option<String>, String> {
    Ok(state
        .inner
        .decision
        .map(|decision| decision.profile().as_str().to_string()))
}

pub fn configure_linux_startup_recovery() -> LinuxStartupRecoveryState {
    #[cfg(target_os = "linux")]
    {
        let decision = determine_linux_webview_decision();
        apply_linux_webview_profile(decision.profile());
        tracing::info!(
            "Linux WebView startup profile selected: profile={}, source={:?}, session={}, appimage={}, auto_relaunch={}",
            decision.profile().as_str(),
            decision.source,
            decision.session_type(),
            decision.is_appimage,
            decision.can_auto_relaunch()
        );
        return LinuxStartupRecoveryState::enabled(decision);
    }

    #[cfg(not(target_os = "linux"))]
    {
        LinuxStartupRecoveryState::disabled()
    }
}

#[cfg(target_os = "linux")]
fn determine_linux_webview_decision() -> LinuxWebviewDecision {
    let persisted_profile = crate::config::load_bootstrap_config().and_then(|config| {
        config
            .linux_webview_profile()
            .and_then(LinuxWebviewProfile::parse)
    });
    let context = linux_webview_context_from(current_env_var, persisted_profile);

    if let Some(profile) = context.env_override {
        return LinuxWebviewDecision {
            profile,
            source: LinuxWebviewProfileSource::EnvOverride,
            is_appimage: context.is_appimage,
            is_wayland: context.is_wayland,
            fallback_guard_active: context.fallback_guard_active,
        };
    }

    if let Some(profile) = context.persisted_profile {
        return LinuxWebviewDecision {
            profile,
            source: LinuxWebviewProfileSource::PersistedBootstrap,
            is_appimage: context.is_appimage,
            is_wayland: context.is_wayland,
            fallback_guard_active: context.fallback_guard_active,
        };
    }

    LinuxWebviewDecision {
        profile: if context.is_appimage && context.is_wayland {
            LinuxWebviewProfile::Safe
        } else {
            LinuxWebviewProfile::Accelerated
        },
        source: LinuxWebviewProfileSource::Heuristic,
        is_appimage: context.is_appimage,
        is_wayland: context.is_wayland,
        fallback_guard_active: context.fallback_guard_active,
    }
}

#[cfg(target_os = "linux")]
fn linux_webview_context_from<F>(
    get_var: F,
    persisted_profile: Option<LinuxWebviewProfile>,
) -> LinuxWebviewContext
where
    F: Fn(&str) -> Option<String>,
{
    let env_override = get_var(LINUX_WEBVIEW_PROFILE_ENV).and_then(|value| {
        let parsed = LinuxWebviewProfile::parse(&value);
        if parsed.is_none() {
            tracing::warn!(
                "Ignoring invalid {} value: {}",
                LINUX_WEBVIEW_PROFILE_ENV,
                value
            );
        }
        parsed
    });

    let session_type = get_var("XDG_SESSION_TYPE")
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let is_wayland = session_type == "wayland" || get_var("WAYLAND_DISPLAY").is_some();
    let is_appimage = get_var("APPIMAGE").is_some() || get_var("APPDIR").is_some();
    let fallback_guard_active = get_var(LINUX_WEBVIEW_FALLBACK_GUARD_ENV).is_some();

    LinuxWebviewContext {
        env_override,
        persisted_profile,
        is_appimage,
        is_wayland,
        fallback_guard_active,
    }
}

#[cfg(target_os = "linux")]
fn apply_linux_webview_profile(profile: LinuxWebviewProfile) {
    if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
        unsafe {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    if profile == LinuxWebviewProfile::Safe
        && std::env::var("WEBKIT_DISABLE_COMPOSITING_MODE").is_err()
    {
        unsafe {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        }
    }
}

fn persist_last_known_good_profile(profile: LinuxWebviewProfile) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let mut bootstrap = crate::config::load_bootstrap_config().unwrap_or_default();
        bootstrap.set_linux_webview_profile(Some(profile.as_str().to_string()));
        return crate::config::save_bootstrap_config(&bootstrap).map_err(|err| err.to_string());
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = profile;
        Ok(())
    }
}

fn relaunch_in_safe_mode() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let executable = relaunch_executable_path()?;
        let args: Vec<OsString> = std::env::args_os().skip(1).collect();
        let current_dir = std::env::current_dir().map_err(|err| err.to_string())?;

        let mut command = std::process::Command::new(&executable);
        command
            .args(args)
            .current_dir(current_dir)
            .env(
                LINUX_WEBVIEW_PROFILE_ENV,
                LinuxWebviewProfile::Safe.as_str(),
            )
            .env(LINUX_WEBVIEW_FALLBACK_GUARD_ENV, "1");

        command
            .spawn()
            .map_err(|err| format!("failed to spawn {:?}: {}", executable, err))?;
        return Ok(());
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err("Linux WebView fallback relaunch is only supported on Linux".to_string())
    }
}

#[cfg(target_os = "linux")]
fn relaunch_executable_path() -> Result<PathBuf, String> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        let appimage_path = PathBuf::from(appimage);
        if appimage_path.is_file() {
            return Ok(appimage_path);
        }
    }

    std::env::current_exe().map_err(|err| err.to_string())
}

#[cfg(target_os = "linux")]
fn current_env_var(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn context_from_entries(
        entries: &[(&str, &str)],
        persisted_profile: Option<LinuxWebviewProfile>,
    ) -> LinuxWebviewContext {
        let env_map = entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect::<HashMap<_, _>>();
        linux_webview_context_from(|key| env_map.get(key).cloned(), persisted_profile)
    }

    #[test]
    fn appimage_wayland_defaults_to_safe() {
        let context = context_from_entries(
            &[
                ("APPIMAGE", "/tmp/OxideTerm.AppImage"),
                ("XDG_SESSION_TYPE", "wayland"),
            ],
            None,
        );
        let decision = if let Some(profile) = context.env_override {
            LinuxWebviewDecision {
                profile,
                source: LinuxWebviewProfileSource::EnvOverride,
                is_appimage: context.is_appimage,
                is_wayland: context.is_wayland,
                fallback_guard_active: context.fallback_guard_active,
            }
        } else {
            LinuxWebviewDecision {
                profile: if context.is_appimage && context.is_wayland {
                    LinuxWebviewProfile::Safe
                } else {
                    LinuxWebviewProfile::Accelerated
                },
                source: LinuxWebviewProfileSource::Heuristic,
                is_appimage: context.is_appimage,
                is_wayland: context.is_wayland,
                fallback_guard_active: context.fallback_guard_active,
            }
        };

        assert_eq!(decision.profile(), LinuxWebviewProfile::Safe);
        assert_eq!(decision.source, LinuxWebviewProfileSource::Heuristic);
    }

    #[test]
    fn regular_linux_defaults_to_accelerated() {
        let context = context_from_entries(&[("XDG_SESSION_TYPE", "x11")], None);
        assert!(!context.is_appimage);
        assert!(!context.is_wayland);

        let decision = LinuxWebviewDecision {
            profile: LinuxWebviewProfile::Accelerated,
            source: LinuxWebviewProfileSource::Heuristic,
            is_appimage: context.is_appimage,
            is_wayland: context.is_wayland,
            fallback_guard_active: context.fallback_guard_active,
        };

        assert_eq!(decision.profile(), LinuxWebviewProfile::Accelerated);
        assert!(decision.can_auto_relaunch());
    }

    #[test]
    fn env_override_wins_over_persisted_profile() {
        let context = context_from_entries(
            &[(LINUX_WEBVIEW_PROFILE_ENV, "safe")],
            Some(LinuxWebviewProfile::Accelerated),
        );

        assert_eq!(context.env_override, Some(LinuxWebviewProfile::Safe));
        assert_eq!(
            context.persisted_profile,
            Some(LinuxWebviewProfile::Accelerated)
        );
    }

    #[test]
    fn persisted_profile_wins_over_heuristic() {
        let context = context_from_entries(
            &[
                ("APPIMAGE", "/tmp/OxideTerm.AppImage"),
                ("XDG_SESSION_TYPE", "wayland"),
            ],
            Some(LinuxWebviewProfile::Accelerated),
        );

        let decision = LinuxWebviewDecision {
            profile: context.persisted_profile.unwrap(),
            source: LinuxWebviewProfileSource::PersistedBootstrap,
            is_appimage: context.is_appimage,
            is_wayland: context.is_wayland,
            fallback_guard_active: context.fallback_guard_active,
        };

        assert_eq!(decision.profile(), LinuxWebviewProfile::Accelerated);
    }

    #[test]
    fn fallback_guard_disables_second_relaunch() {
        let decision = LinuxWebviewDecision {
            profile: LinuxWebviewProfile::Accelerated,
            source: LinuxWebviewProfileSource::Heuristic,
            is_appimage: false,
            is_wayland: true,
            fallback_guard_active: true,
        };

        assert!(!decision.can_auto_relaunch());
    }

    #[test]
    fn safe_relaunch_session_persists_on_ready() {
        let decision = LinuxWebviewDecision {
            profile: LinuxWebviewProfile::Safe,
            source: LinuxWebviewProfileSource::EnvOverride,
            is_appimage: true,
            is_wayland: true,
            fallback_guard_active: true,
        };

        assert!(decision.should_persist_on_ready());
    }
}
