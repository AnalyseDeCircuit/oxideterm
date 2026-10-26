//! Measures the same bounded-copy and result-resolution path used by background search.
use std::hint::black_box;
use std::time::Instant;

use oxideterm_terminal::{GraphicsOptions, TerminalSession};

fn main() {
    println!("case,sample,miss_us,literal_us,regex_us");
    for case in ["full400", "color400", "alternating400", "unicode400"] {
        let mut input = String::new();
        for _ in 0..20_040 {
            for col in 0..400 {
                if case == "color400" && col % 40 == 0 {
                    input.push_str(&format!("\x1b[38;5;{}m", 1 + (col / 40) % 8));
                } else if case == "alternating400" {
                    input.push_str(if col % 2 == 0 { "\x1b[31m" } else { "\x1b[32m" });
                }
                if case == "unicode400" {
                    if col % 4 == 0 {
                        input.push_str("界e\u{301} ");
                    }
                } else {
                    input.push(char::from(b'a' + (col % 26) as u8));
                }
            }
            input.push_str("\x1b[0m\r\n");
        }
        let mut terminal =
            TerminalSession::recording_playback(400, 40, GraphicsOptions::default(), 20_000);
        terminal.feed_recording_output(input.as_bytes());
        let source = terminal.search_source().unwrap();
        for sample in 0..6 {
            let mut times = Vec::new();
            for (query, regex) in [
                ("NEVER_MATCH_012345", false),
                ("zabcdefghijklm", false),
                ("[A-Z]{12}", true),
            ] {
                let start = Instant::now();
                let matches = source.search_matches_with_options(
                    query,
                    oxideterm_terminal::TerminalSearchOptions {
                        case_sensitive: true,
                        regex,
                    },
                    &|| false,
                );
                times.push(start.elapsed().as_micros());
                black_box(matches);
            }
            if sample != 0 {
                println!("{case},{sample},{},{},{}", times[0], times[1], times[2]);
            }
        }
    }
}
