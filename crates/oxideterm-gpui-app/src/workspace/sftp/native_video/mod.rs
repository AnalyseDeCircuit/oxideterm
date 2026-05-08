use std::{cell::RefCell, rc::Rc, time::Duration};

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Window,
};
use oxideterm_preview::{PlatformVideoSnapshot, PlatformVideoState};

#[derive(Clone, Default)]
pub(super) struct SharedSftpNativeVideoSurface(Rc<RefCell<SftpNativeVideoSurface>>);

impl SharedSftpNativeVideoSurface {
    pub(super) fn sync(
        &self,
        path: &str,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> PlatformVideoSnapshot {
        self.0.borrow_mut().sync(path, bounds, window, cx)
    }

    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub(super) fn snapshot(&self) -> PlatformVideoSnapshot {
        self.0.borrow().snapshot()
    }

    pub(super) fn detach(&self) {
        self.0.borrow_mut().detach();
    }
}

pub(super) struct SftpNativeVideoElement {
    path: String,
    surface: SharedSftpNativeVideoSurface,
    child: Option<AnyElement>,
}

pub(super) fn sftp_native_video_element(
    path: String,
    surface: SharedSftpNativeVideoSurface,
    child: impl IntoElement,
) -> SftpNativeVideoElement {
    SftpNativeVideoElement {
        path,
        surface,
        child: Some(child.into_any_element()),
    }
}

impl IntoElement for SftpNativeVideoElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SftpNativeVideoElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let layout_id = self
            .child
            .as_mut()
            .expect("native video child should render once")
            .request_layout(window, cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if let Some(child) = self.child.as_mut() {
            child.prepaint(window, cx);
        }
        // Platform video is a real child surface. Keep it aligned in the same
        // frame as GPUI layout, otherwise scrolled dialogs can leave the player
        // one frame behind the preview body.
        self.surface.sync(&self.path, bounds, window, cx);
        window.request_animation_frame();
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = self.child.as_mut() {
            child.paint(window, cx);
        }
    }
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod unsupported;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub(super) use linux::SftpNativeVideoSurface;
#[cfg(target_os = "macos")]
pub(super) use macos::SftpNativeVideoSurface;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub(super) use unsupported::SftpNativeVideoSurface;
#[cfg(target_os = "windows")]
pub(super) use windows::SftpNativeVideoSurface;
