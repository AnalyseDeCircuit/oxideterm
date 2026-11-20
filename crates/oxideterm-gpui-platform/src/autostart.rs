use std::io;

#[cfg(any(target_os = "windows", target_os = "linux"))]
use std::path::PathBuf;

#[cfg(any(target_os = "windows", target_os = "linux"))]
const APP_NAME: &str = "OxideTerm";
#[cfg(target_os = "linux")]
const LINUX_DESKTOP_ID: &str = "com.oxideterm.app";

/// Returns whether the current user's startup registration points at this executable.
pub fn is_enabled() -> io::Result<bool> {
    platform::is_enabled()
}

/// Adds or removes the current user's startup registration where supported.
///
/// On macOS, OxideTerm delegates registration to Login Items in System Settings.
pub fn set_enabled(enabled: bool) -> io::Result<()> {
    platform::set_enabled(enabled)
}

/// Opens the macOS Login Items settings owned by the operating system.
#[cfg(target_os = "macos")]
pub fn open_login_items_settings() -> io::Result<()> {
    platform::open_login_items_settings()
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn current_executable() -> io::Result<PathBuf> {
    #[cfg(target_os = "linux")]
    if let Some(app_image) = std::env::var_os("APPIMAGE") {
        let path = PathBuf::from(app_image);
        // AppImage's current_exe points into a temporary mount that disappears
        // after exit, so retain the stable outer image path when it is valid.
        if path.is_absolute() && path.is_file() {
            return Ok(path);
        }
    }

    std::env::current_exe()
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(target_os = "linux")]
fn home_directory() -> io::Result<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not available"))
}

#[cfg(target_os = "linux")]
fn registration_matches(path: &std::path::Path, expected: &str) -> io::Result<bool> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(contents == expected),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "linux")]
fn write_registration(path: &std::path::Path, contents: &str) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid_data("startup registration has no parent directory"))?;
    std::fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid_data("startup registration filename is not valid UTF-8"))?;
    let temporary = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));
    std::fs::write(&temporary, contents)?;
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;

    fn registration_path() -> io::Result<PathBuf> {
        let config_directory = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or(home_directory()?.join(".config"));
        Ok(config_directory
            .join("autostart")
            .join(format!("{LINUX_DESKTOP_ID}.desktop")))
    }

    fn desktop_exec_argument(path: &std::path::Path) -> io::Result<String> {
        let value = path
            .to_str()
            .ok_or_else(|| invalid_data("executable path is not valid UTF-8"))?;
        if value.contains(['\n', '\r']) {
            return Err(invalid_data("executable path contains a line break"));
        }
        let mut escaped = String::with_capacity(value.len() + 2);
        escaped.push('"');
        for character in value.chars() {
            if matches!(character, '"' | '\\' | '$' | '`') {
                escaped.push('\\');
            }
            escaped.push(character);
        }
        escaped.push('"');
        Ok(escaped)
    }

    pub(super) fn registration_contents(executable: &std::path::Path) -> io::Result<String> {
        Ok(format!(
            "[Desktop Entry]\nType=Application\nVersion=1.0\nName={APP_NAME}\nComment=Start {APP_NAME} when you sign in\nExec={}\nTerminal=false\nStartupNotify=false\nX-GNOME-Autostart-enabled=true\n",
            desktop_exec_argument(executable)?
        ))
    }

    pub(super) fn is_enabled() -> io::Result<bool> {
        let path = registration_path()?;
        let expected = registration_contents(&current_executable()?)?;
        registration_matches(&path, &expected)
    }

    pub(super) fn set_enabled(enabled: bool) -> io::Result<()> {
        let path = registration_path()?;
        if enabled {
            write_registration(&path, &registration_contents(&current_executable()?)?)
        } else {
            match std::fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use objc2_service_management::{SMAppService, SMAppServiceStatus};

    use super::*;

    pub(super) fn open_login_items_settings() -> io::Result<()> {
        unsafe { SMAppService::openSystemSettingsLoginItems() };
        Ok(())
    }

    pub(super) fn is_enabled() -> io::Result<bool> {
        let service = unsafe { SMAppService::mainAppService() };
        Ok(unsafe { service.status() } == SMAppServiceStatus::Enabled)
    }

    pub(super) fn set_enabled(_enabled: bool) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "manage macOS login items in System Settings",
        ))
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt};

    use windows::{
        Win32::{
            Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR},
            System::Registry::{
                HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW,
                RegSetKeyValueW,
            },
        },
        core::PCWSTR,
    };

    use super::*;

    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const RUN_VALUE: &str = APP_NAME;

    fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    fn command(executable: &std::path::Path) -> io::Result<String> {
        let value = executable
            .to_str()
            .ok_or_else(|| invalid_data("executable path is not valid Unicode"))?;
        if value.contains(['"', '\0', '\n', '\r']) {
            return Err(invalid_data(
                "executable path contains an invalid character",
            ));
        }
        let command = format!("\"{value}\"");
        // Windows documents a 260-character ceiling for Run commands. Reject
        // unusable registrations instead of showing a misleading enabled state.
        if command.encode_utf16().count() > 260 {
            return Err(invalid_data("startup command exceeds the Windows limit"));
        }
        Ok(command)
    }

    fn win32_result(result: WIN32_ERROR) -> io::Result<()> {
        if result == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(result.0 as i32))
        }
    }

    fn read_registered_command() -> io::Result<Option<String>> {
        let key = wide(std::ffi::OsStr::new(RUN_KEY));
        let value = wide(std::ffi::OsStr::new(RUN_VALUE));
        let mut byte_count = 0u32;
        // Query the byte count first so the registry owns all sizing decisions.
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(value.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut byte_count),
            )
        };
        if result == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        win32_result(result)?;

        let mut buffer = vec![0u16; (byte_count as usize).div_ceil(2)];
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(value.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast::<c_void>()),
                Some(&mut byte_count),
            )
        };
        win32_result(result)?;
        let length = buffer
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(buffer.len());
        Ok(Some(String::from_utf16_lossy(&buffer[..length])))
    }

    pub(super) fn is_enabled() -> io::Result<bool> {
        Ok(
            read_registered_command()?.as_deref()
                == Some(command(&current_executable()?)?.as_str()),
        )
    }

    pub(super) fn set_enabled(enabled: bool) -> io::Result<()> {
        let key = wide(std::ffi::OsStr::new(RUN_KEY));
        let value = wide(std::ffi::OsStr::new(RUN_VALUE));
        if !enabled {
            let result = unsafe {
                RegDeleteKeyValueW(
                    HKEY_CURRENT_USER,
                    PCWSTR(key.as_ptr()),
                    PCWSTR(value.as_ptr()),
                )
            };
            return if result == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                win32_result(result)
            };
        }

        let command = wide(std::ffi::OsStr::new(&command(&current_executable()?)?));
        let byte_count = u32::try_from(command.len() * std::mem::size_of::<u16>())
            .map_err(|_| invalid_data("startup command is too long"))?;
        let result = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(value.as_ptr()),
                REG_SZ.0,
                Some(command.as_ptr().cast::<c_void>()),
                byte_count,
            )
        };
        win32_result(result)
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
mod platform {
    use super::*;

    pub(super) fn is_enabled() -> io::Result<bool> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "launch at login is not supported on this platform",
        ))
    }

    pub(super) fn set_enabled(_enabled: bool) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "launch at login is not supported on this platform",
        ))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_registration_quotes_desktop_exec_metacharacters() {
        let contents = platform::registration_contents(std::path::Path::new(
            "/opt/Oxide Term/$preview`build`/oxideterm-native",
        ))
        .unwrap();

        assert!(
            contents.contains("Exec=\"/opt/Oxide Term/\\$preview\\`build\\`/oxideterm-native\"")
        );
        assert!(contents.contains("X-GNOME-Autostart-enabled=true"));
    }
}
