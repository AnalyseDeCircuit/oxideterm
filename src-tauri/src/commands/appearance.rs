//! Appearance commands for window vibrancy/transparency effects.

/// Apply or remove native window vibrancy effect.
///
/// - `mode`: `"native"` to enable, `"off"` or `"css"` to disable
///
/// On macOS: uses NSVisualEffectView with `.sidebar` material.
/// On Windows: uses Mica effect (Windows 11) or Acrylic fallback.
/// On Linux: no-op (not supported natively).
///
/// `apply_vibrancy` / `apply_mica` must be called from the main UI thread.
/// We dispatch via `run_on_main_thread` and bridge the result back through a
/// oneshot channel so the async Tauri command can await it.
#[tauri::command]
pub async fn set_window_vibrancy(
    window: tauri::Window,
    mode: String,
) -> Result<(), String> {
    let enabled = mode == "native";

    let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();

    let window_inner = window.clone();
    window
        .run_on_main_thread(move || {
            let result: Result<(), String> = (|| {
                #[cfg(target_os = "macos")]
                if enabled {
                    use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
                    apply_vibrancy(&window_inner, NSVisualEffectMaterial::Sidebar, None, None)
                        .map_err(|e| format!("Failed to apply vibrancy: {e}"))?;
                }

                #[cfg(target_os = "windows")]
                if enabled {
                    // Try Mica first (Windows 11), fall back to Acrylic
                    use window_vibrancy::apply_mica;
                    apply_mica(&window_inner, None)
                        .or_else(|_| {
                            use window_vibrancy::apply_acrylic;
                            apply_acrylic(&window_inner, None)
                        })
                        .map_err(|e| format!("Failed to apply vibrancy: {e}"))?;
                }

                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                {
                    let _ = &window_inner;
                    if enabled {
                        return Err("Native vibrancy is not supported on this platform".into());
                    }
                }

                Ok(())
            })();
            let _ = tx.send(result);
        })
        .map_err(|e| e.to_string())?;

    rx.await.map_err(|_| "Vibrancy task was cancelled".to_string())?
}
