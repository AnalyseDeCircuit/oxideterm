// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{AppContext, Context, Window};
use oxideterm_public_mcp::{
    DesktopClipboardPayload, DesktopRef, DomainRequest, PublicToolCall, ToolEnvelope, ToolGroup,
};
use oxideterm_remote_desktop::{
    RemoteDesktopConnectionProfile, RemoteDesktopSecret, RemoteDesktopSize,
};
use serde_json::json;

use super::{PublicMcpDesktopRecord, WorkspaceApp, finish_serialized};
use crate::workspace::{
    TabId,
    remote_desktop::{RemoteDesktopPublicClipboardSnapshot, RemoteDesktopSshTunnelLease},
};

use oxideterm_public_mcp_host::{
    PublicMcpClipboardSnapshot, PublicMcpDesktopFrame, PublicMcpDesktopOpen,
};

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
        self.public_mcp.background().desktop_frame(
            request,
            record.tab_id,
            PublicMcpDesktopFrame {
                size: frame.size,
                generation: frame.generation,
                graphics_epoch: frame.graphics_epoch,
                bgra_bytes: zeroize::Zeroizing::new(frame.bgra_bytes),
            },
            session.read(cx).audit_context(),
        );
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
        let Ok((record, session)) =
            self.public_mcp_desktop_session(&request.client_ref, &desktop_ref, cx)
        else {
            request.finish(ToolEnvelope::failed(
                "The remote desktop handle is unavailable",
            ));
            return;
        };
        let session = session.read(cx);
        let snapshot = match session.public_mcp_clipboard_snapshot(args.kind) {
            Ok(RemoteDesktopPublicClipboardSnapshot::Text(text)) => {
                PublicMcpClipboardSnapshot::Text(text)
            }
            Ok(RemoteDesktopPublicClipboardSnapshot::Image { format, bytes }) => {
                PublicMcpClipboardSnapshot::Image { format, bytes }
            }
            Err(error) => {
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        self.public_mcp.background().desktop_clipboard_read(
            request,
            record.tab_id,
            snapshot,
            session.audit_context(),
        );
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
                match self.public_mcp.desktop_clipboard_image(
                    &request.client_ref,
                    artifact_ref,
                    *format,
                ) {
                    Ok((format, bytes)) => session.update(cx, |session, _cx| {
                        session.write_public_mcp_clipboard_image(format, &bytes)
                    }),
                    Err(error) => Err(error),
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
                let audit_context = request.audit_context.clone();
                oxideterm_audit::AuditContext::with_sync_request(audit_context.as_ref(), || {
                    self.apply_public_mcp_desktop_open(request, window, cx)
                })
            }
            PublicMcpDesktopWindowEffect::Reconnect(request) => {
                let audit_context = request.audit_context.clone();
                oxideterm_audit::AuditContext::with_sync_request(audit_context.as_ref(), || {
                    self.apply_public_mcp_desktop_reconnect(request, window, cx)
                })
            }
            PublicMcpDesktopWindowEffect::Close(request) => {
                let audit_context = request.audit_context.clone();
                oxideterm_audit::AuditContext::with_sync_request(audit_context.as_ref(), || {
                    self.apply_public_mcp_desktop_close(request, window, cx)
                })
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
        let records = self.public_mcp.remove_client_desktops(client_ref);
        for record in &records {
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
        for tab_id in self.public_mcp.set_desktop_observation(client_ref, enabled) {
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
        for tab_id in self.public_mcp.client_desktop_tabs(client_ref) {
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
        for tab_id in self.public_mcp.revoke_desktop_clipboard_content(client_ref) {
            if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
                session.update(cx, |session, _cx| session.clear_public_mcp_clipboard());
            }
        }
    }

    pub(in crate::workspace) fn release_public_mcp_desktop_for_closed_tab(
        &mut self,
        tab_id: TabId,
    ) {
        self.public_mcp.remove_closed_desktop(tab_id);
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
        if let Some(error) = self.public_mcp.desktop_open_rejection(&request.client_ref) {
            request.finish(ToolEnvelope::failed(error));
            return;
        }
        let PublicToolCall::OpenDesktop(args) = &request.call else {
            return;
        };
        let PublicMcpDesktopOpen {
            mut profile,
            password,
            ssh_gateway_connection_id,
        } = match self.public_mcp.desktop_profile(
            &request.client_ref,
            &args.connection_ref,
            &self.connection_store,
        ) {
            Ok(profile) => profile,
            Err(error) => {
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        if let Some(ssh_gateway_connection_id) = ssh_gateway_connection_id {
            let pending_tunnel =
                match self.start_remote_desktop_ssh_tunnel(ssh_gateway_connection_id, &profile, cx)
                {
                    Ok(pending_tunnel) => pending_tunnel,
                    Err(error) => {
                        request.finish(ToolEnvelope::failed(error));
                        return;
                    }
                };
            let window_handle = window.window_handle();
            let audit_context = request.audit_context.clone();
            let tunnel_context = audit_context.clone();
            cx.spawn(
                async move |workspace, cx| match oxideterm_audit::AuditContext::scope_optional(
                    tunnel_context,
                    pending_tunnel.finish(),
                )
                .await
                {
                    Ok((transport_endpoint, lease)) => {
                        profile.transport_endpoint = Some(transport_endpoint);
                        let _ = cx.update_window(window_handle, move |_, window, cx| {
                            let _ = workspace.update(cx, |workspace, cx| {
                                oxideterm_audit::AuditContext::with_sync_request(
                                    audit_context.as_ref(),
                                    || {
                                        workspace.finish_public_mcp_desktop_open(
                                            request,
                                            profile,
                                            password,
                                            Some(lease),
                                            window,
                                            cx,
                                        );
                                    },
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
        if let Some(error) = self.public_mcp.desktop_open_rejection(&request.client_ref) {
            request.finish(ToolEnvelope::failed(error));
            return;
        }
        let provider = match self.remote_desktop_provider(profile.protocol, cx) {
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
        self.public_mcp.register_desktop(
            desktop_ref.clone(),
            request.client_ref.clone(),
            tab_id,
            title,
            observing_frames,
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
            .remove_desktop(&request.client_ref, &desktop_ref)
        else {
            request.finish(ToolEnvelope::failed(
                "The remote desktop handle is unavailable",
            ));
            return;
        };
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
            .desktop(client_ref, desktop_ref)
            .ok_or_else(|| "The remote desktop handle is unavailable".to_owned())?;
        let session = self
            .remote_desktop_session_entity(record.tab_id, cx)
            .ok_or_else(|| "The remote desktop session is no longer live".to_owned())?;
        Ok((record, session))
    }
}
