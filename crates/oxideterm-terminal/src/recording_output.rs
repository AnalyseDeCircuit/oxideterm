use std::collections::VecDeque;

enum PendingRecordingFrame {
    Output(zeroize::Zeroizing<Vec<u8>>, usize),
    Resize(u16, u16),
}

// The parser owns the unaccepted suffix. Retrying it never repeats terminal,
// shell-integration, or in-band protocol side effects.
pub(crate) struct RecordingOutput {
    sink: oxideterm_audit::RecordingSink,
    generation: u64,
    pending: VecDeque<PendingRecordingFrame>,
}

impl RecordingOutput {
    pub(crate) fn new(sink: oxideterm_audit::RecordingSink) -> Self {
        Self {
            generation: sink.generation(),
            sink,
            pending: VecDeque::new(),
        }
    }

    pub(crate) fn output(&mut self, bytes: &[u8]) {
        self.flush();
        if !self.sink.is_enabled() {
            return;
        }
        if self.pending.is_empty() {
            match self.sink.try_record_output(bytes) {
                Ok(accepted) if accepted == bytes.len() => return,
                Ok(accepted) => self.pending.push_back(PendingRecordingFrame::Output(
                    zeroize::Zeroizing::new(bytes[accepted..].to_vec()),
                    0,
                )),
                Err(_) => return,
            }
        } else {
            self.pending.push_back(PendingRecordingFrame::Output(
                zeroize::Zeroizing::new(bytes.to_vec()),
                0,
            ));
        }
    }

    pub(crate) fn resize(&mut self, columns: u16, rows: u16) {
        self.flush();
        if !self.sink.is_enabled() {
            return;
        }
        self.pending
            .push_back(PendingRecordingFrame::Resize(columns, rows));
        self.flush();
    }

    pub(crate) fn flush(&mut self) -> bool {
        let generation = self.sink.generation();
        if generation != self.generation || !self.sink.is_enabled() {
            // A later enable must never capture a previous policy interval's suffix.
            self.pending.clear();
            self.generation = generation;
        }
        while let Some(frame) = self.pending.front_mut() {
            let accepted = match frame {
                PendingRecordingFrame::Output(bytes, offset) => {
                    self.sink.try_record_output(&bytes[*offset..]).map(|count| {
                        *offset += count;
                        *offset == bytes.len()
                    })
                }
                PendingRecordingFrame::Resize(columns, rows) => {
                    self.sink.try_resize(*columns, *rows)
                }
            };
            match accepted {
                Ok(true) => {
                    self.pending.pop_front();
                }
                Ok(false) => return false,
                Err(_) => {
                    self.pending.clear();
                    break;
                }
            }
        }
        true
    }
}

impl Drop for RecordingOutput {
    fn drop(&mut self) {
        if !self.pending.is_empty() {
            self.sink.interrupt();
        }
    }
}
