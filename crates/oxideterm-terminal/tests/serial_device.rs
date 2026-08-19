// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::io::{Read, Write};

use oxideterm_terminal::{SerialFlowControl, SerialParity, SerialSessionConfig};

#[test]
#[ignore = "requires OXIDETERM_SERIAL_MANUAL_PORT to point at a real or pseudo serial device"]
fn manual_serial_pseudo_device_round_trip_and_reopen() {
    let port_path = std::env::var("OXIDETERM_SERIAL_MANUAL_PORT")
        .expect("OXIDETERM_SERIAL_MANUAL_PORT must point at a serial device");
    let config = SerialSessionConfig {
        port_path: port_path.clone(),
        baud_rate: 115_200,
        data_bits: 8,
        stop_bits: 1,
        parity: SerialParity::None,
        flow_control: SerialFlowControl::None,
    };
    config.validate().unwrap();

    let first_ping = b"oxideterm-serial-ping-1\r";
    let first_pong = b"oxideterm-serial-pong-1\r";
    let second_ping = b"oxideterm-serial-ping-2\r";
    let second_pong = b"oxideterm-serial-pong-2\r";
    let first_expected = manual_serial_expected(first_ping, first_pong);
    let second_expected = manual_serial_expected(second_ping, second_pong);

    manual_serial_round_trip(&port_path, first_ping, &first_expected);
    manual_serial_round_trip(&port_path, second_ping, &second_expected);
}

fn manual_serial_expected(loopback_payload: &[u8], responder_payload: &[u8]) -> Vec<u8> {
    match std::env::var("OXIDETERM_SERIAL_MANUAL_MODE")
        .unwrap_or_else(|_| "loopback".to_string())
        .as_str()
    {
        "loopback" => loopback_payload.to_vec(),
        "responder" => responder_payload.to_vec(),
        mode => {
            panic!("unsupported OXIDETERM_SERIAL_MANUAL_MODE={mode}; use loopback or responder")
        }
    }
}

fn manual_serial_round_trip(port_path: &str, ping: &[u8], expected: &[u8]) {
    let mut port = serialport::new(port_path, 115_200)
        .data_bits(serialport::DataBits::Eight)
        .stop_bits(serialport::StopBits::One)
        .parity(serialport::Parity::None)
        .flow_control(serialport::FlowControl::None)
        .timeout(std::time::Duration::from_secs(2))
        .open()
        .expect("manual serial port should open at 115200 8N1");

    port.write_all(ping).expect("manual serial write failed");
    port.flush().expect("manual serial flush failed");

    let mut read_buf = vec![0_u8; expected.len()];
    port.read_exact(&mut read_buf)
        .expect("manual serial read failed");
    assert_eq!(read_buf, expected);
}
