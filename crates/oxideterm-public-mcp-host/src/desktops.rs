use super::*;
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use oxideterm_audit::{
    AuditAuthorization, AuditCategory, AuditContext, AuditEvidence, AuditOutcome, AuditSource,
};
use oxideterm_connections::SecretString;
use oxideterm_public_mcp::{DesktopClipboardImageFormat, artifact::ArtifactProjection};
use oxideterm_remote_desktop::{
    RemoteDesktopClipboardFormat, RemoteDesktopConnectionProfile, RemoteDesktopEndpoint,
    RemoteDesktopProtocol, RemoteDesktopSecret, RemoteDesktopSize,
};

use oxideterm_public_mcp::calls::DESKTOP_MAX_FRAME_PIXELS;
const PUBLIC_MCP_DESKTOP_CLIPBOARD_IMAGE_LIMIT_BYTES: u64 = 16 * 1024 * 1024;
const PUBLIC_MCP_DESKTOP_CAPACITY: usize = 32;
const PUBLIC_MCP_DESKTOP_CAPACITY_PER_CLIENT: usize = 8;

pub struct PublicMcpDesktopFrame {
    pub size: RemoteDesktopSize,
    pub generation: u64,
    pub graphics_epoch: u64,
    pub bgra_bytes: Zeroizing<Vec<u8>>,
}

pub enum PublicMcpClipboardSnapshot {
    Text(Zeroizing<String>),
    Image {
        format: RemoteDesktopClipboardFormat,
        bytes: Zeroizing<Vec<u8>>,
    },
}

pub struct PublicMcpDesktopOpen {
    pub profile: RemoteDesktopConnectionProfile,
    pub password: Option<RemoteDesktopSecret>,
    pub ssh_gateway_connection_id: Option<String>,
}

impl PublicMcpHost {
    pub fn desktop_profile(
        &mut self,
        client: &ClientRef,
        reference: &ConnectionRef,
        store: &ConnectionStore,
    ) -> Result<PublicMcpDesktopOpen, &'static str> {
        let key = self
            .connection_key(client, reference, store)
            .ok_or("The saved connection is unavailable")?;
        let profile_id = key
            .strip_prefix(CONNECTION_KEY_DESKTOP_PREFIX)
            .ok_or("The saved connection is not an RDP or VNC profile")?;
        let saved = store
            .get_remote_desktop_profile(profile_id)
            .cloned()
            .ok_or("The saved remote desktop profile is unavailable")?;
        if saved.pre_connect_command.is_some() {
            return Err("Open this profile in OxideTerm to run its local pre-connect command");
        }
        let password = store
            .get_remote_desktop_credential(profile_id)
            .map_err(|_| "The saved remote desktop credential is unavailable")?
            .map(SecretString::into_zeroizing)
            .map(RemoteDesktopSecret::from);
        if saved.protocol == RemoteDesktopProtocol::Rdp && password.is_none() {
            return Err("This RDP profile requires a device-local credential");
        }
        Ok(PublicMcpDesktopOpen {
            password,
            ssh_gateway_connection_id: saved.ssh_gateway_connection_id,
            profile: RemoteDesktopConnectionProfile {
                pre_connect_command: None,
                id: saved.id,
                label: saved.name,
                protocol: saved.protocol,
                endpoint: RemoteDesktopEndpoint::new(saved.host, saved.port),
                transport_endpoint: None,
                socks_proxy: None,
                username: saved.username,
                domain: saved.domain,
                credential_ref: saved.credential_ref,
                read_only: saved.read_only,
                session_options: saved.session_options,
            },
        })
    }

    pub fn desktop_open_rejection(&self, client: &ClientRef) -> Option<&'static str> {
        if !self
            .state
            .clients
            .get(client)
            .is_some_and(|c| c.enabled && c.tool_groups.contains(&ToolGroup::DesktopSession))
        {
            return Some("The MCP client authorization changed before the desktop opened");
        }
        let desktops = self.desktops.lock();
        let count = desktops
            .values()
            .filter(|record| &record.client_ref == client)
            .count();
        (desktops.len() >= PUBLIC_MCP_DESKTOP_CAPACITY
            || count >= PUBLIC_MCP_DESKTOP_CAPACITY_PER_CLIENT)
            .then_some("The retained remote desktop session limit has been reached")
    }

    pub fn register_desktop(
        &self,
        id: DesktopRef,
        client: ClientRef,
        tab_id: TabId,
        title: String,
        observing_frames: bool,
    ) {
        self.desktops.lock().insert(
            id,
            PublicMcpDesktopRecord {
                client_ref: client,
                tab_id,
                title,
                observing_frames,
                frame_artifacts: Default::default(),
                clipboard_artifacts: Default::default(),
            },
        );
    }

    pub fn desktop(&self, client: &ClientRef, id: &DesktopRef) -> Option<PublicMcpDesktopRecord> {
        self.desktops
            .lock()
            .get(id)
            .filter(|record| &record.client_ref == client)
            .cloned()
    }

    pub fn remove_desktop(
        &self,
        client: &ClientRef,
        id: &DesktopRef,
    ) -> Option<PublicMcpDesktopRecord> {
        let mut desktops = self.desktops.lock();
        if !desktops
            .get(id)
            .is_some_and(|record| &record.client_ref == client)
        {
            return None;
        }
        let record = desktops.remove(id)?;
        self.revoke_desktop_artifacts(&record);
        Some(record)
    }

    pub fn remove_client_desktops(&self, client: &ClientRef) -> Vec<PublicMcpDesktopRecord> {
        let mut desktops = self.desktops.lock();
        desktops
            .extract_if(|_, record| &record.client_ref == client)
            .map(|(_, record)| {
                self.revoke_desktop_artifacts(&record);
                record
            })
            .collect()
    }

    pub fn remove_closed_desktop(&self, tab: TabId) {
        let mut desktops = self.desktops.lock();
        for (_, record) in desktops.extract_if(|_, record| record.tab_id == tab) {
            self.revoke_desktop_artifacts(&record);
        }
    }

    fn revoke_desktop_artifacts(&self, record: &PublicMcpDesktopRecord) {
        for artifact in record
            .frame_artifacts
            .iter()
            .chain(&record.clipboard_artifacts)
        {
            self.state.artifacts.revoke(&record.client_ref, artifact);
        }
    }

    pub fn set_desktop_observation(&self, client: &ClientRef, enabled: bool) -> Vec<TabId> {
        let mut desktops = self.desktops.lock();
        desktops
            .values_mut()
            .filter(|r| &r.client_ref == client && r.observing_frames != enabled)
            .map(|record| {
                record.observing_frames = enabled;
                if !enabled {
                    for artifact in record.frame_artifacts.drain() {
                        self.state.artifacts.revoke(client, &artifact);
                    }
                }
                record.tab_id
            })
            .collect()
    }

    pub fn client_desktop_tabs(&self, client: &ClientRef) -> Vec<TabId> {
        self.desktops
            .lock()
            .values()
            .filter(|r| &r.client_ref == client)
            .map(|r| r.tab_id)
            .collect()
    }

    pub fn revoke_desktop_clipboard_content(&self, client: &ClientRef) -> Vec<TabId> {
        self.desktops
            .lock()
            .values_mut()
            .filter(|r| &r.client_ref == client)
            .map(|record| {
                for artifact in record.clipboard_artifacts.drain() {
                    self.state.artifacts.revoke(client, &artifact);
                }
                record.tab_id
            })
            .collect()
    }

    pub fn desktop_clipboard_image(
        &self,
        client: &ClientRef,
        artifact: &ArtifactRef,
        format: DesktopClipboardImageFormat,
    ) -> Result<(RemoteDesktopClipboardFormat, Zeroizing<Vec<u8>>), String> {
        let content = self
            .state
            .artifacts
            .read_all(
                client,
                artifact,
                PUBLIC_MCP_DESKTOP_CLIPBOARD_IMAGE_LIMIT_BYTES,
            )
            .map_err(|error| error.to_string())?;
        if content.projection.media_type != public_desktop_clipboard_media_type(format) {
            return Err("The clipboard artifact media type does not match format".into());
        }
        Ok((public_desktop_clipboard_format(format), content.bytes))
    }
}

impl PublicMcpBackground {
    pub fn desktop_frame(
        &self,
        request: DomainRequest,
        tab_id: TabId,
        frame: PublicMcpDesktopFrame,
        audit_context: Option<AuditContext>,
    ) {
        let (desktop_ref, after_generation, inline) = match &request.call {
            PublicToolCall::DesktopFrame(args) => {
                (args.desktop_ref.clone(), args.after_generation, false)
            }
            PublicToolCall::DesktopAct(args) => (args.desktop_ref.clone(), None, true),
            _ => return,
        };
        let mut audit = audit_context.map(|ctx| {
            ctx.for_request()
                .operation(AuditCategory::File, "desktop_screenshot", Some("png"))
        });
        let PublicMcpDesktopFrame {
            generation,
            graphics_epoch,
            size,
            ..
        } = frame;
        if after_generation == Some(generation) {
            if let Some(audit) = audit.take() {
                audit.finish(AuditOutcome::Unchanged, AuditEvidence::Protocol, None, None);
            }
            finish_serialized(
                request,
                json!({"desktop_ref":desktop_ref, "generation":generation, "graphics_epoch":graphics_epoch, "unchanged":true}),
            );
            return;
        }
        if u64::from(size.width).saturating_mul(u64::from(size.height)) > DESKTOP_MAX_FRAME_PIXELS {
            if let Some(audit) = audit.take() {
                audit.finish(AuditOutcome::Failed, AuditEvidence::Request, None, None);
            }
            request.finish(ToolEnvelope::failed(
                "The remote framebuffer exceeds the supported artifact dimensions",
            ));
            return;
        }
        let backend = self.clone();
        self.forwarding_runtime.spawn_blocking(move || {
            let png = match encode_public_mcp_desktop_png(frame) {
                Ok(png) => png,
                Err(error) => {
                    if let Some(audit) = audit.take() { audit.finish(AuditOutcome::Failed, AuditEvidence::Protocol, None, None); }
                    request.finish(ToolEnvelope::failed(error));
                    return;
                }
            };
            if inline {
                let suspended = backend.admission.lock();
                let desktops = backend.desktops.lock();
                let owned = desktops.get(&desktop_ref).is_some_and(|record|
                    record.client_ref == request.client_ref && record.tab_id == tab_id && record.observing_frames);
                let permitted = backend.state.clients.get(&request.client_ref).is_some_and(|client|
                    client.enabled && client.tool_groups.contains(&request.call.required_group()) &&
                    request.call.additional_required_groups().iter().all(|group| client.tool_groups.contains(group)));
                if *suspended || !owned || !permitted || request.is_cancelled() {
                    if let Some(audit) = audit.take() { audit.finish(AuditOutcome::Denied, AuditEvidence::Lifecycle, None, None); }
                    request.finish(ToolEnvelope::failed("Desktop image access is no longer available"));
                    return;
                }
                if let Some(audit) = audit.take() { audit.finish(AuditOutcome::Succeeded, AuditEvidence::Protocol, None, Some(png.len() as u64)); }
                request.finish(ToolEnvelope {
                    prepared_quick_command: None,
                    outcome: ToolOutcome::Completed, error: None,
                    data: Some(json!({"desktop_ref":desktop_ref, "generation":generation, "graphics_epoch":graphics_epoch, "width":size.width, "height":size.height})),
                    inline_png: Some(oxideterm_public_mcp::calls::InlinePng(png)),
                });
                return;
            }
            let result = backend.state.artifacts.stage(request.client_ref.clone(), &png, "image/png".to_owned(),
                    Some(format!("remote-desktop-{generation}.png"))).map_err(|error|error.to_string());
            let artifact = match result {
                Ok(artifact) => artifact,
                Err(error) => {
                    if let Some(audit) = audit.take() { audit.finish(AuditOutcome::Failed, AuditEvidence::Protocol, None, None); }
                    request.finish(ToolEnvelope::failed(error));
                    return;
                }
            };
            let suspended = backend.admission.lock();
            if let Err(error) = backend.retain_desktop_artifact(&request, &desktop_ref, tab_id, &artifact, ToolGroup::DesktopObserve, *suspended) {
                if let Some(audit) = audit.take() { audit.finish(if request.is_cancelled() { AuditOutcome::Cancelled } else { AuditOutcome::Denied }, AuditEvidence::Lifecycle, None, None); }
                if !request.is_cancelled() { request.finish(ToolEnvelope::failed(error)); }
                return;
            }
            if let Some(audit) = audit.take() { audit.finish(AuditOutcome::Succeeded, AuditEvidence::Protocol, None, Some(artifact.size)); }
            finish_serialized(request, json!({"desktop_ref":desktop_ref, "generation":generation, "graphics_epoch":graphics_epoch,
                "width":size.width, "height":size.height, "unchanged":false, "artifact":artifact}));
        });
    }

    pub fn desktop_clipboard_read(
        &self,
        request: DomainRequest,
        tab_id: TabId,
        snapshot: PublicMcpClipboardSnapshot,
        audit_context: Option<AuditContext>,
    ) {
        let audit_context = audit_context.map(|context| context.for_request());
        let PublicToolCall::ReadDesktopClipboard(args) = &request.call else {
            return;
        };
        let desktop_ref = args.desktop_ref.clone();
        match snapshot {
            PublicMcpClipboardSnapshot::Text(text) => {
                audit_clipboard(audit_context, "remote_to_mcp text", text.len());
                finish_serialized(
                    request,
                    json!({"desktop_ref":desktop_ref, "kind":"text", "text":text.as_str()}),
                );
            }
            PublicMcpClipboardSnapshot::Image { format, bytes } => {
                let backend = self.clone();
                self.forwarding_runtime.spawn_blocking(move || {
                    let artifact = match backend.state.artifacts.stage(request.client_ref.clone(), &bytes,
                        remote_desktop_clipboard_media_type(format).to_owned(), Some("remote-clipboard".to_owned())) {
                        Ok(artifact) => artifact,
                        Err(error) => { request.finish(ToolEnvelope::failed(error.to_string())); return; }
                    };
                    let suspended = backend.admission.lock();
                    if let Err(error) = backend.retain_desktop_artifact(&request, &desktop_ref, tab_id, &artifact, ToolGroup::DesktopClipboard, *suspended) {
                        if !request.is_cancelled() { request.finish(ToolEnvelope::failed(error)); }
                        return;
                    }
                    audit_clipboard(audit_context, "remote_to_mcp image", bytes.len());
                    finish_serialized(request, json!({"desktop_ref":desktop_ref, "kind":"image",
                        "format":remote_desktop_clipboard_format_name(format), "artifact":artifact}));
                });
            }
        }
    }

    fn retain_desktop_artifact(
        &self,
        request: &DomainRequest,
        id: &DesktopRef,
        tab: TabId,
        artifact: &ArtifactProjection,
        group: ToolGroup,
        suspended: bool,
    ) -> Result<(), &'static str> {
        let artifacts = &self.state.artifacts;
        let mut desktops = self.desktops.lock();
        let authorized = !suspended
            && self
                .state
                .clients
                .get(&request.client_ref)
                .is_some_and(|client| {
                    client.enabled
                        && client.tool_groups.contains(&group)
                        && client.tool_groups.contains(&ToolGroup::ArtifactTransfer)
                });
        let record = desktops.get_mut(id).filter(|record| {
            record.client_ref == request.client_ref
                && record.tab_id == tab
                && (group != ToolGroup::DesktopObserve || record.observing_frames)
        });
        match record {
            Some(record) if authorized && !request.is_cancelled() => {
                let retained = if group == ToolGroup::DesktopObserve {
                    &mut record.frame_artifacts
                } else {
                    &mut record.clipboard_artifacts
                };
                retained.retain(|id| artifacts.is_available(&request.client_ref, id));
                retained.insert(artifact.artifact_ref.clone());
                Ok(())
            }
            _ => {
                artifacts.revoke(&request.client_ref, &artifact.artifact_ref);
                Err(if authorized {
                    "The remote desktop handle is no longer available"
                } else {
                    "The remote desktop artifact authorization changed"
                })
            }
        }
    }
}

fn audit_clipboard(context: Option<AuditContext>, detail: &str, bytes: usize) {
    if let Some(context) = context {
        let context = context.for_request();
        let mut audit = context.operation(
            AuditCategory::File,
            "desktop_clipboard_transfer",
            Some(detail),
        );
        let authorization = if matches!(context.source, AuditSource::Mcp | AuditSource::Plugin) {
            AuditAuthorization::Approved
        } else {
            AuditAuthorization::NotRequired
        };
        audit.authorization(authorization, None);
        audit.finish(
            AuditOutcome::Sent,
            AuditEvidence::Protocol,
            None,
            Some(bytes as u64),
        );
    }
}

fn encode_public_mcp_desktop_png(
    frame: PublicMcpDesktopFrame,
) -> Result<Zeroizing<Vec<u8>>, String> {
    let mut rgba = frame.bgra_bytes;
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
