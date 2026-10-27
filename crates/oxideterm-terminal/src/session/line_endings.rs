#[derive(Debug, Default)]
pub(crate) struct LineEndingIngress {
    previous_was_carriage_return: bool,
}

impl LineEndingIngress {
    pub(crate) fn normalize(&mut self, bytes: &[u8], line_ending: SerialLineEnding) -> Vec<u8> {
        if matches!(line_ending, SerialLineEnding::None) {
            self.previous_was_carriage_return = false;
            return bytes.to_vec();
        }

        let replacement = line_ending_bytes(line_ending);
        let mut normalized = Vec::with_capacity(bytes.len() + 4);
        for &byte in bytes {
            match byte {
                b'\r' => {
                    normalized.extend_from_slice(replacement);
                    self.previous_was_carriage_return = true;
                }
                b'\n' if self.previous_was_carriage_return => {
                    // The preceding CR already emitted one normalized line ending.
                    self.previous_was_carriage_return = false;
                }
                b'\n' => {
                    normalized.extend_from_slice(replacement);
                    self.previous_was_carriage_return = false;
                }
                _ => {
                    normalized.push(byte);
                    self.previous_was_carriage_return = false;
                }
            }
        }
        normalized
    }

    pub(crate) fn reset(&mut self) {
        self.previous_was_carriage_return = false;
    }
}

pub(crate) fn encode_text_line_endings(bytes: &[u8], line_ending: SerialLineEnding) -> Vec<u8> {
    if matches!(line_ending, SerialLineEnding::None) {
        return bytes.to_vec();
    }

    let replacement = line_ending_bytes(line_ending);

    let mut encoded = Vec::with_capacity(bytes.len() + 4);
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                if bytes.get(index + 1) == Some(&b'\n') {
                    index += 1;
                }
                encoded.extend_from_slice(replacement);
            }
            b'\n' => encoded.extend_from_slice(replacement),
            byte => encoded.push(byte),
        }
        index += 1;
    }
    encoded
}

fn line_ending_bytes(line_ending: SerialLineEnding) -> &'static [u8] {
    match line_ending {
        SerialLineEnding::Lf => b"\n",
        SerialLineEnding::CrLf => b"\r\n",
        SerialLineEnding::Cr => b"\r",
        SerialLineEnding::None => b"",
    }
}
