// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_remote_desktop::{
    RemoteDesktopHelperRequest, RemoteDesktopSize, RemoteDesktopWheelDelta,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemoteDesktopMappedPoint {
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemoteDesktopViewportMapper {
    remote_size: RemoteDesktopSize,
    viewport_width: f32,
    viewport_height: f32,
}

impl RemoteDesktopViewportMapper {
    pub fn new(
        remote_size: RemoteDesktopSize,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Option<Self> {
        if viewport_width <= 0.0 || viewport_height <= 0.0 {
            return None;
        }

        Some(Self {
            remote_size,
            viewport_width,
            viewport_height,
        })
    }

    pub fn map_point(self, local_x: f32, local_y: f32) -> RemoteDesktopMappedPoint {
        let scaled_x = scale_axis(local_x, self.viewport_width, self.remote_size.width);
        let scaled_y = scale_axis(local_y, self.viewport_height, self.remote_size.height);
        RemoteDesktopMappedPoint {
            x: scaled_x,
            y: scaled_y,
        }
    }

    pub fn mouse_move_request(self, local_x: f32, local_y: f32) -> RemoteDesktopHelperRequest {
        let point = self.map_point(local_x, local_y);
        RemoteDesktopHelperRequest::MouseMove {
            x: point.x,
            y: point.y,
        }
    }

    pub fn resize_request(width: f32, height: f32) -> Option<RemoteDesktopHelperRequest> {
        if width <= 0.0 || height <= 0.0 {
            return None;
        }

        Some(RemoteDesktopHelperRequest::Resize {
            size: RemoteDesktopSize::clamped(width.round() as u32, height.round() as u32),
        })
    }

    pub fn wheel_request(delta_x: f32, delta_y: f32) -> RemoteDesktopHelperRequest {
        RemoteDesktopHelperRequest::Wheel {
            delta: RemoteDesktopWheelDelta {
                x: delta_x,
                y: delta_y,
            },
        }
    }
}

fn scale_axis(local: f32, viewport: f32, remote: u32) -> u32 {
    if remote == 0 {
        return 0;
    }

    let max = remote.saturating_sub(1) as f32;
    let ratio = (local / viewport).clamp(0.0, 1.0);
    (ratio * max).round() as u32
}

#[cfg(test)]
mod tests {
    use oxideterm_remote_desktop::RemoteDesktopHelperRequest;

    use super::*;

    #[test]
    fn mapper_scales_points_to_remote_framebuffer() {
        let mapper = RemoteDesktopViewportMapper::new(
            RemoteDesktopSize {
                width: 1920,
                height: 1080,
            },
            960.0,
            540.0,
        )
        .unwrap();

        assert_eq!(
            mapper.map_point(480.0, 270.0),
            RemoteDesktopMappedPoint { x: 960, y: 540 }
        );
    }

    #[test]
    fn mapper_clamps_points_to_framebuffer_edges() {
        let mapper = RemoteDesktopViewportMapper::new(
            RemoteDesktopSize {
                width: 100,
                height: 80,
            },
            50.0,
            40.0,
        )
        .unwrap();

        assert_eq!(
            mapper.map_point(-10.0, 90.0),
            RemoteDesktopMappedPoint { x: 0, y: 79 }
        );
    }

    #[test]
    fn resize_request_uses_remote_size_bounds() {
        let request = RemoteDesktopViewportMapper::resize_request(20.0, 10.0).unwrap();

        assert!(matches!(
            request,
            RemoteDesktopHelperRequest::Resize {
                size: RemoteDesktopSize {
                    width: 200,
                    height: 120
                }
            }
        ));
    }

    #[test]
    fn invalid_viewport_has_no_mapper() {
        assert!(
            RemoteDesktopViewportMapper::new(
                RemoteDesktopSize {
                    width: 100,
                    height: 100,
                },
                0.0,
                10.0,
            )
            .is_none()
        );
    }
}
