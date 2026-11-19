// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

// Keep scheduling policy independent from GPUI so lifecycle edges remain unit-testable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SftpRemoteLoadState {
    pub loading: bool,
    pub pending: bool,
    pub inflight: bool,
}

impl SftpRemoteLoadState {
    pub fn request(mut self) -> Self {
        // A newer request queues behind the one shared in-flight list operation.
        self.loading = true;
        self.pending = true;
        self
    }

    pub fn start(mut self) -> Option<Self> {
        // SFTP views share one list slot, which keeps stale completions unambiguous.
        if self.inflight || !self.pending {
            return None;
        }
        self.loading = true;
        self.pending = false;
        self.inflight = true;
        Some(self)
    }

    pub fn complete(mut self) -> Self {
        // Keep the loading indicator only when another request is already queued.
        self.inflight = false;
        self.loading = self.pending;
        self
    }
}

pub fn remote_list_result_is_superseded(
    requested_path: &str,
    current_path: &str,
    load_pending: bool,
) -> bool {
    // A queued navigation owns the visible path, so an older listing must only release the slot.
    load_pending && requested_path != current_path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_remote_load_starts_after_the_previous_request_completes() {
        let old_request = SftpRemoteLoadState::default().request().start().unwrap();
        let switched_view = old_request.request();

        let old_request_completed = switched_view.complete();

        assert_eq!(
            old_request_completed,
            SftpRemoteLoadState {
                loading: true,
                pending: true,
                inflight: false,
            }
        );
        assert_eq!(
            old_request_completed.start(),
            Some(SftpRemoteLoadState {
                loading: true,
                pending: false,
                inflight: true,
            })
        );
    }
}
