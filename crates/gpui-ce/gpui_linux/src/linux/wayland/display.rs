use std::{
    fmt::Debug,
    hash::{Hash, Hasher},
};

use anyhow::Context as _;
use uuid::Uuid;
use wayland_backend::client::ObjectId;

use gpui::{Bounds, DevicePixels, DisplayId, Pixels, PlatformDisplay};

#[derive(Debug, Clone)]
pub(crate) struct WaylandDisplay {
    /// The ID of the wl_output object
    pub id: ObjectId,
    pub name: Option<String>,
    pub bounds: Bounds<Pixels>,
    pub physical_bounds: Bounds<DevicePixels>,
    pub scale_factor: f32,
}

impl Hash for WaylandDisplay {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PlatformDisplay for WaylandDisplay {
    fn id(&self) -> DisplayId {
        DisplayId::new(self.id.protocol_id() as u64)
    }

    fn uuid(&self) -> anyhow::Result<Uuid> {
        let name = self
            .name
            .as_ref()
            .context("Wayland display does not have a name")?;
        Ok(Uuid::new_v5(&Uuid::NAMESPACE_DNS, name.as_bytes()))
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
