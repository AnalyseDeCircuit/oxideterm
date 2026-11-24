use super::*;
use gpui::PathPromptOptions;
pub(super) fn pick_path(
    flow: &Entity<ConnectionFlowEntity>,
    field: NewConnectionField,
    i18n: &I18n,
    cx: &mut App,
) {
    let receiver = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some(SharedString::from(
            i18n.t("sessionManager.edit_properties.browse"),
        )),
    });
    let selection = async move {
        let Ok(Ok(Some(paths))) = receiver.await else {
            return None;
        };
        let Some(path) = paths.into_iter().next() else {
            return None;
        };
        Some(path.to_string_lossy().to_string())
    };
    flow.update(cx, |connection_flow, cx| {
        connection_flow.start_path_picker(field, selection, cx);
    });
}
pub(super) fn serial_ports() -> Result<Vec<crate::form_model::SerialPortInfo>, String> {
    oxideterm_terminal::serial_list_ports().map_err(|error| error.to_string())
}

use crate::form_model::SshAlgorithmPreferences;
impl FormAlgorithmCapabilities {
    pub(in crate::views) fn desktop() -> Self {
        fn choices(offer: oxideterm_ssh::SshAlgorithmOffer) -> SshAlgorithmPreferences {
            SshAlgorithmPreferences {
                kex: offer.kex,
                host_key: offer.host_key_algorithms,
                cipher: offer.ciphers,
                mac: offer.macs,
                compression: offer.compression,
            }
        }
        let report = oxideterm_ssh::ssh_capability_report();
        Self {
            default_offer: choices(report.default_offer),
            legacy_compatibility_offer: choices(report.legacy_compatibility_offer),
        }
    }
}
