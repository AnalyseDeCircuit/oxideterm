use anyhow::Context as _;
use uuid::Uuid;
use x11rb::{
    connection::Connection as _,
    protocol::{randr::ConnectionExt as _, xproto::ConnectionExt as _},
    xcb_ffi::XCBConnection,
};

use gpui::{Bounds, DevicePixels, DisplayId, Pixels, PlatformDisplay, Point, Size, px};

#[derive(Debug)]
pub(crate) struct X11Display {
    display_id: DisplayId,
    bounds: Bounds<Pixels>,
    physical_bounds: Bounds<DevicePixels>,
    scale_factor: f32,
    uuid: Uuid,
    primary: bool,
}

impl X11Display {
    pub(crate) fn new(
        xcb: &XCBConnection,
        scale_factor: f32,
        x_screen_index: usize,
    ) -> anyhow::Result<Self> {
        let screen = xcb
            .setup()
            .roots
            .get(x_screen_index)
            .with_context(|| format!("No screen found with index {x_screen_index}"))?;
        Ok(Self {
            display_id: DisplayId::new(x_screen_index as u64),
            physical_bounds: Bounds {
                origin: Default::default(),
                size: Size {
                    // X11 screen dimensions are u16, so widen them before converting to device pixels.
                    width: u32::from(screen.width_in_pixels).into(),
                    height: u32::from(screen.height_in_pixels).into(),
                },
            },
            bounds: Bounds {
                origin: Default::default(),
                size: Size {
                    width: px(screen.width_in_pixels as f32 / scale_factor),
                    height: px(screen.height_in_pixels as f32 / scale_factor),
                },
            },
            scale_factor,
            uuid: Uuid::from_bytes([0; 16]),
            primary: true,
        })
    }

    pub(crate) fn all(xcb: &XCBConnection, scale_factor: f32) -> Vec<Self> {
        let mut displays = Vec::new();
        for (screen_index, screen) in xcb.setup().roots.iter().enumerate() {
            let Ok(monitors_cookie) = xcb.randr_get_monitors(screen.root, true) else {
                continue;
            };
            let Ok(monitors) = monitors_cookie.reply() else {
                continue;
            };
            for monitor in monitors.monitors {
                let name = xcb
                    .get_atom_name(monitor.name)
                    .ok()
                    .and_then(|cookie| cookie.reply().ok())
                    .and_then(|reply| String::from_utf8(reply.name).ok())
                    .unwrap_or_else(|| format!("monitor-{}", monitor.name));
                let stable_name = format!("{screen_index}:{name}");
                let display_id =
                    DisplayId::new(((screen_index as u64) << 32) | u64::from(monitor.name));
                let physical_bounds = Bounds {
                    origin: Point {
                        x: i32::from(monitor.x).into(),
                        y: i32::from(monitor.y).into(),
                    },
                    size: Size {
                        width: u32::from(monitor.width).into(),
                        height: u32::from(monitor.height).into(),
                    },
                };
                displays.push(Self {
                    display_id,
                    bounds: physical_bounds.to_pixels(scale_factor),
                    physical_bounds,
                    scale_factor,
                    uuid: Uuid::new_v5(&Uuid::NAMESPACE_DNS, stable_name.as_bytes()),
                    primary: monitor.primary,
                });
            }
        }
        if displays.is_empty() {
            displays.extend(
                xcb.setup()
                    .roots
                    .iter()
                    .enumerate()
                    .filter_map(|(screen_index, _)| {
                        Self::new(xcb, scale_factor, screen_index).ok()
                    }),
            );
        }
        displays
    }

    pub(crate) fn primary(xcb: &XCBConnection, scale_factor: f32) -> Option<Self> {
        let mut displays = Self::all(xcb, scale_factor);
        let primary_index = displays
            .iter()
            .position(|display| display.primary)
            .unwrap_or(0);
        (primary_index < displays.len()).then(|| displays.swap_remove(primary_index))
    }
}

impl PlatformDisplay for X11Display {
    fn id(&self) -> DisplayId {
        self.display_id
    }

    fn uuid(&self) -> anyhow::Result<Uuid> {
        Ok(self.uuid)
    }

    fn bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }

    fn physical_bounds(&self) -> Bounds<DevicePixels> {
        self.physical_bounds
    }

    fn scale_factor(&self) -> f32 {
        self.scale_factor
    }
}
