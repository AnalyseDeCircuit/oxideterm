use gpui::{Window, WindowBackgroundAppearance};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeVibrancyMode {
    Off,
    System,
    Mica,
    Acrylic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum VibrancySupport {
    Supported,
    Fallback { reason: &'static str },
    Unsupported { reason: &'static str },
}

pub fn available_modes() -> &'static [NativeVibrancyMode] {
    imp::AVAILABLE_MODES
}

pub fn apply_window_vibrancy(window: &mut Window, mode: NativeVibrancyMode) -> VibrancySupport {
    imp::apply_window_vibrancy(window, mode)
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    pub(super) const AVAILABLE_MODES: &[NativeVibrancyMode] =
        &[NativeVibrancyMode::Off, NativeVibrancyMode::System];

    pub(super) fn apply_window_vibrancy(
        window: &mut Window,
        mode: NativeVibrancyMode,
    ) -> VibrancySupport {
        window.set_background_appearance(match mode {
            NativeVibrancyMode::Off => WindowBackgroundAppearance::Opaque,
            NativeVibrancyMode::System | NativeVibrancyMode::Mica | NativeVibrancyMode::Acrylic => {
                WindowBackgroundAppearance::Blurred
            }
        });
        VibrancySupport::Supported
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use std::{ffi::c_void, mem};

    use raw_window_handle::RawWindowHandle;
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Dwm::{DWMWINDOWATTRIBUTE, DwmSetWindowAttribute},
    };

    use super::*;

    const DWMWA_SYSTEMBACKDROP_TYPE: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(38);
    const DWMSBT_AUTO: i32 = 0;
    const DWMSBT_MAINWINDOW: i32 = 2;
    const DWMSBT_TRANSIENTWINDOW: i32 = 3;

    pub(super) const AVAILABLE_MODES: &[NativeVibrancyMode] = &[
        NativeVibrancyMode::Off,
        NativeVibrancyMode::System,
        NativeVibrancyMode::Mica,
        NativeVibrancyMode::Acrylic,
    ];

    pub(super) fn apply_window_vibrancy(
        window: &mut Window,
        mode: NativeVibrancyMode,
    ) -> VibrancySupport {
        match mode {
            NativeVibrancyMode::Off => {
                let _ = set_system_backdrop(window, DWMSBT_AUTO);
                window.set_background_appearance(WindowBackgroundAppearance::Opaque);
                VibrancySupport::Supported
            }
            NativeVibrancyMode::Acrylic => {
                if set_system_backdrop(window, DWMSBT_TRANSIENTWINDOW).is_err() {
                    window.set_background_appearance(WindowBackgroundAppearance::Blurred);
                    VibrancySupport::Fallback {
                        reason: "DWM acrylic backdrop is unavailable",
                    }
                } else {
                    window.set_background_appearance(WindowBackgroundAppearance::Transparent);
                    VibrancySupport::Supported
                }
            }
            NativeVibrancyMode::System | NativeVibrancyMode::Mica => {
                let backdrop = match mode {
                    NativeVibrancyMode::System => DWMSBT_MAINWINDOW,
                    NativeVibrancyMode::Mica => DWMSBT_MAINWINDOW,
                    _ => unreachable!(),
                };
                match set_system_backdrop(window, backdrop) {
                    Ok(()) => {
                        window.set_background_appearance(WindowBackgroundAppearance::Transparent);
                        VibrancySupport::Supported
                    }
                    Err(reason) => {
                        window.set_background_appearance(WindowBackgroundAppearance::Opaque);
                        VibrancySupport::Unsupported { reason }
                    }
                }
            }
        }
    }

    fn set_system_backdrop(window: &Window, backdrop: i32) -> Result<(), &'static str> {
        // GPUI's Window has its own window_handle() → AnyWindowHandle, which
        // shadows the raw-window-handle trait.  Use UFCS to call the trait.
        let handle = raw_window_handle::HasWindowHandle::window_handle(window)
            .map_err(|_| "window handle is unavailable")?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err("window is not a Win32 window");
        };
        let hwnd = HWND(handle.hwnd.get() as *mut c_void);
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop as *const i32 as *const c_void,
                mem::size_of::<i32>() as u32,
            )
            .map_err(|_| "DWM system backdrop is unavailable")
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    use super::*;

    pub(super) const AVAILABLE_MODES: &[NativeVibrancyMode] = &[NativeVibrancyMode::Off];

    pub(super) fn apply_window_vibrancy(
        window: &mut Window,
        mode: NativeVibrancyMode,
    ) -> VibrancySupport {
        window.set_background_appearance(WindowBackgroundAppearance::Opaque);
        match mode {
            NativeVibrancyMode::Off => VibrancySupport::Supported,
            _ => VibrancySupport::Unsupported {
                reason: "native vibrancy is unsupported on this platform",
            },
        }
    }
}
