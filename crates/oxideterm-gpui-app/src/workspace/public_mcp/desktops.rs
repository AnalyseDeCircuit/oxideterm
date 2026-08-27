// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{AppContext, Context, Window};
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use oxideterm_connections::SecretString;
use oxideterm_public_mcp::{
    DesktopClipboardImageFormat, DesktopClipboardPayload, DesktopRef, DomainRequest,
    PublicToolCall, ToolEnvelope, ToolGroup,
};
use oxideterm_remote_desktop::{
    RemoteDesktopClipboardFormat, RemoteDesktopConnectionProfile, RemoteDesktopEndpoint,
    RemoteDesktopProtocol, RemoteDesktopSecret, RemoteDesktopSize, builtin_provider_registry,
};
use serde_json::json;
use zeroize::Zeroizing;

use super::{
    CONNECTION_KEY_DESKTOP_PREFIX, PublicMcpDesktopRecord, WorkspaceApp, finish_serialized,
};
use crate::workspace::{
    TabId,
    remote_desktop::{RemoteDesktopPublicClipboardSnapshot, RemoteDesktopSshTunnelLease},
};

const PUBLIC_MCP_DESKTOP_MAX_FRAME_PIXELS: u64 = 16_777_216;
const PUBLIC_MCP_DESKTOP_CLIPBOARD_IMAGE_LIMIT_BYTES: u64 = 16 * 1024 * 1024;
const PUBLIC_MCP_DESKTOP_CAPACITY: usize = 32;
const PUBLIC_MCP_DESKTOP_CAPACITY_PER_CLIENT: usize = 8;

pub(in crate::workspace) enum PublicMcpDesktopWindowEffect {
    Open(DomainRequest),
    Reconnect(DomainRequest),
    Close(DomainRequest),
    Revoke(Vec<TabId>),
}

impl PublicMcpDesktopWindowEffect {
    pub(in crate::workspace) fn finish_without_window(self) {
        let request = match self {
            Self::Open(request) | Self::Reconnect(request) | Self::Close(request) => Some(request),
            Self::Revoke(_) => None,
        };
        if let Some(request) = request {
            request.finish(ToolEnvelope::failed(
                "A live OxideTerm window is required for remote desktop sessions",
            ));
        }
    }
}

impl WorkspaceApp {
    pub(super) fn handle_public_mcp_desktop_open(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        if request.is_cancelled() {
            return;
        }
        self.enqueue_public_mcp_desktop_window_effect(
            PublicMcpDesktopWindowEffect::Open(request),
            cx,
        );
    }

    pub(super) fn handle_public_mcp_desktop_state(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::DesktopState(args) = &request.call else {
            return;
        };
        match self.public_mcp_desktop_projection(&request.client_ref, &args.desktop_ref, cx) {
            Ok(desktop) => finish_serialized(request, json!({ "desktop": desktop })),
            Err(error) => request.finish(ToolEnvelope::failed(error)),
        }
    }

    pub(super) fn handle_public_mcp_desktop_frame(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::DesktopFrame(args) = &request.call else {
            return;
        };
        let desktop_ref = args.desktop_ref.clone();
        let after_generation = args.after_generation;
        let Ok((record, session)) =
            self.public_mcp_desktop_session(&request.client_ref, &desktop_ref, cx)
        else {
            request.finish(ToolEnvelope::failed(
                "The remote desktop handle is unavailable",
            ));
            return;
        };
        let Some(frame) = session.read(cx).public_mcp_frame_snapshot() else {
            request.finish(ToolEnvelope::failed(
                "The remote desktop has not produced a framebuffer",
            ));
            return;
        };
        if after_generation == Some(frame.generation) {
            finish_serialized(
                request,
                json!({
                    "desktop_ref": desktop_ref,
                    "generation": frame.generation,
                    "graphics_epoch": frame.graphics_epoch,
                    "unchanged": true,
                }),
            );
            return;
        }
        let pixel_count = u64::from(frame.size.width).saturating_mul(u64::from(frame.size.height));
        if pixel_count > PUBLIC_MCP_DESKTOP_MAX_FRAME_PIXELS {
            request.finish(ToolEnvelope::failed(
                "The remote framebuffer exceeds the supported artifact dimensions",
            ));
            return;
        }
        let artifact_store = self.public_mcp.state.artifacts.clone();
        let client_ref = request.client_ref.clone();
        let artifact_client_ref = client_ref.clone();
        let width = frame.size.width;
        let height = frame.size.height;
        let generation = frame.generation;
        let graphics_epoch = frame.graphics_epoch;
        let encode_task = cx.background_executor().spawn(async move {
            let png = encode_public_mcp_desktop_png(frame)?;
            artifact_store
                .stage(
                    artifact_client_ref,
                    &png,
                    "image/png".to_owned(),
                    Some(format!("remote-desktop-{generation}.png")),
                )
                .map_err(|error| error.to_string())
        });
        cx.spawn(async move |workspace, cx| {
            let result = encode_task.await;
            let _ = workspace.update(cx, move |workspace, _cx| match result {
                Ok(artifact) => {
                    let artifact_store = workspace.public_mcp.state.artifacts.clone();
                    let artifact_authorized = workspace
                        .public_mcp
                        .clients()
                        .into_iter()
                        .find(|client| client.client_ref == client_ref)
                        .is_some_and(|client| {
                            client.enabled
                                && client.tool_groups.contains(&ToolGroup::DesktopObserve)
                                && client.tool_groups.contains(&ToolGroup::ArtifactTransfer)
                        });
                    let live =
                        workspace
                            .public_mcp
                            .desktops
                            .get_mut(&desktop_ref)
                            .filter(|current| {
                                current.client_ref == client_ref
                                    && current.tab_id == record.tab_id
                                    && current.observing_frames
                            });
                    if request.is_cancelled() || !artifact_authorized || live.is_none() {
                        workspace
                            .public_mcp
                            .state
                            .artifacts
                            .revoke(&client_ref, &artifact.artifact_ref);
                        if !request.is_cancelled() {
                            let error = if artifact_authorized {
                                "The remote desktop handle is no longer available"
                            } else {
                                "The remote desktop artifact authorization changed"
                            };
                            request.finish(ToolEnvelope::failed(error));
                        }
                        return;
                    }
                    if let Some(live) = live {
                        live.frame_artifacts.retain(|artifact_ref| {
                            artifact_store.is_available(&client_ref, artifact_ref)
                        });
                        live.frame_artifacts.insert(artifact.artifact_ref.clone());
                    }
                    finish_serialized(
                        request,
                        json!({
                            "desktop_ref": desktop_ref,
                            "generation": generation,
                            "graphics_epoch": graphics_epoch,
                            "width": width,
                            "height": height,
                            "unchanged": false,
                            "artifact": artifact,
                        }),
                    );
                }
                Err(error) => request.finish(ToolEnvelope::failed(error)),
            });
        })
        .detach();
    }

    pub(super) fn handle_public_mcp_desktop_input(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::DesktopInput(args) = &request.call else {
            return;
        };
        let desktop_ref = args.desktop_ref.clone();
        let Ok((_, session)) =
            self.public_mcp_desktop_session(&request.client_ref, &desktop_ref, cx)
        else {
            request.finish(ToolEnvelope::failed(
                "The remote desktop handle is unavailable",
            ));
            return;
        };
        let result = session.update(cx, |session, _cx| {
            session.apply_public_mcp_input(args.graphics_epoch, &args.event)
        });
        match result {
            Ok(()) => finish_serialized(
                request,
                json!({ "desktop_ref": desktop_ref, "accepted": true }),
            ),
            Err(error) => request.finish(ToolEnvelope::failed(error)),
        }
    }

    pub(super) fn handle_public_mcp_desktop_resize(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::ResizeDesktop(args) = &request.call else {
            return;
        };
        let desktop_ref = args.desktop_ref.clone();
        let size = RemoteDesktopSize {
            width: args.width,
            height: args.height,
        };
        let Ok((_, session)) =
            self.public_mcp_desktop_session(&request.client_ref, &desktop_ref, cx)
        else {
            request.finish(ToolEnvelope::failed(
                "The remote desktop handle is unavailable",
            ));
            return;
        };
        match session.update(cx, |session, _cx| session.apply_public_mcp_resize(size)) {
            Ok(()) => finish_serialized(
                request,
                json!({ "desktop_ref": desktop_ref, "resize_requested": true }),
            ),
            Err(error) => request.finish(ToolEnvelope::failed(error)),
        }
    }

    pub(super) fn handle_public_mcp_desktop_clipboard_read(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::ReadDesktopClipboard(args) = &request.call else {
            return;
        };
        let desktop_ref = args.desktop_ref.clone();
        let Ok((_, session)) =
            self.public_mcp_desktop_session(&request.client_ref, &desktop_ref, cx)
        else {
            request.finish(ToolEnvelope::failed(
                "The remote desktop handle is unavailable",
            ));
            return;
        };
        match session.read(cx).public_mcp_clipboard_snapshot(args.kind) {
            Ok(RemoteDesktopPublicClipboardSnapshot::Text(text)) => finish_serialized(
                request,
                json!({ "desktop_ref": desktop_ref, "kind": "text", "text": text.as_str() }),
            ),
            Ok(RemoteDesktopPublicClipboardSnapshot::Image { format, bytes }) => {
                let media_type = remote_desktop_clipboard_media_type(format).to_owned();
                let artifact_store = self.public_mcp.state.artifacts.clone();
                match artifact_store.stage(
                    request.client_ref.clone(),
                    &bytes,
                    media_type,
                    Some("remote-clipboard".to_owned()),
                ) {
                    Ok(artifact) => {
                        if let Some(record) = self.public_mcp.desktops.get_mut(&desktop_ref) {
                            record.clipboard_artifacts.retain(|artifact_ref| {
                                artifact_store.is_available(&request.client_ref, artifact_ref)
                            });
                            record
                                .clipboard_artifacts
                                .insert(artifact.artifact_ref.clone());
                        }
                        finish_serialized(
                            request,
                            json!({
                                "desktop_ref": desktop_ref,
                                "kind": "image",
                                "format": remote_desktop_clipboard_format_name(format),
                                "artifact": artifact,
                            }),
                        );
                    }
                    Err(error) => request.finish(ToolEnvelope::failed(error.to_string())),
                }
            }
            Err(error) => request.finish(ToolEnvelope::failed(error)),
        }
    }

    pub(super) fn handle_public_mcp_desktop_clipboard_write(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::WriteDesktopClipboard(args) = &request.call else {
            return;
        };
        let desktop_ref = args.desktop_ref.clone();
        let Ok((_, session)) =
            self.public_mcp_desktop_session(&request.client_ref, &desktop_ref, cx)
        else {
            request.finish(ToolEnvelope::failed(
                "The remote desktop handle is unavailable",
            ));
            return;
        };
        let result = match &args.payload {
            DesktopClipboardPayload::Text { text } => session.update(cx, |session, _cx| {
                session.write_public_mcp_clipboard_text(text)
            }),
            DesktopClipboardPayload::Image {
                artifact_ref,
                format,
            } => {
                let expected_media_type = public_desktop_clipboard_media_type(*format);
                match self.public_mcp.state.artifacts.read_all(
                    &request.client_ref,
                    artifact_ref,
                    PUBLIC_MCP_DESKTOP_CLIPBOARD_IMAGE_LIMIT_BYTES,
                ) {
                    Ok(content) if content.projection.media_type == expected_media_type => {
                        let format = public_desktop_clipboard_format(*format);
                        session.update(cx, |session, _cx| {
                            session.write_public_mcp_clipboard_image(format, &content.bytes)
                        })
                    }
                    Ok(_) => Err("The clipboard artifact media type does not match format".into()),
                    Err(error) => Err(error.to_string()),
                }
            }
        };
        match result {
            Ok(()) => finish_serialized(
                request,
                json!({ "desktop_ref": desktop_ref, "accepted": true }),
            ),
            Err(error) => request.finish(ToolEnvelope::failed(error)),
        }
    }

    pub(super) fn handle_public_mcp_desktop_reconnect(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        self.enqueue_public_mcp_desktop_window_effect(
            PublicMcpDesktopWindowEffect::Reconnect(request),
            cx,
        );
    }

    pub(super) fn handle_public_mcp_desktop_close(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        self.enqueue_public_mcp_desktop_window_effect(
            PublicMcpDesktopWindowEffect::Close(request),
            cx,
        );
    }

    pub(in crate::workspace) fn apply_public_mcp_desktop_window_effect(
        &mut self,
        effect: PublicMcpDesktopWindowEffect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match effect {
            PublicMcpDesktopWindowEffect::Open(request) => {
                self.apply_public_mcp_desktop_open(request, window, cx)
            }
            PublicMcpDesktopWindowEffect::Reconnect(request) => {
                self.apply_public_mcp_desktop_reconnect(request, window, cx)
            }
            PublicMcpDesktopWindowEffect::Close(request) => {
                self.apply_public_mcp_desktop_close(request, window, cx)
            }
            PublicMcpDesktopWindowEffect::Revoke(tab_ids) => {
                for tab_id in tab_ids {
                    self.close_tab_by_id(tab_id, window, cx);
                }
            }
        }
    }

    pub(super) fn revoke_public_mcp_client_desktops(
        &mut self,
        client_ref: &oxideterm_public_mcp::ClientRef,
        cx: &mut Context<Self>,
    ) {
        let desktop_refs = self
            .public_mcp
            .desktops
            .iter()
            .filter_map(|(desktop_ref, record)| {
                (&record.client_ref == client_ref).then_some(desktop_ref.clone())
            })
            .collect::<Vec<_>>();
        let records = desktop_refs
            .into_iter()
            .filter_map(|desktop_ref| self.public_mcp.desktops.remove(&desktop_ref))
            .collect::<Vec<_>>();
        for record in &records {
            for artifact_ref in record
                .frame_artifacts
                .iter()
                .chain(record.clipboard_artifacts.iter())
            {
                self.public_mcp
                    .state
                    .artifacts
                    .revoke(client_ref, artifact_ref);
            }
            if record.observing_frames
                && let Some(session) = self.remote_desktop_session_entity(record.tab_id, cx)
            {
                session.update(cx, |session, cx| {
                    session.detach_public_mcp_frame_observer(cx)
                });
            }
        }
        let tab_ids = records
            .into_iter()
            .map(|record| record.tab_id)
            .collect::<Vec<_>>();
        if !tab_ids.is_empty() {
            let _ = self.enqueue_public_mcp_desktop_window_effect(
                PublicMcpDesktopWindowEffect::Revoke(tab_ids),
                cx,
            );
        }
    }

    pub(super) fn set_public_mcp_client_desktop_observation(
        &mut self,
        client_ref: &oxideterm_public_mcp::ClientRef,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let mut tab_ids = Vec::new();
        let mut revoked_artifacts = Vec::new();
        for record in self
            .public_mcp
            .desktops
            .values_mut()
            .filter(|record| &record.client_ref == client_ref)
        {
            if record.observing_frames == enabled {
                continue;
            }
            record.observing_frames = enabled;
            tab_ids.push(record.tab_id);
            if !enabled {
                revoked_artifacts.extend(record.frame_artifacts.drain());
            }
        }
        for artifact_ref in revoked_artifacts {
            self.public_mcp
                .state
                .artifacts
                .revoke(client_ref, &artifact_ref);
        }
        for tab_id in tab_ids {
            if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
                session.update(cx, |session, cx| {
                    if enabled {
                        session.attach_public_mcp_frame_observer(cx);
                    } else {
                        session.detach_public_mcp_frame_observer(cx);
                    }
                });
            }
        }
    }

    pub(super) fn release_public_mcp_client_desktop_inputs(
        &self,
        client_ref: &oxideterm_public_mcp::ClientRef,
        cx: &mut Context<Self>,
    ) {
        let tab_ids = self
            .public_mcp
            .desktops
            .values()
            .filter_map(|record| (&record.client_ref == client_ref).then_some(record.tab_id))
            .collect::<Vec<_>>();
        for tab_id in tab_ids {
            if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
                session.update(cx, |session, _cx| session.release_public_mcp_inputs());
            }
        }
    }

    pub(super) fn revoke_public_mcp_client_desktop_clipboard_content(
        &mut self,
        client_ref: &oxideterm_public_mcp::ClientRef,
        cx: &mut Context<Self>,
    ) {
        let mut tab_ids = Vec::new();
        let mut artifacts = Vec::new();
        for record in self
            .public_mcp
            .desktops
            .values_mut()
            .filter(|record| &record.client_ref == client_ref)
        {
            tab_ids.push(record.tab_id);
            artifacts.extend(record.clipboard_artifacts.drain());
        }
        for artifact_ref in artifacts {
            self.public_mcp
                .state
                .artifacts
                .revoke(client_ref, &artifact_ref);
        }
        for tab_id in tab_ids {
            if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
                session.update(cx, |session, _cx| session.clear_public_mcp_clipboard());
            }
        }
    }

    pub(in crate::workspace) fn release_public_mcp_desktop_for_closed_tab(
        &mut self,
        tab_id: TabId,
    ) {
        let desktop_refs = self
            .public_mcp
            .desktops
            .iter()
            .filter_map(|(desktop_ref, record)| {
                (record.tab_id == tab_id).then_some(desktop_ref.clone())
            })
            .collect::<Vec<_>>();
        for desktop_ref in desktop_refs {
            let Some(record) = self.public_mcp.desktops.remove(&desktop_ref) else {
                continue;
            };
            // The visible tab owns this helper session, so a UI close also revokes
            // its public artifacts and handle without exposing the internal TabId.
            for artifact_ref in record
                .frame_artifacts
                .iter()
                .chain(record.clipboard_artifacts.iter())
            {
                self.public_mcp
                    .state
                    .artifacts
                    .revoke(&record.client_ref, artifact_ref);
            }
        }
    }

    fn apply_public_mcp_desktop_open(
        &mut self,
        request: DomainRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if request.is_cancelled() {
            return;
        }
        if let Some(error) = self.public_mcp_desktop_open_rejection(&request) {
            request.finish(ToolEnvelope::failed(error));
            return;
        }
        let PublicToolCall::OpenDesktop(args) = &request.call else {
            return;
        };
        let connection_ref = args.connection_ref.clone();
        let Some(connection_key) = self.public_mcp.connection_key(
            &request.client_ref,
            &connection_ref,
            &self.connection_store,
        ) else {
            request.finish(ToolEnvelope::failed("The saved connection is unavailable"));
            return;
        };
        let Some(profile_id) = connection_key.strip_prefix(CONNECTION_KEY_DESKTOP_PREFIX) else {
            request.finish(ToolEnvelope::failed(
                "The saved connection is not an RDP or VNC profile",
            ));
            return;
        };
        let Some(saved) = self
            .connection_store
            .get_remote_desktop_profile(profile_id)
            .cloned()
        else {
            request.finish(ToolEnvelope::failed(
                "The saved remote desktop profile is unavailable",
            ));
            return;
        };
        let ssh_gateway_connection_id = saved.ssh_gateway_connection_id.clone();
        let password = match self
            .connection_store
            .get_remote_desktop_credential(profile_id)
        {
            Ok(secret) => secret
                .map(SecretString::into_zeroizing)
                .map(RemoteDesktopSecret::from),
            Err(_) => {
                request.finish(ToolEnvelope::failed(
                    "The saved remote desktop credential is unavailable",
                ));
                return;
            }
        };
        if saved.protocol == RemoteDesktopProtocol::Rdp && password.is_none() {
            request.finish(ToolEnvelope::failed(
                "This RDP profile requires a device-local credential",
            ));
            return;
        }
        let mut profile = RemoteDesktopConnectionProfile {
            id: saved.id.clone(),
            label: saved.name.clone(),
            protocol: saved.protocol,
            endpoint: RemoteDesktopEndpoint::new(saved.host, saved.port),
            transport_endpoint: None,
            username: saved.username,
            domain: saved.domain,
            credential_ref: saved.credential_ref,
            read_only: saved.read_only,
            session_options: saved.session_options,
        };
        if let Some(ssh_gateway_connection_id) = ssh_gateway_connection_id {
            let pending_tunnel = match self.start_remote_desktop_ssh_tunnel(
                ssh_gateway_connection_id,
                profile.endpoint.clone(),
                cx,
            ) {
                Ok(pending_tunnel) => pending_tunnel,
                Err(error) => {
                    request.finish(ToolEnvelope::failed(error));
                    return;
                }
            };
            let window_handle = window.window_handle();
            cx.spawn(
                async move |workspace, cx| match pending_tunnel.finish().await {
                    Ok((transport_endpoint, lease)) => {
                        profile.transport_endpoint = Some(transport_endpoint);
                        let _ = cx.update_window(window_handle, move |_, window, cx| {
                            let _ = workspace.update(cx, |workspace, cx| {
                                workspace.finish_public_mcp_desktop_open(
                                    request,
                                    profile,
                                    password,
                                    Some(lease),
                                    window,
                                    cx,
                                );
                            });
                        });
                    }
                    Err(error) => request.finish(ToolEnvelope::failed(error)),
                },
            )
            .detach();
            return;
        }
        self.finish_public_mcp_desktop_open(request, profile, password, None, window, cx);
    }

    fn public_mcp_desktop_open_rejection(&self, request: &DomainRequest) -> Option<&'static str> {
        let session_group_enabled = self
            .public_mcp
            .clients()
            .into_iter()
            .find(|client| client.client_ref == request.client_ref)
            .is_some_and(|client| {
                client.enabled && client.tool_groups.contains(&ToolGroup::DesktopSession)
            });
        if !session_group_enabled {
            return Some("The MCP client authorization changed before the desktop opened");
        }
        let client_session_count = self
            .public_mcp
            .desktops
            .values()
            .filter(|record| record.client_ref == request.client_ref)
            .count();
        (self.public_mcp.desktops.len() >= PUBLIC_MCP_DESKTOP_CAPACITY
            || client_session_count >= PUBLIC_MCP_DESKTOP_CAPACITY_PER_CLIENT)
            .then_some("The retained remote desktop session limit has been reached")
    }

    fn finish_public_mcp_desktop_open(
        &mut self,
        request: DomainRequest,
        profile: RemoteDesktopConnectionProfile,
        password: Option<RemoteDesktopSecret>,
        ssh_tunnel: Option<RemoteDesktopSshTunnelLease>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if request.is_cancelled() {
            return;
        }
        if let Some(error) = self.public_mcp_desktop_open_rejection(&request) {
            request.finish(ToolEnvelope::failed(error));
            return;
        }
        let provider = match builtin_provider_registry()
            .ok()
            .and_then(|registry| registry.get_for_protocol(profile.protocol).cloned())
        {
            Some(provider) => provider,
            None => {
                request.finish(ToolEnvelope::failed(
                    "The remote desktop provider is unavailable",
                ));
                return;
            }
        };
        let title = profile.label.clone();
        let saved_profile_id = profile.id.clone();
        let tab_id = self.open_remote_desktop_tab_with_tunnel(
            profile,
            provider,
            title.clone(),
            password,
            ssh_tunnel,
            window,
            cx,
        );
        let Some(session) = self.remote_desktop_session_entity(tab_id, cx) else {
            self.close_tab_by_id(tab_id, window, cx);
            request.finish(ToolEnvelope::failed(
                "The remote desktop session did not register",
            ));
            return;
        };
        let observing_frames = self
            .public_mcp
            .clients()
            .into_iter()
            .find(|client| client.client_ref == request.client_ref)
            .is_some_and(|client| client.tool_groups.contains(&ToolGroup::DesktopObserve));
        if observing_frames {
            session.update(cx, |session, cx| {
                session.attach_public_mcp_frame_observer(cx)
            });
        }
        let desktop_ref = DesktopRef::new();
        self.public_mcp.desktops.insert(
            desktop_ref.clone(),
            PublicMcpDesktopRecord {
                client_ref: request.client_ref.clone(),
                tab_id,
                title,
                observing_frames,
                frame_artifacts: Default::default(),
                clipboard_artifacts: Default::default(),
            },
        );
        let _ = self
            .connection_store
            .mark_remote_desktop_profile_used(&saved_profile_id);
        self.queue_cloud_sync_dirty_refresh(cx);
        match self.public_mcp_desktop_projection(&request.client_ref, &desktop_ref, cx) {
            Ok(desktop) => finish_serialized(request, json!({ "desktop": desktop })),
            Err(error) => request.finish(ToolEnvelope::failed(error)),
        }
    }

    fn apply_public_mcp_desktop_reconnect(
        &mut self,
        request: DomainRequest,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        let PublicToolCall::ReconnectDesktop(_) = &request.call else {
            return;
        };
        request.finish(ToolEnvelope::failed(
            "Remote desktop reconnect creates a new tab and must be requested from Active Sessions",
        ));
    }

    fn apply_public_mcp_desktop_close(
        &mut self,
        request: DomainRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::CloseDesktop(args) = &request.call else {
            return;
        };
        let desktop_ref = args.desktop_ref.clone();
        let Some(record) = self
            .public_mcp
            .desktops
            .get(&desktop_ref)
            .filter(|record| record.client_ref == request.client_ref)
            .cloned()
        else {
            request.finish(ToolEnvelope::failed(
                "The remote desktop handle is unavailable",
            ));
            return;
        };
        self.public_mcp.desktops.remove(&desktop_ref);
        for artifact_ref in record
            .frame_artifacts
            .iter()
            .chain(record.clipboard_artifacts.iter())
        {
            self.public_mcp
                .state
                .artifacts
                .revoke(&request.client_ref, artifact_ref);
        }
        if record.observing_frames
            && let Some(session) = self.remote_desktop_session_entity(record.tab_id, cx)
        {
            session.update(cx, |session, cx| {
                session.detach_public_mcp_frame_observer(cx)
            });
        }
        self.close_tab_by_id(record.tab_id, window, cx);
        finish_serialized(
            request,
            json!({ "desktop_ref": desktop_ref, "close_requested": true }),
        );
    }

    fn public_mcp_desktop_projection(
        &self,
        client_ref: &oxideterm_public_mcp::ClientRef,
        desktop_ref: &DesktopRef,
        cx: &gpui::App,
    ) -> Result<serde_json::Value, String> {
        let (record, session) = self.public_mcp_desktop_session(client_ref, desktop_ref, cx)?;
        let state = session.read(cx).public_mcp_state_projection();
        Ok(json!({
            "desktop_ref": desktop_ref,
            "title": record.title,
            "state": state,
        }))
    }

    fn public_mcp_desktop_session(
        &self,
        client_ref: &oxideterm_public_mcp::ClientRef,
        desktop_ref: &DesktopRef,
        cx: &gpui::App,
    ) -> Result<
        (
            PublicMcpDesktopRecord,
            gpui::Entity<crate::workspace::remote_desktop::RemoteDesktopSessionEntity>,
        ),
        String,
    > {
        let record = self
            .public_mcp
            .desktops
            .get(desktop_ref)
            .filter(|record| &record.client_ref == client_ref)
            .cloned()
            .ok_or_else(|| "The remote desktop handle is unavailable".to_owned())?;
        let session = self
            .remote_desktop_session_entity(record.tab_id, cx)
            .ok_or_else(|| "The remote desktop session is no longer live".to_owned())?;
        Ok((record, session))
    }
}

fn encode_public_mcp_desktop_png(
    frame: oxideterm_gpui_remote_desktop::RemoteDesktopFrameSnapshot,
) -> Result<Zeroizing<Vec<u8>>, String> {
    let mut rgba = Zeroizing::new(frame.bgra_bytes);
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let mut png = Zeroizing::new(Vec::new());
    PngEncoder::new(&mut *png)
        .write_image(
            &rgba,
            frame.size.width,
            frame.size.height,
            ExtendedColorType::Rgba8,
        )
        .map_err(|_| "The remote framebuffer could not be encoded".to_owned())?;
    Ok(png)
}

fn public_desktop_clipboard_format(
    format: DesktopClipboardImageFormat,
) -> RemoteDesktopClipboardFormat {
    match format {
        DesktopClipboardImageFormat::Png => RemoteDesktopClipboardFormat::ImagePng,
        DesktopClipboardImageFormat::Jpeg => RemoteDesktopClipboardFormat::ImageJpeg,
        DesktopClipboardImageFormat::Webp => RemoteDesktopClipboardFormat::ImageWebp,
        DesktopClipboardImageFormat::Gif => RemoteDesktopClipboardFormat::ImageGif,
        DesktopClipboardImageFormat::Svg => RemoteDesktopClipboardFormat::ImageSvg,
        DesktopClipboardImageFormat::Bmp => RemoteDesktopClipboardFormat::ImageBmp,
        DesktopClipboardImageFormat::Tiff => RemoteDesktopClipboardFormat::ImageTiff,
    }
}

fn public_desktop_clipboard_media_type(format: DesktopClipboardImageFormat) -> &'static str {
    remote_desktop_clipboard_media_type(public_desktop_clipboard_format(format))
}

fn remote_desktop_clipboard_format_name(format: RemoteDesktopClipboardFormat) -> &'static str {
    match format {
        RemoteDesktopClipboardFormat::ImagePng => "png",
        RemoteDesktopClipboardFormat::ImageJpeg => "jpeg",
        RemoteDesktopClipboardFormat::ImageWebp => "webp",
        RemoteDesktopClipboardFormat::ImageGif => "gif",
        RemoteDesktopClipboardFormat::ImageSvg => "svg",
        RemoteDesktopClipboardFormat::ImageBmp => "bmp",
        RemoteDesktopClipboardFormat::ImageTiff => "tiff",
    }
}

fn remote_desktop_clipboard_media_type(format: RemoteDesktopClipboardFormat) -> &'static str {
    match format {
        RemoteDesktopClipboardFormat::ImagePng => "image/png",
        RemoteDesktopClipboardFormat::ImageJpeg => "image/jpeg",
        RemoteDesktopClipboardFormat::ImageWebp => "image/webp",
        RemoteDesktopClipboardFormat::ImageGif => "image/gif",
        RemoteDesktopClipboardFormat::ImageSvg => "image/svg+xml",
        RemoteDesktopClipboardFormat::ImageBmp => "image/bmp",
        RemoteDesktopClipboardFormat::ImageTiff => "image/tiff",
    }
}
