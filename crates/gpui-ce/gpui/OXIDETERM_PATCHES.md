# OxideTerm GPUI-CE Vendor Ledger

OxideTerm vendors a pinned GPUI-CE source snapshot. This is not an OxideTerm
fork with an independent upstream history, and it is not the previous
monolithic `gpui` crate copied from crates.io.

The authoritative provenance record is [`UPSTREAM_BASELINE.toml`](./UPSTREAM_BASELINE.toml).
It pins:

- upstream repository: `https://github.com/gpui-ce/gpui-ce.git`;
- upstream commit: `6c799b8e994266233014cea66d7769675ec1967c`;
- OxideTerm import base: `7c2c5046c93ae4d54a132d40c33d0c622274354a`;
- the SHA-256 digest of the reviewed workspace `Cargo.lock`;
- the pristine upstream tree object for every imported crate;
- the imported and deliberately excluded paths;
- the license and packaged-license locations.

The tree object IDs in `UPSTREAM_BASELINE.toml` describe the pristine upstream
snapshot before the local deltas in this document are applied. Do not replace
them with hashes of the modified OxideTerm trees.

## Imported Closure

The approved vendor closure contains these 16 crates:

1. `crates/gpui-ce/gpui`
2. `crates/gpui-ce/gpui_ce_util`
3. `crates/gpui-ce/gpui_collections`
4. `crates/gpui-ce/gpui_derive_refineable`
5. `crates/gpui-ce/gpui_linux`
6. `crates/gpui-ce/gpui_macos`
7. `crates/gpui-ce/gpui_macros`
8. `crates/gpui-ce/gpui_media`
9. `crates/gpui-ce/gpui_platform`
10. `crates/gpui-ce/gpui_refineable`
11. `crates/gpui-ce/gpui_scheduler`
12. `crates/gpui-ce/gpui_shared_string`
13. `crates/gpui-ce/gpui_sum_tree`
14. `crates/gpui-ce/gpui_wgpu`
15. `crates/gpui-ce/gpui_windows`
16. `crates/gpui-ce/gpui_zed_util`

WebAssembly is not a shipped or supported OxideTerm target. `crates/gpui_web`,
`crates/gpui_elements`, `crates/gpui_tokio`, and `tooling/perf` are deliberately
excluded.

The workspace root declares the closure as local path dependencies and keeps
the GPUI crates non-publishable. Several vendored manifests therefore have
small integration-only changes such as `publish = false`, workspace dependency
inheritance, an explicit Apache-2.0 license for `gpui_shared_string`, and
OxideTerm-compatible dependency versions. These manifest changes are part of
the vendor delta and must be reviewed during every refresh.

## Renderer Baseline: No Blade

This vendor baseline does not contain the old Blade renderer, Blade shaders, or
the old Blade-specific Naga buffer-contract test. Do not replay any of the
previous `src/platform/blade/*`, `macos-blade`, or Blade WGSL patches. The
semantic Rust-to-shader layout contract is still mandatory and is migrated to
the WGPU backend below.

The renderer layout is now:

- Linux and FreeBSD: `gpui_wgpu`, using WGPU with Vulkan and GL backends;
- macOS: `gpui_macos`, using Metal;
- Windows: `gpui_windows`, using Direct3D 11;

Backdrop filtering is already provided by the pinned GPUI-CE baseline. The
OxideTerm UI uses the upstream `backdrop_blur` style path. There is no local
`PaintBackdropBlur` scene primitive or renderer-specific backdrop-blur patch to
preserve.

### WGPU shader-transfer contract

`crates/gpui-ce/gpui_wgpu/src/wgpu_renderer.rs` must not upload scene structures as raw
memory. Scene structures can contain Rust enums, booleans, private fields, or
padding that do not share WGSL's representation. Preserve these rules:

- every uniform and storage upload uses a `bytemuck::Pod` transfer structure;
- scene enums and booleans are converted to explicit `u32` values;
- reusable scratch vectors hold fully initialized storage-buffer instances;
- the generic storage-buffer writer accepts only `T: Pod` and performs the
  only typed-slice-to-byte conversion;
- Naga reflection recursively enumerates every structure reachable from a
  uniform or storage binding and checks every field offset, total size, and
  storage-array stride;
- the separate dual-source-blending shader is audited in addition to the base
  shader module.

`Background::shader_components` is a hidden, backend-facing decomposition that
lets renderer crates encode private background tags without inspecting enum
storage. New background variants must update that exhaustive conversion and
the shader encoding together.

The historical Blade blur struct is not copied because WGPU owns a different
`BlurParams` contract. Its replacement is the Naga size/offset audit against
the actual WGPU uniform.

## Local Patch Inventory

Every source file changed by OxideTerm should retain an English modification
notice near the top. The sections below define the behavior to preserve; line
numbers are intentionally omitted because upstream refreshes move code.

### Transparent border-only quad overdraw

`crates/gpui-ce/gpui/src/window.rs` limits transparent, border-only quads to four clipped
border regions before inserting them into the scene:

- the quad retains its original bounds, corner radii, border widths, and border style so the
  shader still evaluates rounded and dashed borders against the correct geometry;
- only each primitive's rectangular content mask is narrowed to one edge;
- the masks include one device pixel of antialiasing margin;
- opaque quads remain one primitive, and controls too small to contain a removable interior
  retain the original primitive;
- an existing ancestor content mask is intersected with every edge mask, and empty
  intersections are omitted.

OxideTerm has many large bordered panels. Rendering one transparent quad for each panel would
run the quad fragment shader across its entire transparent interior. Preserving the geometry
while excluding that interior reduces GPU overdraw without changing layout or border appearance.
The behavior is covered by tests for transparent and opaque quads and is an independent
implementation informed by `zed-industries/zed#61274`.

### Native Windows thread-pool dispatch

`crates/gpui-ce/gpui_windows/src/dispatcher.rs` schedules background work with the native
Win32 thread-pool API instead of the WinRT `Windows::System::Threading` projection:

- immediate tasks use `TrySubmitThreadpoolCallback` with the requested Win32 callback priority;
- delayed tasks use a one-shot `CreateThreadpoolTimer` deadline expressed as a negative
  100-nanosecond interval;
- exactly one callback reconstructs and runs each raw `RunnableVariant`;
- failed submission or timer creation reconstructs and drops the runnable immediately so raw
  task ownership is never leaked;
- timer callbacks close their native timer after execution;
- the existing GPUI task timing records and main-thread priority queue remain unchanged.

The unused workspace `System_Threading` feature is removed from `Cargo.toml`;
`Win32_System_Threading` remains required. This avoids the extra WinRT work-item wrapper and its
internal synchronization on the Windows hot path. Preserve these ownership and priority rules
during vendor refreshes. The behavior is an independent implementation informed by
`zed-industries/zed#60917`.

### Re-entrant draw and element-arena lifetime

Windows can synchronously re-enter its window procedure while GPUI is drawing, including through
cross-thread messages and nested COM/OLE loops used by clipboard and drag-and-drop operations.
OxideTerm therefore carries one coordinated draw-lifetime patch across the generic GPUI and
Windows platform layers:

- `crates/gpui-ce/gpui/src/arena.rs` counts active draw scopes. `clear` defers while any
  enclosing scope remains active, and `Drop` force-clears only when the arena itself is destroyed.
  Allocation bounds are checked in address space before a pointer is constructed, avoiding an
  out-of-bounds pointer on chunk spill.
- `crates/gpui-ce/gpui/src/window.rs` enters and exits the App-owned arena scope for every draw.
  Scope teardown is a guard operation so normal return and panic unwinding both balance the
  nesting count. The resulting clear token validates the owning App before clearing.
- The generic `on_request_frame` callback rejects a request received while a GPUI draw is already
  on the current thread's stack. A deferred forced render remains pending across both re-entry
  and frame throttling.
- `crates/gpui-ce/gpui_windows/src/events.rs`,
  `crates/gpui-ce/gpui_windows/src/platform.rs`, and
  `crates/gpui-ce/gpui_windows/src/window.rs` share one platform-owned draw coordinator across all
  windows. A nested Windows paint validates the update region to prevent a `WM_PAINT` busy loop;
  the existing vsync thread invalidates all windows again on the next tick. Forced device-recovery
  renders remain pending until a draw acquires the coordinator.
- Synchronous draw helpers in `crates/gpui-ce/gpui/src/app.rs`,
  `crates/gpui-ce/gpui/src/app/test_app.rs`,
  `crates/gpui-ce/gpui/src/app/test_context.rs`, and
  `crates/gpui-ce/gpui/src/elements/div.rs` clear through the same App-validated token. Product
  benchmark and test helpers in `oxideterm-gpui-ui`, `oxideterm-gpui-editor`,
  `oxideterm-gpui-ide`, and `oxideterm-gpui-app` pass their App context through the same API.

Arena unit tests cover nested clear deferral and unbalanced scopes. A GPUI integration test opens
a second window during the outer window's paint and proves that elements allocated before the
nested draw remain valid afterward. A Windows-only unit test covers coordinator lease release.
The behavior is an independent implementation informed by `zed-industries/zed#60295`; do not
reduce it to a window-local boolean because the safety invariant spans arena allocation, generic
frame dispatch, presentation, IME updates, and multiple windows.

### Metal intermediate texture lifetime

`crates/gpui-ce/gpui_macos/src/metal_renderer.rs` allocates its full-window intermediate
textures lazily. Preserve these lifecycle rules:

- updating the drawable size records the valid size and invalidates intermediates created for a
  previous size, but does not immediately allocate replacements;
- an ordinary scene renders directly to the drawable without path, scene-color, blur, or filter
  group intermediates;
- path and multisample intermediates are allocated only when the scene contains paths;
- the full-resolution scene-color target, half-resolution blur ping/pong targets, and bounded
  full-resolution filter-group targets are allocated together only when a scene contains a
  backdrop filter or content-filter boundary;
- headless rendering follows the same allocation and size-invalidation rules;
- `intermediate_textures_follow_scene_demand_and_resize` must continue to cover an ordinary
  scene, independent path and filter allocation, resize invalidation, and recreation at the new
  size.

The reason for this patch is memory proportional to the drawable area. The previous lifecycle
created the path target, scene-color target, every filter-group target, and both blur targets
whenever the window size changed, including at initial window creation. Most terminal frames do
not use paths or filters, so those private Metal textures reserved substantial unified memory
while idle and amplified the per-window and per-session memory difference observed on macOS.
Allocation must therefore remain demand-driven; a vendor refresh must not restore eager creation
merely to simplify resize handling.

### macOS text-system feature

`crates/oxideterm-gpui-platform/Cargo.toml` must enable the vendored
`gpui_platform/font-kit` feature. Without it, GPUI-CE constructs
`NoopTextSystem` on macOS: layout surfaces and SVG assets remain visible, but
all glyph-backed text and icons disappear.

### Nested scroll ownership

OxideTerm contains nested ordinary overflow regions and virtual lists. Preserve the event
ownership rules in `crates/gpui-ce/gpui/src/elements/div.rs` and
`crates/gpui-ce/gpui/src/elements/list.rs`:

- the innermost hovered scroll region consumes a wheel event when its offset changes;
- an event continues bubbling only when that region is already at its boundary;
- virtual-list scrolling reports whether it actually moved so the event handler can apply the
  same rule.

Without this patch, one macOS trackpad event can move both a child and its scrollable ancestor,
which makes the GPUI-CE build feel substantially more sensitive than the previous GPUI build.

### Bounded text and input lock lifetimes

Preserve the explicit temporary bindings that release internal locks before processing owned
results or entering later dispatch work:

- `crates/gpui-ce/gpui_macos/src/text_system.rs` releases the font database read lock before
  extending the family-name result;
- `crates/gpui-ce/gpui_macos/src/window.rs` releases the window-state lock before returning from
  the input method command branches;
- `crates/gpui-ce/gpui/src/text_system/line_layout.rs` releases the previous-frame cache lock
  before promoting a cached line or shaping a replacement.

These are semantic lock-scope safeguards migrated from the previous GPUI vendor tree. Do not
collapse the bindings back into `if let` scrutinee temporaries during cleanup.

### Windows DirectWrite callback and glyph readback safety

`crates/gpui-ce/gpui_windows/src/direct_write.rs` carries the Windows text-rendering hardening
landed in `zed-industries/zed@89e8a4b9ec7e` after the pinned GPUI-CE baseline:

- the renderer context passed to `IDWriteTextLayout::Draw` is a mutable binding and the callback
  pointer is derived from `&raw mut`, because `DrawGlyphRun` appends shaped runs and advances the
  measured width through that pointer;
- DirectWrite glyph arrays are converted with a null-aware helper, so a zero-length null array is
  accepted without constructing an invalid Rust slice and a nonzero null array is rejected;
- color-glyph staging textures are unmapped immediately after their rows are copied.

The mutable-pointer provenance fix prevents optimized Windows builds from treating callback writes
as undefined behavior, which can otherwise surface as missing, stale, or misplaced terminal glyphs.
Preserve this patch until the next audited GPUI-CE baseline contains the equivalent upstream fix.

### DynamicTexture and renderer resource generations

OxideTerm remote desktop requires one stable mutable BGRA framebuffer instead
of rebuilding many immutable `RenderImage` tiles.

Core changes:

- `crates/gpui-ce/gpui/src/assets.rs`
  - defines `DynamicTextureId`, hidden `DynamicTextureParams`, and
    `DynamicTexture` with a stable identity and fixed device-pixel size;
- `crates/gpui-ce/gpui/src/platform.rs`
  - adds `AtlasKey::DynamicTexture` as a polychrome atlas entry;
  - adds `PlatformAtlas::update` for relative dirty-region uploads;
  - adds `PlatformAtlas::resource_generation` for GPU-resource invalidation;
- `crates/gpui-ce/gpui/src/window.rs`
  - validates positive dimensions, bounds, overflow, and exact four-byte BGRA
    upload length;
  - exposes `update_dynamic_texture`, `paint_dynamic_texture`,
    `drop_dynamic_texture`, and `renderer_resource_generation`;
  - creates a blank stable atlas entry when an update precedes first paint;
- `crates/gpui-ce/gpui/src/platform/test/window.rs`
  - records dynamic texture allocations and uploads in the test atlas;
  - exposes a controllable test resource generation.

The public upload contract is strict:

- the texture size and update size are expressed in device pixels;
- bytes are tightly packed BGRA, four bytes per pixel;
- update bounds are relative to the dynamic texture origin;
- the region must fit entirely inside the texture;
- the byte length must equal `width * height * 4`;
- a dynamic texture has a dedicated backend allocation and is painted as one
  polychrome sprite.

Backend changes:

- `crates/gpui-ce/gpui_wgpu/src/wgpu_atlas.rs`
  - uses exact-size dedicated WGPU allocations for dynamic textures;
  - queues and validates sub-region uploads;
  - advances `resource_generation` whenever atlas resources are invalidated,
    including device recovery;
- `crates/gpui-ce/gpui_macos/src/metal_atlas.rs`
  - uses exact-size dedicated Metal textures and relative region uploads;
  - currently returns generation `0` because the Metal atlas is not recreated
    by the current backend;
- `crates/gpui-ce/gpui_windows/src/directx_atlas.rs`
  - uses exact-size dedicated Direct3D textures and `UpdateSubresource`-style
    relative region uploads;
  - clears ordinary and dynamic atlas resources through one reset path on
    device loss and advances `resource_generation` exactly once per reset;
- `crates/gpui-ce/gpui/src/platform/test/window.rs`
  - provides the deterministic headless implementation used by GPUI tests.

Any future backend that replaces or clears resources behind a stable
`DynamicTexture` identity must advance `resource_generation`. Returning `0` is
valid only when the backend does not expose such replacement through this
contract. A refresh must not silently turn a real reset into a permanent
generation of zero.

### WGPU backend selection and recovery

The GPUI-CE WGPU path carries OxideTerm-specific VM and recovery behavior:

- `crates/gpui-ce/gpui_wgpu/src/wgpu_context.rs`
  - recognizes the case-insensitive
    `OXIDETERM_GPU_BACKEND=auto|vulkan|opengl` contract and safely falls back
    to auto after warning about an invalid value;
  - recognizes `OXIDETERM_GPU_DEVICE_ID` as an optional four-digit hexadecimal
    PCI device override and ignores an invalid value after reporting it;
  - keeps auto mode ordered across Vulkan and GL and permits CPU adapters as
    the final fallback;
  - ranks explicit device, compositor match, device type, and backend in a
    deterministic order;
  - tests real surface configuration before accepting an adapter;
- `crates/gpui-ce/gpui_wgpu/src/wgpu_renderer.rs`
  - coordinates device recovery across windows;
  - uses exponential retry backoff from 100 milliseconds to 5 seconds without
    sleeping on the render thread;
  - stops after 12 consecutive failures and emits one actionable terminal
    diagnostic instead of retrying forever;
  - clears recoverable frame resources without terminating the application;
  - recreates resources transactionally and notifies the atlas only after a
    replacement context succeeds;
- `crates/gpui-ce/gpui_linux/src/linux/wayland/client.rs`,
  `crates/gpui-ce/gpui_linux/src/linux/x11/client.rs`, and
  `crates/gpui-ce/gpui_windows/src/window.rs`
  - initialize the shared WGPU context and recovery state through the
    OxideTerm recovery-aware context wrapper;
- `crates/gpui-ce/gpui_wgpu/src/wgpu_atlas.rs`
  - drops stale uploads and advances the renderer resource generation when the
    WGPU resources are cleared or replaced.

Do not restore the old hard-coded `ZED_DEVICE_ID` environment-variable
contract. The product-facing names are intentionally scoped to OxideTerm.

Other inherited diagnostic controls are also product-scoped:

- `crates/gpui-ce/gpui/src/platform.rs` uses `OXIDETERM_GPUI_HEADLESS`;
- `crates/gpui-ce/gpui_wgpu/src/wgpu_renderer.rs` uses the
  `OXIDETERM_FONTS_GAMMA`, `OXIDETERM_FONTS_GRAYSCALE_ENHANCED_CONTRAST`, and
  `OXIDETERM_FONTS_SUBPIXEL_ENHANCED_CONTRAST` support overrides;
- `crates/gpui-ce/gpui_ce_util/src/lib.rs` uses `OXIDETERM_MEASUREMENTS`;
- `crates/gpui-ce/gpui_zed_util/src/util.rs` uses `OXIDETERM_ALLOW_ROOT`.

Do not reintroduce inherited public `ZED_*` aliases. A refresh must audit the
complete vendor closure, not only the renderer crates.

### Linux display-backend startup fallback

Linux desktop startup must not panic merely because `WAYLAND_DISPLAY` is set
while the selected compositor omits a GPUI-required global. Preserve these
rules across vendor refreshes:

- `crates/gpui-ce/gpui_linux/src/linux/wayland/client.rs` returns a diagnostic
  error when the Wayland connection, registry, event sources, `wl_seat`,
  `wl_compositor`, `wl_shm`, or `xdg_wm_base` cannot be initialized;
- `crates/gpui-ce/gpui_linux/src/linux.rs` logs that Wayland initialization
  failed and falls back to X11 when that backend is compiled;
- if both desktop backends fail, the terminal diagnostic includes both errors
  instead of preserving an opaque Wayland `unwrap()` panic.

WSLg is a known reason for `WAYLAND_DISPLAY` to be present without a
`wl_seat`. Keep the fallback capability-based rather than hard-coding a WSL
environment check, so remote, nested, kiosk, and future compositors receive the
same behavior.

### Hidden system cursor

Remote desktop must be able to hide the local system pointer while painting a
remote cursor. Preserve `CursorStyle::None` across the full platform split:

- `crates/gpui-ce/gpui/src/platform.rs` defines the public variant;
- `crates/gpui-ce/gpui_linux/src/linux/wayland.rs` and
  `crates/gpui-ce/gpui_linux/src/linux/wayland/client.rs` preserve explicit hiding
  across Wayland pointer events;
- `crates/gpui-ce/gpui_linux/src/linux/x11/client.rs` uses the cached transparent X11
  cursor;
- `crates/gpui-ce/gpui_linux/src/linux/platform.rs` rejects accidental generic mapping
  after the Linux clients have handled the variant;
- `crates/gpui-ce/gpui_macos/src/window.rs` hides the AppKit cursor and avoids restoring
  a visible cursor region;
- `crates/gpui-ce/gpui_windows/src/util.rs` maps the variant to a null Win32 cursor
  handle.

### Virtual GPU metadata

OxideTerm render policy must distinguish virtual adapters from CPU software
rasterizers and physical GPUs.

- `crates/gpui-ce/gpui/src/gpui.rs` adds the serde-defaulted
  `GpuSpecs::is_virtual_gpu` field;
- `crates/gpui-ce/gpui_wgpu/src/wgpu_renderer.rs` maps WGPU
  `DeviceType::VirtualGpu` directly;
- `crates/gpui-ce/gpui_windows/src/directx_renderer.rs` recognizes the maintained PCI
  vendor ID set for common hypervisor adapters;
- `crates/oxideterm-gpui-platform/src/rendering.rs` converts the metadata into
  product render-policy input;
- `crates/oxideterm-render-policy/src/lib.rs` maps `VirtualGpu` to the low-power
  automatic policy rather than treating it as software emulation.

Do not infer virtual status from a device-name substring in product code. Add a
well-scoped backend detector and a focused test when a new backend needs virtual
adapter classification.

### UniformList overscan

`crates/gpui-ce/gpui/src/elements/uniform_list.rs` adds `with_overscan` and expands the
visible range by a bounded number of rows on both sides. The default is zero,
which preserves upstream behavior. Keep the clamping and edge-range unit test;
this patch prevents empty edges during fast scrolling without changing the
list's ownership of scrolling.

### Timer and Corner compatibility exports

`crates/gpui-ce/gpui/src/gpui.rs` temporarily re-exports:

- `smol::Timer` as `gpui::Timer` for existing asynchronous delays;
- `geometry::Anchor` as `gpui::Corner` for existing anchored overlays.

These aliases are migration compatibility layers, not new GPUI abstractions.
Remove either alias only after all OxideTerm call sites have moved to the
GPUI-CE API and the full workspace builds without it.

## Product-Owned Remote Desktop Integration

Remote desktop behavior belongs to OxideTerm product crates and must not be
moved into the vendored GPUI crates:

- `crates/oxideterm-gpui-remote-desktop/src/state.rs`
  - owns the CPU BGRA framebuffer, stable `DynamicTexture`, dirty rectangles,
    upload sequencing, and retired-texture collection;
  - keeps the remote protocol's texture generation separate from the renderer
    resource generation;
  - schedules one full-frame re-upload when the renderer resource generation
    changes, confirms it only after a successful upload, and retries failures;
- `crates/oxideterm-gpui-remote-desktop/src/view.rs`
  - reads `Window::renderer_resource_generation`, drains uploads during paint,
    updates dirty regions, and paints one dynamic texture;
  - applies `CursorStyle::None` when the remote cursor should replace the local
    pointer;
- `crates/oxideterm-gpui-app/src/workspace/remote_desktop/session.rs`
  - drops retired or session-owned dynamic textures during reconnect,
    replacement, failure, disconnect, and close paths.

Cursor images remain ordinary immutable images. Only the changing desktop
framebuffer uses `DynamicTexture`.

## Refresh Procedure

Refresh the vendor only on an isolated migration branch or worktree.

1. Check out the candidate GPUI-CE commit in a clean temporary directory and
   record its full commit ID.
2. Recompute the dependency closure from the candidate manifests. Any addition
   to or removal from the 16-crate closure requires an explicit review of why it
   is needed, its license, and whether it becomes a shipped target.
3. Record the pristine Git tree ID for every imported crate
   in `UPSTREAM_BASELINE.toml` before applying local patches.
4. Refresh only the declared crate directories. Do not import font assets,
   excluded examples, performance tooling, Blade code, or unrelated Zed
   application crates.
5. Reapply the manifest integration and each local patch category in this
   ledger as a separate, reviewable change. Prefer an upstream equivalent when
   it satisfies the complete product contract.
6. Review every upstream renderer split independently: WGPU, Metal, Direct3D,
   the test atlas, and cursor handling. Do not assume a trait change reached all
   backends.
7. Reapply the product-owned remote desktop integration only in the OxideTerm
   crates. Verify that ordinary dirty updates retain the same dynamic texture
   identity.
8. Update `Cargo.lock`, `NOTICE`, `THIRD_PARTY_NOTICES.md`, packaged license
   files, and native-package verification lists.
9. Update the pinned commit, tree IDs, import date, and this ledger only after
   the resulting source and package checks pass.
10. Inspect the final diff against both the previous OxideTerm vendor snapshot
    and the pristine candidate commit. Unexplained local differences are a
    failed refresh.

Never copy the candidate tree over the current vendor tree and assume the local
patches survived. The manifest and source deltas in this ledger are intentional
and must be reviewed explicitly.

## Verification

Verify provenance against the clean upstream checkout:

```sh
python3 scripts/quality/verify_gpui_vendor.py \
  --upstream-checkout /path/to/clean/gpui-ce
```

Verify formatting, the vendor crates, and product integration:

```sh
cargo fmt --check
cargo check --locked -p gpui -p gpui_platform -p gpui_wgpu
cargo test --locked -p gpui --lib
cargo test --locked -p gpui_wgpu --lib
cargo check --locked -p oxideterm-gpui-app
cargo test --locked -p oxideterm-gpui-remote-desktop --lib
cargo test --locked -p oxideterm-render-policy --lib
git diff --check
```

Run target-native renderer checks on their actual operating systems:

```sh
# macOS
cargo check --locked -p gpui_macos
cargo test --locked -p gpui_macos --lib

# Windows
cargo check --locked -p gpui_windows
cargo test --locked -p gpui_windows --lib

# Linux or FreeBSD with the configured display dependencies
cargo check --locked -p gpui_linux
```

Verify notices and packaged legal documents:

```sh
python3 scripts/release/generate_third_party_notices.py --check
python3 -m unittest \
  scripts.tests.test_package_native \
  scripts.tests.test_verify_native_package
```

The remote desktop regression suite must prove full-frame creation, dirty-region
application, texture identity reuse, renderer-generation full refresh, retry
after upload failure, and texture retirement. VM qualification remains a
separate runtime gate: exercise Vulkan and OpenGL fallback on representative
VMware, VirtualBox, Hyper-V, and QEMU/virtio configurations rather than treating
successful compilation as proof of driver compatibility.
