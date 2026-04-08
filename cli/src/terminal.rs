// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Raw terminal mode management.
//!
//! Provides RAII-based raw mode toggling and terminal size queries.
//! On Unix uses `termios` via `libc`. On Windows uses Console API.

/// RAII guard that restores original terminal settings on drop.
pub struct RawModeGuard {
    #[cfg(unix)]
    original: libc::termios,
    #[cfg(windows)]
    handle: isize,
    #[cfg(windows)]
    original_mode: u32,
    #[cfg(windows)]
    active: bool,
}

impl RawModeGuard {
    /// Enter raw mode. Returns a guard that restores settings on drop.
    pub fn enter() -> Result<Self, String> {
        #[cfg(unix)]
        {
            use std::mem::MaybeUninit;
            let fd = libc::STDIN_FILENO;

            let mut original = MaybeUninit::<libc::termios>::uninit();
            if unsafe { libc::tcgetattr(fd, original.as_mut_ptr()) } != 0 {
                return Err("Failed to get terminal attributes".to_string());
            }
            let original = unsafe { original.assume_init() };

            let mut raw = original;
            unsafe { libc::cfmakeraw(&mut raw) };
            // Keep ISIG so SIGWINCH still works
            raw.c_lflag |= libc::ISIG;

            if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
                return Err("Failed to set raw mode".to_string());
            }

            Ok(Self { original })
        }

        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Console::{
                GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT,
                ENABLE_PROCESSED_INPUT, STD_INPUT_HANDLE,
            };

            let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
            if handle.is_null() || handle == -1isize as _ {
                return Ok(Self {
                    handle: 0,
                    original_mode: 0,
                    active: false,
                });
            }

            let mut original_mode = 0;
            if unsafe { GetConsoleMode(handle, &mut original_mode) } == 0 {
                return Ok(Self {
                    handle: handle as isize,
                    original_mode: 0,
                    active: false,
                });
            }

            let mut raw_mode = original_mode;
            raw_mode &= !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT);

            if unsafe { SetConsoleMode(handle, raw_mode) } == 0 {
                return Err("Failed to set raw mode".to_string());
            }

            Ok(Self {
                handle: handle as isize,
                original_mode,
                active: true,
            })
        }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.original);
            }
        }

        #[cfg(windows)]
        {
            if self.active {
                unsafe {
                    windows_sys::Win32::System::Console::SetConsoleMode(
                        self.handle as _,
                        self.original_mode,
                    );
                }
            }
        }
    }
}

/// Get current terminal size (cols, rows).
pub fn get_terminal_size() -> (u16, u16) {
    #[cfg(unix)]
    {
        use std::mem::MaybeUninit;
        let mut ws = MaybeUninit::<libc::winsize>::uninit();
        let ret = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, ws.as_mut_ptr()) };
        if ret == 0 {
            let ws = unsafe { ws.assume_init() };
            (ws.ws_col, ws.ws_row)
        } else {
            (80, 24) // fallback
        }
    }

    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{
            GetConsoleScreenBufferInfo, GetStdHandle, CONSOLE_SCREEN_BUFFER_INFO, STD_OUTPUT_HANDLE,
        };
        let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        if !handle.is_null() && handle != -1isize as _ {
            let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
            if unsafe { GetConsoleScreenBufferInfo(handle, &mut info) } != 0 {
                let cols = (info.srWindow.Right - info.srWindow.Left + 1) as u16;
                let rows = (info.srWindow.Bottom - info.srWindow.Top + 1) as u16;
                return (cols, rows);
            }
        }
        (80, 24) // fallback
    }
}

/// Try to resize the CLI terminal window to match the session size
/// using the xterm `\e[8;rows;cols t` escape sequence.
/// Returns the actual size after the attempt.
pub fn try_resize_terminal(cols: u16, rows: u16) -> (u16, u16) {
    use std::io::Write;
    let seq = format!("\x1b[8;{};{}t", rows, cols);
    let _ = std::io::stdout().write_all(seq.as_bytes());
    let _ = std::io::stdout().flush();
    // Give the terminal emulator time to process
    std::thread::sleep(std::time::Duration::from_millis(100));
    get_terminal_size()
}

/// Disable all mouse tracking modes on the CLI terminal.
/// TUI apps (e.g. yazi) enable mouse tracking via escape sequences.
/// If they exit without cleanup (Ctrl+C), mouse events from the CLI
/// terminal get forwarded to the server shell as garbage text.
pub fn disable_mouse_tracking() {
    use std::io::Write;
    // Disable: X10, normal, button-event, any-event, SGR extended, urxvt
    let _ = std::io::stdout()
        .write_all(b"\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[?1015l");
    let _ = std::io::stdout().flush();
}

/// Install a SIGWINCH handler that writes to the given pipe fd.
///
/// When the terminal is resized, the handler writes a single byte to `write_fd`,
/// which can be polled alongside the WebSocket connection.
#[cfg(unix)]
pub fn install_sigwinch_handler(write_fd: i32) {
    unsafe {
        SIGWINCH_PIPE_FD.store(write_fd, std::sync::atomic::Ordering::Relaxed);
        libc::signal(libc::SIGWINCH, sigwinch_handler as libc::sighandler_t);
    }
}

/// Reset the SIGWINCH handler to default and clear the pipe fd.
/// Must be called before closing the pipe file descriptors to
/// prevent the signal handler from writing to a closed fd.
#[cfg(unix)]
pub fn reset_sigwinch_handler() {
    unsafe {
        libc::signal(libc::SIGWINCH, libc::SIG_DFL);
        SIGWINCH_PIPE_FD.store(-1, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(unix)]
static SIGWINCH_PIPE_FD: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

#[cfg(unix)]
extern "C" fn sigwinch_handler(_sig: libc::c_int) {
    let fd = SIGWINCH_PIPE_FD.load(std::sync::atomic::Ordering::Relaxed);
    if fd >= 0 {
        unsafe {
            libc::write(fd, b"R" as *const u8 as *const libc::c_void, 1);
        }
    }
}
