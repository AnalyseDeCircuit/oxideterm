use crate::NewConnectionForm;
use oxideterm_connections::{SerialProfile, TelnetProfile};

pub fn terminal_serial_parity_from_profile(
    parity: &oxideterm_connections::SerialParity,
) -> oxideterm_terminal::SerialParity {
    match parity {
        oxideterm_connections::SerialParity::None => oxideterm_terminal::SerialParity::None,
        oxideterm_connections::SerialParity::Odd => oxideterm_terminal::SerialParity::Odd,
        oxideterm_connections::SerialParity::Even => oxideterm_terminal::SerialParity::Even,
    }
}

pub fn terminal_serial_flow_from_profile(
    flow: &oxideterm_connections::SerialFlowControl,
) -> oxideterm_terminal::SerialFlowControl {
    match flow {
        oxideterm_connections::SerialFlowControl::None => {
            oxideterm_terminal::SerialFlowControl::None
        }
        oxideterm_connections::SerialFlowControl::Software => {
            oxideterm_terminal::SerialFlowControl::Software
        }
        oxideterm_connections::SerialFlowControl::Hardware => {
            oxideterm_terminal::SerialFlowControl::Hardware
        }
    }
}

pub fn terminal_serial_runtime_options_from_profile(
    profile: &SerialProfile,
) -> oxideterm_terminal::SerialRuntimeOptions {
    oxideterm_terminal::SerialRuntimeOptions {
        line_ending: terminal_serial_line_ending_from_profile(profile.input_line_ending),
        output_line_ending: terminal_serial_line_ending_from_profile(profile.output_line_ending),
        ..Default::default()
    }
}

pub fn terminal_serial_line_ending_from_profile(
    line_ending: oxideterm_connections::SerialLineEnding,
) -> oxideterm_terminal::SerialLineEnding {
    match line_ending {
        oxideterm_connections::SerialLineEnding::Lf => oxideterm_terminal::SerialLineEnding::Lf,
        oxideterm_connections::SerialLineEnding::CrLf => oxideterm_terminal::SerialLineEnding::CrLf,
        oxideterm_connections::SerialLineEnding::Cr => oxideterm_terminal::SerialLineEnding::Cr,
        oxideterm_connections::SerialLineEnding::None => oxideterm_terminal::SerialLineEnding::None,
    }
}

pub fn terminal_serial_send_mode_from_profile(
    mode: oxideterm_connections::SerialSendMode,
) -> oxideterm_terminal::SerialSendMode {
    match mode {
        oxideterm_connections::SerialSendMode::Text => oxideterm_terminal::SerialSendMode::Text,
        oxideterm_connections::SerialSendMode::Hex => oxideterm_terminal::SerialSendMode::Hex,
    }
}

pub fn terminal_serial_display_mode_from_profile(
    mode: oxideterm_connections::SerialDisplayMode,
) -> oxideterm_terminal::SerialDisplayMode {
    match mode {
        oxideterm_connections::SerialDisplayMode::Text => {
            oxideterm_terminal::SerialDisplayMode::Text
        }
        oxideterm_connections::SerialDisplayMode::Hex => oxideterm_terminal::SerialDisplayMode::Hex,
        oxideterm_connections::SerialDisplayMode::Mixed => {
            oxideterm_terminal::SerialDisplayMode::Mixed
        }
    }
}

pub fn terminal_serial_runtime_options_from_ble_profile(
    profile: &oxideterm_connections::BleProfile,
) -> oxideterm_terminal::SerialRuntimeOptions {
    oxideterm_terminal::SerialRuntimeOptions {
        line_ending: terminal_serial_line_ending_from_profile(profile.input_line_ending),
        output_line_ending: terminal_serial_line_ending_from_profile(profile.output_line_ending),
        send_mode: terminal_serial_send_mode_from_profile(profile.send_mode),
        display_mode: terminal_serial_display_mode_from_profile(profile.display_mode),
        local_echo: profile.local_echo,
    }
}

pub fn terminal_serial_runtime_options_from_ble_form(
    form: &NewConnectionForm,
) -> oxideterm_terminal::SerialRuntimeOptions {
    oxideterm_terminal::SerialRuntimeOptions {
        line_ending: terminal_serial_line_ending_from_profile(form.ble.input_line_ending),
        output_line_ending: terminal_serial_line_ending_from_profile(form.ble.output_line_ending),
        send_mode: terminal_serial_send_mode_from_profile(form.ble.send_mode),
        display_mode: terminal_serial_display_mode_from_profile(form.ble.display_mode),
        local_echo: form.ble.local_echo,
    }
}

pub fn terminal_telnet_line_endings_from_profile(
    profile: &TelnetProfile,
) -> oxideterm_terminal::TelnetLineEndings {
    oxideterm_terminal::TelnetLineEndings {
        input: terminal_serial_line_ending_from_profile(profile.input_line_ending),
        output: terminal_serial_line_ending_from_profile(profile.output_line_ending),
    }
}
