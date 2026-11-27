use oxideterm_plugin_protocol::discovery::{DiscoveryOpen, DiscoveryReply, DiscoveryRequest};
use std::collections::HashMap;

#[derive(Hash, Eq, PartialEq)]
enum ResourceId {
    Open(String),
    Refresh(String),
}
struct Resource {
    provider: String,
    source: String,
    close: bool,
}

/// Host-issued IDs refer to this generation until acknowledged cleanup or process exit.
#[derive(Default)]
pub(super) struct DiscoveryResources(HashMap<ResourceId, Resource>);

impl DiscoveryResources {
    pub(super) fn cleanup_request(
        &self,
        provider: &str,
        source: &str,
        request: DiscoveryRequest,
    ) -> DiscoveryRequest {
        if let DiscoveryRequest::CancelOpen { instance_id } = &request
            && self
                .0
                .get(&ResourceId::Open(instance_id.clone()))
                .is_some_and(|r| r.close && r.provider == provider && r.source == source)
        {
            // Cancellation may have raced the final ready response.
            return DiscoveryRequest::Close {
                instance_id: instance_id.clone(),
            };
        }
        request
    }
    /// A malformed launch can still deliver a provider-owned resource. Record its
    /// owner before validation so cancelling the rejected open sends the correct cleanup.
    pub(super) fn observe_ready(
        &mut self,
        provider: &str,
        source: &str,
        request: &DiscoveryRequest,
        value: &serde_json::Value,
    ) -> Result<(), oxideterm_plugin_protocol::PluginError> {
        let DiscoveryRequest::PrepareOpen { instance_id, .. } = request else {
            return Ok(());
        };
        if value.get("kind").and_then(|kind| kind.as_str()) != Some("ready") {
            return Ok(());
        }
        let key = ResourceId::Open(instance_id.clone());
        match value.get("owner").and_then(|owner| owner.as_str()) {
            Some("host") => {
                self.0.remove(&key);
            }
            Some("provider") => {
                self.0.insert(
                    key,
                    Resource {
                        provider: provider.into(),
                        source: source.into(),
                        close: true,
                    },
                );
            }
            _ => {
                return Err(oxideterm_plugin_protocol::PluginError::protocol(
                    "invalidDiscoveryMessage",
                    "Unable to determine discovery resource ownership",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn record(
        &mut self,
        provider: &str,
        source: &str,
        request: &DiscoveryRequest,
        reply: &DiscoveryReply,
    ) {
        let (key, cleanup) = match (request, reply) {
            (
                DiscoveryRequest::PrepareOpen { instance_id, .. },
                DiscoveryReply::Open(DiscoveryOpen::Choose { .. }),
            ) => (ResourceId::Open(instance_id.clone()), Some(false)),
            (DiscoveryRequest::List { refresh_id, .. }, DiscoveryReply::List(page)) => (
                ResourceId::Refresh(refresh_id.clone()),
                page.next_cursor.as_ref().map(|_| false),
            ),
            (DiscoveryRequest::CancelRefresh { refresh_id }, DiscoveryReply::Done) => {
                (ResourceId::Refresh(refresh_id.clone()), None)
            }
            (
                DiscoveryRequest::Close { instance_id }
                | DiscoveryRequest::CancelOpen { instance_id },
                DiscoveryReply::Done,
            ) => (ResourceId::Open(instance_id.clone()), None),
            _ => return,
        };
        if let Some(close) = cleanup {
            self.0.insert(
                key,
                Resource {
                    provider: provider.into(),
                    source: source.into(),
                    close,
                },
            );
        } else {
            self.0.remove(&key);
        }
    }
    pub(super) fn drain(&mut self) -> Vec<(String, String, DiscoveryRequest)> {
        self.0
            .drain()
            .map(|(id, r)| {
                let request = match id {
                    ResourceId::Open(instance_id) if r.close => {
                        DiscoveryRequest::Close { instance_id }
                    }
                    ResourceId::Open(instance_id) => DiscoveryRequest::CancelOpen { instance_id },
                    ResourceId::Refresh(refresh_id) => {
                        DiscoveryRequest::CancelRefresh { refresh_id }
                    }
                };
                (r.provider, r.source, request)
            })
            .collect()
    }
}
