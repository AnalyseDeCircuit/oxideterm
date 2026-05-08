impl Drop for LocalPtySession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn control_code_for_ascii(ch: char) -> Option<u8> {
    let lower = ch.to_ascii_lowercase();
    if lower.is_ascii_lowercase() {
        Some((lower as u8) & 0x1f)
    } else {
        None
    }
}

