use super::DirectXDevices;
use anyhow::Result;
use gpui::{Bounds, ContentMask, Corners, Quad, ScaledPixels, point, rgb, size};

#[test]
#[ignore = "requires a hardware DirectX device and a desktop session"]
fn drawing_releases_completed_media_without_texture_uploads() -> Result<()> {
    use gpui::{DevicePixels, PlatformAtlas, Scene, WindowBackgroundAppearance};
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    use windows::{Win32::UI::WindowsAndMessaging::*, core::w};

    unsafe {
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!(""),
            WS_POPUP,
            0,
            0,
            64,
            64,
            None,
            None,
            None,
            None,
        )?;
        let result = (|| -> Result<()> {
            let devices = DirectXDevices::new()?;
            let mut renderer = super::DirectXRenderer::new(hwnd, &devices, true)?;
            renderer.resize(size(DevicePixels(64), DevicePixels(64)))?;
            let resource = Arc::new([42u8; 16]);
            let retained = Arc::downgrade(&resource);
            let receipt = renderer.atlas.gpu_submission()?;
            receipt.retain(resource);
            let deadline = Instant::now() + Duration::from_secs(2);
            let scene = Scene::default();
            while retained.upgrade().is_some() && Instant::now() < deadline {
                renderer.draw(&scene, WindowBackgroundAppearance::Opaque)?;
                std::thread::yield_now();
            }
            assert!(
                retained.upgrade().is_none(),
                "normal draws must retire media resources without a CPU upload polling task"
            );
            assert!(receipt.is_complete());
            Ok(())
        })();
        let _ = DestroyWindow(hwnd);
        result
    }
}

#[test]
#[ignore = "requires a hardware DirectX device and a desktop session"]
fn rounded_clip_preserves_pixels_and_parent_background() -> Result<()> {
    use gpui::{DevicePixels, FilterBoundary, Scene, WindowBackgroundAppearance};
    use windows::{Win32::UI::WindowsAndMessaging::*, core::w};

    unsafe {
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!(""),
            WS_POPUP,
            0,
            0,
            64,
            64,
            None,
            None,
            None,
            None,
        )?;
        let result = (|| -> Result<()> {
            let devices = DirectXDevices::new()?;
            let mut renderer = super::DirectXRenderer::new(hwnd, &devices, true)?;
            renderer.resize(size(DevicePixels(64), DevicePixels(64)))?;
            let bounds = Bounds::new(
                point(ScaledPixels(0.), ScaledPixels(0.)),
                size(ScaledPixels(64.), ScaledPixels(64.)),
            );
            let mut scene = Scene::default();
            scene.insert_primitive(Quad {
                bounds,
                content_mask: ContentMask { bounds },
                background: rgb(0x204080).into(),
                ..Default::default()
            });
            let boundary = FilterBoundary {
                order: 0,
                bounds,
                content_mask: ContentMask { bounds },
                corner_radii: Corners::all(ScaledPixels(20.)),
                filters: Default::default(),
                opacity: 1.,
                clip_rounded: true,
                is_start: true,
            };
            scene.insert_primitive(boundary.clone());
            scene.insert_primitive(Quad {
                bounds,
                content_mask: ContentMask { bounds },
                background: rgb(0xc08040).into(),
                ..Default::default()
            });
            scene.insert_primitive(Quad {
                bounds: Bounds::new(
                    point(ScaledPixels(0.), ScaledPixels(32.)),
                    size(ScaledPixels(64.), ScaledPixels(32.)),
                ),
                content_mask: ContentMask { bounds },
                background: rgb(0x40c080).into(),
                ..Default::default()
            });
            scene.insert_primitive(FilterBoundary {
                is_start: false,
                ..boundary
            });
            scene.finish();
            let image = renderer.render_to_image(&scene, WindowBackgroundAppearance::Opaque)?;
            for (x, y) in [(0, 0), (63, 0), (0, 63), (63, 63)] {
                assert_eq!(
                    image.get_pixel(x, y).0,
                    [32, 64, 128, 255],
                    "corner {x},{y}"
                );
            }
            assert_eq!(image.get_pixel(32, 31).0, [192, 128, 64, 255]);
            assert_eq!(image.get_pixel(32, 32).0, [64, 192, 128, 255]);
            assert_eq!(image.get_pixel(32, 62).0, [64, 192, 128, 255]);
            assert_eq!(image.get_pixel(32, 1).0, [192, 128, 64, 255]);
            Ok(())
        })();
        DestroyWindow(hwnd)?;
        result
    }
}
