// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) type SharedVncCapabilities = Arc<Mutex<NegotiatedCapabilities>>;

/// Clones the complete cumulative server evidence into a helper event.
pub(super) fn vnc_capabilities_event(
    capabilities: &SharedVncCapabilities,
) -> Result<RemoteDesktopHelperEvent, String> {
    let capabilities = capabilities
        .lock()
        .map_err(|_| "VNC capability state lock is poisoned.".to_string())?
        .clone();
    Ok(RemoteDesktopHelperEvent::CapabilitiesNegotiated { capabilities })
}

/// Applies one observation and emits only when the cumulative snapshot changes.
pub(super) fn update_vnc_capabilities(
    capabilities: &SharedVncCapabilities,
    update: impl FnOnce(&mut NegotiatedCapabilities),
) -> Result<Option<RemoteDesktopHelperEvent>, String> {
    let mut capabilities = capabilities
        .lock()
        .map_err(|_| "VNC capability state lock is poisoned.".to_string())?;
    let previous = capabilities.clone();
    update(&mut capabilities);
    if *capabilities == previous {
        return Ok(None);
    }
    Ok(Some(RemoteDesktopHelperEvent::CapabilitiesNegotiated {
        capabilities: capabilities.clone(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cumulative_capability_updates_preserve_prior_evidence() {
        let capabilities = Arc::new(Mutex::new(NegotiatedCapabilities::default()));
        update_vnc_capabilities(&capabilities, |snapshot| {
            snapshot.encrypted = NegotiatedCapabilityStatus::Supported;
        })
        .unwrap();
        let event = update_vnc_capabilities(&capabilities, |snapshot| {
            snapshot.qemu_audio = NegotiatedCapabilityStatus::Supported;
        })
        .unwrap()
        .unwrap();

        let RemoteDesktopHelperEvent::CapabilitiesNegotiated { capabilities } = event else {
            panic!("expected negotiated capability event");
        };
        assert_eq!(
            capabilities.encrypted,
            NegotiatedCapabilityStatus::Supported
        );
        assert_eq!(
            capabilities.qemu_audio,
            NegotiatedCapabilityStatus::Supported
        );
    }

    #[test]
    fn duplicate_observations_do_not_emit_duplicate_events() {
        let capabilities = Arc::new(Mutex::new(NegotiatedCapabilities::default()));
        update_vnc_capabilities(&capabilities, |snapshot| {
            snapshot.qemu_audio = NegotiatedCapabilityStatus::Supported;
        })
        .unwrap();

        assert!(
            update_vnc_capabilities(&capabilities, |snapshot| {
                snapshot.qemu_audio = NegotiatedCapabilityStatus::Supported;
            })
            .unwrap()
            .is_none()
        );
    }
}
