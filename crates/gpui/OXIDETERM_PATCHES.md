# OxideTerm GPUI Vendor Patches

This directory is a vendored GPUI crate, not a plain crates.io copy. OxideTerm
currently patches `gpui` through the workspace `[patch.crates-io]` entry in the
root `Cargo.toml`, so all `gpui = 0.2.2` users in this workspace resolve to this
local crate.

Before upgrading it, compare this tree against the exact upstream GPUI release
and preserve every OxideTerm-specific patch listed below. The local differences
currently cover mutable remote-desktop textures and cross-platform backdrop
blur rendering, including the Linux Blade shader buffer contract.

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
  -> dedicated platform texture sub-region upload
  -> one painted desktop sprite
```

This remains a rendering-focused fork, not a general fork of GPUI's layout,
input, windowing, or element system.

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
  - Allocate each `DynamicTexture` into its own Metal texture instead of a
    shared sprite atlas page.
  - Implement `PlatformAtlas::update(...)` using the existing Metal region
    upload path.
- `src/platform/windows/directx_atlas.rs`
  - Allocate each `DynamicTexture` into its own DirectX texture instead of a
    shared sprite atlas page.
  - Implement `PlatformAtlas::update(...)` using the existing DirectX
    `UpdateSubresource` path.
- `src/platform/blade/blade_atlas.rs`
  - Allocate each `DynamicTexture` into its own Blade texture instead of a
    shared sprite atlas page.
  - Implement `PlatformAtlas::update(...)` by enqueueing a region upload on the
    existing Blade atlas upload belt.
- `src/platform/test/window.rs`
  - Implement the test atlas `update(...)` method as a no-op so GPUI tests and
    downstream unit tests can compile without a real GPU backend.
- `examples/image.rs` and `examples/image_gallery.rs`
  - Use `gpui_http_client`'s fake test client so the vendored examples compile
    in the OxideTerm workspace without Zed's `reqwest_client` crate.

### Backdrop Blur Rendering

OxideTerm also carries a framebuffer-backed backdrop blur primitive used by
translucent workspace and modal surfaces. Preserve these files as one patch
set; changing only one renderer or shader can leave the host and GPU layouts
incompatible:

- `src/scene.rs`
  - Define the `BackdropBlur` scene primitive, batching, work-area calculation,
    and cached-frame signature.
- `src/window.rs`
  - Add `PaintBackdropBlur` and `Window::paint_backdrop_blur(...)`.
- `src/platform/mac/metal_renderer.rs` and `src/platform/mac/shaders.metal`
  - Snapshot, downsample, blur, and composite the framebuffer through Metal.
- `src/platform/windows/directx_renderer.rs` and
  `src/platform/windows/shaders.hlsl`
  - Implement the equivalent DirectX pipeline. Preserve the render-target copy
    direction fixed by `88d72256`.
- `src/platform/blade/blade_renderer.rs` and
  `src/platform/blade/shaders.wgsl`
  - Implement the Linux/FreeBSD Blade pipeline.
  - Keep Rust reflection type names identical to their WGSL structure names.
  - Keep `BackdropBlurPassParams` at 32 bytes on both sides. Its trailing
    padding is three scalar `f32` fields; a WGSL `vec3<f32>` would move to the
    next 16-byte uniform boundary and expand the shader structure to 48 bytes.
  - Convert scene `PolychromeSprite::grayscale` from Rust `bool` to an explicit
    POD `u32` before uploading it to WGSL. Equal structure sizes do not make a
    one-byte Rust boolean compatible with a four-byte WGSL integer.
  - Reuse the renderer-owned `polychrome_sprite_scratch` buffer for that POD
    conversion. `BufferBelt::alloc_pod(...)` copies the slice immediately, so
    retaining capacity avoids a heap allocation for every image batch.
  - Maintain the headless buffer-contract test. It recursively enumerates every
    structure reachable from a uniform or storage binding and verifies names,
    field counts, field offsets, structure sizes, storage-array strides,
    uniform sizes, scalar widths, and enum discriminants.
- `Cargo.toml`
  - Keep the `naga` development dependency with `wgsl-in`; the headless Blade
    contract test uses the same WGSL parser and layout rules as renderer startup.

The Linux fixes are represented by `99e7bb15`, `d8737492`, and `be03ead0`.
When rebasing onto a newer GPUI, re-evaluate the resulting behavior instead of
blindly replaying these commits: upstream may have added its own backdrop blur
or changed Blade/Naga layout rules.

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
- Dynamic textures use the same sprite drawing primitive as ordinary images,
  but they get a dedicated platform texture allocation so remote desktop uploads
  do not share texture pages with icons, images, glyphs, or other atlas users.

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
6. Confirm every backdrop blur backend still implements the same scene
   primitive semantics: framebuffer snapshot, two-pass blur, clipping, rounded
   mask, and overlay compositing.
7. For Blade, enumerate all WGSL `var<uniform>` and `var<storage>` roots and
   confirm the buffer-contract test contains every recursively reachable
   structure. The test intentionally fails if a new shared structure is added
   without being audited.
8. Run the verification commands below.

## Verification

After changing this vendor fork or the remote desktop renderer, run:

```sh
cargo fmt --check
cargo test -p gpui
cargo test -p gpui --features macos-blade blade_shader_buffer_contract_matches_rust
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

The Blade contract test requires no display server or GPU. On Linux the Blade
module is already enabled by the X11/Wayland features. On macOS, the explicit
`macos-blade` feature makes the same Linux renderer module available for this
headless contract test; it does not change the production macOS Metal backend.
