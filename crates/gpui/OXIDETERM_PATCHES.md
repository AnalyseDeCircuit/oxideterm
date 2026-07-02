# OxideTerm GPUI Vendor Patches

This directory is a vendored GPUI crate, not a plain crates.io copy. OxideTerm
currently patches `gpui` through the workspace `[patch.crates-io]` entry in the
root `Cargo.toml`, so all `gpui = 0.2.2` users in this workspace resolve to this
local crate.

Before upgrading it, compare this tree against the exact upstream GPUI release
and preserve the OxideTerm-specific dynamic texture patches listed below.

## Why GPUI Is Vendored

OxideTerm's RDP and VNC surfaces need to render a continuously changing remote
framebuffer. The previous implementation converted framebuffer regions into
many `RenderImage` tiles and repainted those tiles through GPUI's ordinary image
cache. That worked functionally, but it had poor scaling characteristics for
remote desktop workloads:

- dirty updates still created new image objects for updated tiles;
- GPUI atlas entries had to be inserted and retired repeatedly;
- the renderer had to draw many sprites instead of one stable desktop surface;
- the app could not upload a sub-rectangle into an existing texture through the
  public GPUI API.

The upstream GPUI 0.2.2 API exposes `RenderImage`, but `RenderImage` is a cached
image object rather than a mutable framebuffer surface. The platform atlas
implementations already have lower-level region upload primitives, but those are
not reachable from application code. OxideTerm vendors GPUI to expose the narrow
dynamic texture capability needed by the remote desktop renderer:

```text
remote framebuffer backing buffer
  -> dirty rectangle accumulator
  -> GPUI DynamicTexture
  -> platform atlas sub-region upload
  -> one painted desktop sprite
```

This is intentionally a small rendering extension, not a general fork of GPUI's
layout, input, windowing, or element system.

## Required Local Patches

Keep these patches when updating GPUI:

- `src/assets.rs`
  - Add `DynamicTextureId`.
  - Add `DynamicTextureParams`.
  - Add `DynamicTexture`, a stable BGRA texture handle with a fixed device-pixel
    size.
- `src/platform.rs`
  - Add `AtlasKey::DynamicTexture`.
  - Route dynamic textures to the polychrome atlas.
  - Add `PlatformAtlas::update(...)` for sub-rectangle texture uploads.
- `src/window.rs`
  - Add dynamic texture byte-size and bounds validation helpers.
  - Add `Window::update_dynamic_texture(...)`.
  - Add `Window::paint_dynamic_texture(...)`.
  - Add `Window::drop_dynamic_texture(...)`.
- `src/platform/mac/metal_atlas.rs`
  - Implement `PlatformAtlas::update(...)` using the existing Metal region
    upload path.
- `src/platform/windows/directx_atlas.rs`
  - Implement `PlatformAtlas::update(...)` using the existing DirectX
    `UpdateSubresource` path.
- `src/platform/blade/blade_atlas.rs`
  - Implement `PlatformAtlas::update(...)` by enqueueing a region upload on the
    existing Blade atlas upload belt.
- `src/platform/test/window.rs`
  - Implement the test atlas `update(...)` method as a no-op so GPUI tests and
    downstream unit tests can compile without a real GPU backend.

## API Contract

The patched dynamic texture path has these constraints:

- Pixel data is BGRA, four bytes per pixel.
- `Window::update_dynamic_texture(...)` accepts bounds relative to the dynamic
  texture's top-left corner.
- Update bounds must fit inside the texture.
- The update byte length must exactly match `width * height * 4`.
- `Window::paint_dynamic_texture(...)` paints the whole texture as one
  polychrome sprite.
- If an update arrives before the texture is first painted, GPUI creates a
  blank atlas entry and then applies the update.

These checks are deliberately strict. Remote desktop corruption should fail
early instead of silently uploading malformed data into the GPU atlas.

## OxideTerm Call Sites

The current consumer is the remote desktop renderer:

- `crates/oxideterm-gpui-remote-desktop/src/state.rs`
  - Maintains the CPU-side BGRA framebuffer backing buffer.
  - Queues full-frame and dirty-rectangle uploads for a stable
    `DynamicTexture`.
- `crates/oxideterm-gpui-remote-desktop/src/view.rs`
  - Drains pending dynamic texture uploads during paint.
  - Paints a single dynamic texture for the remote desktop surface.
- `crates/oxideterm-gpui-app/src/workspace/remote_desktop.rs`
  - Drops retired dynamic textures when remote desktop sessions reconnect,
    fail, disconnect, or close.

Cursor images still use ordinary `RenderImage` because cursor shapes are small
and do not need this mutable framebuffer path.

## Upgrade Checklist

When updating GPUI:

1. Start from the exact upstream GPUI release or commit being adopted.
2. Reapply the required local patches above.
3. Check whether upstream GPUI has gained a public mutable texture, surface, or
   atlas sub-region API. If it has, prefer replacing this local patch with the
   upstream API instead of carrying the fork indefinitely.
4. Confirm the dynamic texture path still uses the same atlas coordinate space
   as `paint_image`.
5. Confirm every platform backend used by OxideTerm implements
   `PlatformAtlas::update(...)`.
6. Run the verification commands below.

## Verification

After changing this vendor fork or the remote desktop renderer, run:

```sh
cargo fmt --check
cargo check -p oxideterm-gpui-app
cargo test -p oxideterm-gpui-remote-desktop
cargo test -p oxideterm-gpui-app remote_desktop
git diff --check
```

The remote desktop tests should cover:

- full frame creation;
- dirty update application to the CPU backing buffer;
- dynamic texture reuse across dirty updates;
- full-frame recovery after mismatched base frames;
- dynamic texture retirement on failure or session replacement.

The important regression signal is that ordinary dirty updates do not create or
retire remote desktop framebuffer `RenderImage` tiles anymore. They should reuse
one dynamic texture and queue sub-rectangle uploads instead.
