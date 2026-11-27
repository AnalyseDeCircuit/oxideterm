// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::PluginError;
use oxideterm_plugin_manifest::discovery::{
    DiscoveryConfigUse, DiscoveryLaunchKind, NativePluginDiscoveryProviderDef,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryGroupSegment {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryTarget {
    pub id: String,
    pub label: String,
    /// Public display text supplied by the provider, independent of launch parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub scope: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_paths: Vec<Vec<DiscoveryGroupSegment>>,
    pub target_revision: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryPage {
    pub targets: Vec<DiscoveryTarget>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryOption {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscoveryOwner {
    Host,
    Provider,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryEndpoint {
    pub host: String,
    pub port: std::num::NonZeroU16,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DiscoveryLaunch {
    Ssh {
        username: Option<String>,
        endpoint: DiscoveryEndpoint,
        #[serde(default)]
        allow_reuse_on_open: bool,
        #[serde(default)]
        asset_session: bool,
        session_identity: Option<String>,
    },
    Terminal {
        args: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DiscoveryOpen {
    Choose {
        title_key: String,
        continuation: String,
        options: Vec<DiscoveryOption>,
    },
    Ready {
        launch: DiscoveryLaunch,
        owner: DiscoveryOwner,
    },
}

pub enum DiscoveryReply {
    List(DiscoveryPage),
    Open(DiscoveryOpen),
    Done,
}

pub enum DiscoveryRequest {
    List {
        refresh_id: String,
        config: Value,
        cursor: Option<String>,
    },
    PrepareOpen {
        instance_id: String,
        target_id: String,
        config: Value,
        continuation: Option<String>,
        option_id: Option<String>,
    },
    SourceAction {
        action_id: String,
        config: Value,
    },
    CancelRefresh {
        refresh_id: String,
    },
    CancelOpen {
        instance_id: String,
    },
    Close {
        instance_id: String,
    },
}

fn invalid() -> PluginError {
    PluginError::protocol(
        "invalidDiscoveryMessage",
        "Invalid discovery message or undeclared capability",
    )
}
fn text_ok(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}
fn username_ok(value: &str) -> bool {
    text_ok(value) && value.len() <= 255
}
fn host_ok(value: &str) -> bool {
    text_ok(value)
        && !value.chars().any(char::is_whitespace)
        && !value.contains(['/', '@', '?', '#'])
}
fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, PluginError> {
    serde_json::from_value(value).map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> NativePluginDiscoveryProviderDef {
        serde_json::from_value(json!({"id":"hosts","title":"Hosts","icon":"server","protocolVersion":1,"launchKinds":["ssh"],"form":{"controls":[]}})).unwrap()
    }

    #[test]
    fn discovery_list_validates_optional_group_paths() {
        let list = DiscoveryRequest::List {
            refresh_id: "round".into(),
            config: json!({}),
            cursor: None,
        };
        let parse = |target| list.parse_reply(&provider(), json!({"targets":[target]}));
        let flat = json!({"id":"vm","label":"Build"});
        let Ok(DiscoveryReply::List(page)) = parse(flat.clone()) else {
            panic!("legacy target rejected")
        };
        assert_eq!(page.targets[0].id, "vm");
        assert!(page.targets[0].group_paths.is_empty());
        assert!(parse(json!({"id":"vm","label":"Build","detail":"host\nforged"})).is_err());
        for paths in [
            json!([[]]),
            json!([[{"id":"", "label":"A"}]]),
            json!([[{"id":"x","label":"bad\nlabel"}]]),
        ] {
            let mut target = flat.clone();
            target["groupPaths"] = paths;
            assert!(parse(target).is_err());
        }
    }

    #[test]
    fn discovery_open_requires_a_valid_endpoint_and_transport_specific_authority() {
        let request = DiscoveryRequest::PrepareOpen {
            instance_id: "instance".into(),
            target_id: "vm".into(),
            config: json!({}),
            continuation: None,
            option_id: None,
        };
        let definition = provider();
        assert_eq!(
            request.command_args("s", &definition).unwrap().1,
            json!({"providerId":"hosts","sourceId":"s","instanceId":"instance","targetId":"vm","config":{}})
        );
        let reply = |launch| {
            request.parse_reply(
                &definition,
                json!({"kind":"ready","owner":"host","launch":launch}),
            )
        };
        assert!(
            matches!(reply(json!({"kind":"ssh","endpoint":{"host":"vm.test","port":2222},"username":"ops@vm"})),Ok(DiscoveryReply::Open(DiscoveryOpen::Ready{launch:DiscoveryLaunch::Ssh{username:Some(user),allow_reuse_on_open:false,asset_session:false,..},..})) if user=="ops@vm")
        );
        assert!(matches!(
            reply(json!({"kind":"ssh","endpoint":{"host":"vm.test","port":2222},"username":"ops@asset","allowReuseOnOpen":true,"assetSession":true})),
            Ok(DiscoveryReply::Open(DiscoveryOpen::Ready {
                launch: DiscoveryLaunch::Ssh {
                    allow_reuse_on_open: true,
                    asset_session: true,
                    ..
                },
                ..
            }))
        ));
        for launch in [
            json!({"kind":"ssh","endpoint":{"host":"vm.test","port":22},"assetSession":true}),
            json!({"kind":"ssh","endpoint":{"host":"vm.test","port":22},"sessionIdentity":" "}),
            json!({"kind":"ssh","endpoint":{"host":"vm.test","port":22},"sessionIdentity":"a\nb"}),
            json!({"kind":"ssh","endpoint":null}),
            json!({"kind":"ssh"}),
            json!({"kind":"ssh","endpoint":{"host":"","port":22}}),
            json!({"kind":"ssh","endpoint":{"host":"vm.test","port":0}}),
            json!({"kind":"ssh","endpoint":{"host":"vm.test","port":65536}}),
            json!({"kind":"ssh","endpoint":{"host":"vm.test","port":22},"username":"ops\nother"}),
            json!({"kind":"terminal","args":["unregistered"]}),
        ] {
            assert!(matches!(reply(launch),Err(error) if error.code=="invalidDiscoveryMessage"));
        }
        let mut descriptor = provider();
        descriptor.launch_kinds.push(DiscoveryLaunchKind::Terminal);
        assert!(matches!(
            request.parse_reply(&descriptor, json!({"kind":"ready","owner":"provider","launch":{"kind":"terminal","args":[],"allowReuseOnOpen":true}})),
            Err(error) if error.code == "invalidDiscoveryMessage"
        ));
    }
}

impl DiscoveryRequest {
    pub fn command_args(
        &self,
        source: &str,
        provider: &NativePluginDiscoveryProviderDef,
    ) -> Result<(&'static str, Value), PluginError> {
        if !text_ok(source) {
            return Err(invalid());
        }
        let (name, mut args) = match self {
            Self::List {
                refresh_id,
                config,
                cursor,
            } if text_ok(refresh_id) => {
                let mut args = json!({"refreshId":refresh_id,"config":provider.config_for(source, config, DiscoveryConfigUse::Catalog)});
                if let Some(cursor) = cursor {
                    args["cursor"] = json!(cursor);
                }
                ("discovery.list", args)
            }
            Self::PrepareOpen {
                instance_id,
                target_id,
                config,
                continuation,
                option_id,
            } if text_ok(instance_id) && text_ok(target_id) => {
                let mut args = json!({"instanceId":instance_id,"targetId":target_id,"config":provider.config_for(source, config, DiscoveryConfigUse::Open)});
                match (continuation, option_id) {
                    (Some(token), Some(id)) if text_ok(token) && text_ok(id) => {
                        args["continuation"] = json!(token);
                        args["optionId"] = json!(id);
                    }
                    (None, None) => (),
                    _ => return Err(invalid()),
                }
                ("discovery.prepareOpen", args)
            }
            Self::SourceAction { action_id, config }
                if provider
                    .source_actions
                    .iter()
                    .any(|action| action.id == *action_id) =>
            {
                (
                    "discovery.sourceAction",
                    json!({"actionId":action_id,"config":provider.config_for(source, config, DiscoveryConfigUse::Both)}),
                )
            }
            Self::CancelRefresh { refresh_id } if text_ok(refresh_id) => {
                ("discovery.cancel", json!({"refreshId":refresh_id}))
            }
            Self::CancelOpen { instance_id } if text_ok(instance_id) => {
                ("discovery.cancel", json!({"instanceId":instance_id}))
            }
            Self::Close { instance_id } if text_ok(instance_id) => {
                ("discovery.close", json!({"instanceId":instance_id}))
            }
            _ => return Err(invalid()),
        };
        args["providerId"] = json!(provider.id);
        args["sourceId"] = json!(source);
        Ok((name, args))
    }

    pub fn parse_reply(
        &self,
        provider: &NativePluginDiscoveryProviderDef,
        value: Value,
    ) -> Result<DiscoveryReply, PluginError> {
        match self {
            Self::List { .. } => {
                let page: DiscoveryPage = decode(value)?;
                if page.next_cursor.as_deref().is_some_and(|v| !text_ok(v))
                    || page.targets.iter().any(|t| {
                        !text_ok(&t.id)
                            || !text_ok(&t.label)
                            || t.detail.as_deref().is_some_and(|s| !text_ok(s))
                            || t.scope.as_deref().is_some_and(|s| !text_ok(s))
                            || t.group_paths.iter().any(|path| {
                                path.is_empty()
                                    || path.len() > 64
                                    || path
                                        .iter()
                                        .any(|part| !text_ok(&part.id) || !text_ok(&part.label))
                            })
                            || t.tags.iter().any(|tag| !text_ok(tag))
                    })
                {
                    return Err(invalid());
                }
                Ok(DiscoveryReply::List(page))
            }
            Self::PrepareOpen { .. } => {
                if value
                    .pointer("/launch/username")
                    .is_some_and(|v| !v.is_string())
                {
                    return Err(invalid());
                }
                let open: DiscoveryOpen = decode(value)?;
                match &open {
                    DiscoveryOpen::Choose {
                        title_key,
                        continuation,
                        options,
                    } => {
                        let mut ids = HashSet::new();
                        if !text_ok(title_key)
                            || !text_ok(continuation)
                            || options.is_empty()
                            || options.iter().any(|o| {
                                !text_ok(&o.id) || !text_ok(&o.label) || !ids.insert(&o.id)
                            })
                        {
                            return Err(invalid());
                        }
                    }
                    DiscoveryOpen::Ready {
                        launch:
                            DiscoveryLaunch::Ssh {
                                username,
                                endpoint,
                                asset_session,
                                session_identity,
                                ..
                            },
                        owner
                    } => {
                        if !provider.launch_kinds.contains(&DiscoveryLaunchKind::Ssh)
                            || session_identity.as_deref().is_some_and(|id| !text_ok(id))
                            || (*asset_session
                                && (*owner != DiscoveryOwner::Host || username.is_none()))
                            || !host_ok(&endpoint.host)
                            || username.as_deref().is_some_and(|s| !username_ok(s))
                        {
                            return Err(invalid());
                        }
                    }
                    DiscoveryOpen::Ready {
                        launch: DiscoveryLaunch::Terminal { args },
                        ..
                    } => {
                        if !provider
                            .launch_kinds
                            .contains(&DiscoveryLaunchKind::Terminal)
                            || args.iter().any(|arg| arg.contains('\0'))
                        {
                            return Err(invalid());
                        }
                    }
                }
                Ok(DiscoveryReply::Open(open))
            }
            _ if value == json!({}) => Ok(DiscoveryReply::Done),
            _ => Err(invalid()),
        }
    }
}
