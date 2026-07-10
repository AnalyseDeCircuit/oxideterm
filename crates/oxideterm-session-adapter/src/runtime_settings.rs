// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Conversion from persisted settings into runtime-owned configuration types.

use std::time::Duration;

use oxideterm_settings::{PersistedSettings, TerminalEncoding as SettingsTerminalEncoding};
use oxideterm_sftp::SftpTransferRuntimeSettings;
use oxideterm_ssh::ReconnectTiming;
use oxideterm_terminal::TerminalEncoding;

pub fn sftp_runtime_settings_from_settings(
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

pub fn reconnect_timing_from_settings(settings: &PersistedSettings) -> ReconnectTiming {
    ReconnectTiming {
        retry_base_delay: Duration::from_millis(settings.reconnect.base_delay_ms.max(1) as u64),
        retry_max_delay: Duration::from_millis(settings.reconnect.max_delay_ms.max(1) as u64),
        ..ReconnectTiming::default()
    }
}

pub fn reconnect_max_attempts_from_settings(settings: &PersistedSettings) -> u32 {
    settings.reconnect.max_attempts.max(1) as u32
}

pub fn terminal_encoding_from_settings(encoding: SettingsTerminalEncoding) -> TerminalEncoding {
    match encoding {
        SettingsTerminalEncoding::Utf8 => TerminalEncoding::Utf8,
        SettingsTerminalEncoding::Gbk => TerminalEncoding::Gbk,
        SettingsTerminalEncoding::Gb18030 => TerminalEncoding::Gb18030,
        SettingsTerminalEncoding::Big5 => TerminalEncoding::Big5,
        SettingsTerminalEncoding::ShiftJis => TerminalEncoding::ShiftJis,
        SettingsTerminalEncoding::EucJp => TerminalEncoding::EucJp,
        SettingsTerminalEncoding::EucKr => TerminalEncoding::EucKr,
        SettingsTerminalEncoding::Windows1252 => TerminalEncoding::Windows1252,
    }
}
