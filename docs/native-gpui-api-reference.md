# OxideTerm GPUI API Reference

This is a local working reference for the Rust native UI. It is intentionally based on the GPUI APIs and patterns used in this repository, not on a generic GPUI tutorial.

Current pinned versions:

- `gpui = 0.2.2`
- `gpui-component = 0.5.1`

## Entry Point

Native startup lives in `crates/oxideterm-gpui-app/src/main.rs`.

```rust
Application::new()
    .with_assets(NativeAssets)
    .run(|cx: &mut App| {
        cx.activate(true);
        cx.on_action(quit);
        cx.bind_keys(platform::app_key_bindings(&startup_settings));
        cx.set_menus(platform::app_menus(&I18n::default()));

        cx.open_window(platform::window_options(bounds), |window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx)?);
            cx.new(|cx| Root::new(workspace, window, cx))
        })?;
    });
```

Use `actions!` for command types, then bind them with `cx.on_action(...)` or route through workspace action handlers.

```rust
actions!(oxideterm, [NewTerminal, NewConnection, CommandPalette, Quit]);
```

## Entity And Render Model

An interactive screen is usually a GPUI entity:

```rust
let entity = cx.new(|cx| MyView::new(cx));
```

Render implementations use:

```rust
impl Render for MyView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().child("content")
    }
}
```

OxideTerm often keeps most state in `WorkspaceApp` and renders split files through `include!(...)`; for isolated high-frequency surfaces, prefer a separate `Entity<T>` so `cx.notify()` does not repaint unrelated UI.

Use `cx.notify()` after mutating render state. Avoid calling it from polling paths unless the visible data actually changed.

## Element Builder Basics

Most UI is fluent `div()` builders. Import these traits when methods look missing:

```rust
use gpui::{ParentElement, Styled, InteractiveElement, StatefulInteractiveElement};
use gpui::prelude::*;
```

Common shape:

```rust
div()
    .flex()
    .flex_col()
    .gap(px(tokens.spacing.three))
    .rounded(px(tokens.radii.md))
    .border_1()
    .border_color(rgb(tokens.ui.border))
    .bg(rgb(tokens.ui.bg_panel))
    .text_size(px(tokens.metrics.ui_text_sm))
    .text_color(rgb(tokens.ui.text))
    .child(header)
    .when(condition, |el| el.child(extra))
    .when_some(optional, |el, value| el.child(render_value(value)))
```

Important conventions:

- Prefer token values from `ThemeTokens` over hard-coded numbers.
- Use `AnyElement` when returning mixed element types.
- Convert with `.into_any_element()` at function boundaries.
- Use `.min_w(px(0.0))` on flex children containing truncating or wrapping text.
- Use `.overflow_hidden()` for clipped pills/cards and `.truncate()` for one-line labels.
- Do not put card-in-card layouts unless the Tauri source does.

## Custom Element Lifecycle

Most UI should use builder elements, but anchor probes and custom renderers implement `Element` directly.

The lifecycle used in this repo is:

```rust
impl Element for Probe {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let layout_id = self.child.as_mut().unwrap().request_layout(window, cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.child.as_mut().unwrap().prepaint(window, cx);
        // Use `bounds` here for same-frame anchor measurement.
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.as_mut().unwrap().paint(window, cx);
    }
}
```

Use this sparingly. The two valid local cases so far are:

- Measuring trigger bounds for overlays in the same frame.
- Rendering custom text input/selection segments that GPUI does not provide.

Do not use custom `Element` as a shortcut for ordinary layout.

## Events

Use `cx.listener` for handlers that need `&mut WorkspaceApp`:

```rust
.on_mouse_down(
    MouseButton::Left,
    cx.listener(|this, event, window, cx| {
        this.open_new_connection_form(window, cx);
        cx.stop_propagation();
    }),
)
```

Useful event types:

- `MouseDownEvent`
- `MouseMoveEvent`
- `MouseUpEvent`
- `ScrollWheelEvent`
- `KeyDownEvent`

Stop propagation inside overlays, modals, popovers, and nested scroll areas:

```rust
.on_scroll_wheel(|_, _, cx| cx.stop_propagation())
```

## Keyboard Actions

Global commands are GPUI actions. Startup bindings are built in `crates/oxideterm-gpui-app/src/keybindings.rs` and registered through `platform::app_key_bindings`.

Action routing generally enters `WorkspaceApp` through `workspace/actions.rs`. Be careful with Escape ordering:

1. Close the topmost overlay/popover/dialog.
2. Close command palette or shortcuts modal.
3. Close active surface-specific transient UI.
4. Only then collapse larger panels such as the AI sidebar.

Text input handlers should ignore shortcut keystrokes and use platform text/composition state instead of raw key names.

## Focus

Workspace owns a `FocusHandle` and implements `Focusable`.

Use:

```rust
window.focus(&self.focus_handle);
```

Focus needs to be explicit after opening modals, command palettes, and custom inputs. OxideTerm custom inputs use a hidden IME element (`WorkspaceImeElement`) to receive text composition.

## Async Work

Use `cx.spawn` for UI-owned async work:

```rust
cx.spawn(async move |weak, cx| {
    let result = do_work().await;
    let _ = weak.update(cx, move |this, cx| {
        this.state = result;
        cx.notify();
    });
})
.detach();
```

Rules:

- Never call Tokio-only APIs from the GPUI main thread unless a runtime is active.
- Use app/backend runtime handles for network, SSH, MCP, cloud sync, and AI work.
- Never hold a mutable borrow of `self` across async boundaries.
- For delayed UI such as tooltips, store a generation/id and ignore stale completions.

## Scroll Containers

Basic scroll:

```rust
div().overflow_y_scroll()
```

Project selectable scroll helpers live in `workspace.rs`:

```rust
element.selectable_overflow_y_scroll(&handle)
element.selectable_overflow_y_scrollbar(&handle)
```

## Element Backdrop Blur

Tauri source of truth:

- `src/components/ui/dialog.tsx`: `DialogOverlay` is `bg-black/60` plus `linuxBackdropBlurClass("backdrop-blur-sm")`.
- `src/components/command-palette/CommandPalette.tsx`: command palette keeps the shared dialog blur and only overrides the overlay color to `bg-black/40`.
- `src/components/fileManager/QuickLook.tsx`: QuickLook is `bg-black/80` plus the same `backdrop-blur-sm`.
- `src/lib/linuxWebviewProfile.ts`: unsafe Linux webview profiles strip the blur class but keep the overlay color.

Native GPUI mapping lives in `crates/oxideterm-gpui-ui/src/modal.rs`:

```rust
TauriBackdropRole::{Dialog, CommandPalette, QuickLook, Popover}
TauriBackdropEffect { color, blur_px }
set_tauri_backdrop_blur_allowed(render_policy.allow_background_blur)
```

This deliberately keeps the Tauri blur request in shared data even though GPUI
0.2.2 cannot render it yet. The current painted result is the correct overlay
color fallback, not full visual parity.

The renderer gap is structural:

- `StyleRefinement` has no `filter` or `backdrop_filter` field.
- `PaintQuad` only carries bounds, radii, background, border widths, border color, and border style.
- `Scene::Primitive` batches `Shadow`, `Quad`, `Path`, `Underline`, sprites, and `Surface`; there is no order-aware backdrop primitive.
- The Metal, Blade/WGSL, and Windows quad shaders draw solid quad backgrounds and do not sample the already-rendered framebuffer.
- `WindowBackgroundAppearance::Blurred` is window-level vibrancy/background blur. It blurs outside-window content or the native window background, not the GPUI scene behind an individual modal element.

Do not solve modal blur with a WebView, JavaScript, screen capture, or
`NSVisualEffectView` placed over the GPUI view. Those routes do not match CSS
`backdrop-filter` semantics and will break top-layer composition.

A real parity implementation needs a GPUI renderer primitive roughly shaped as:

```rust
pub struct BackdropBlur {
    pub bounds: Bounds<ScaledPixels>,
    pub corner_radii: Corners<ScaledPixels>,
    pub blur_radius: ScaledPixels,
    pub overlay_color: Hsla,
    pub content_mask: ContentMask<ScaledPixels>,
    pub order: PaintOrder,
}
```

Renderer requirements:

- Preserve paint order, so the primitive samples only content already drawn behind it.
- Copy or render the current framebuffer into an intermediate texture before the blur pass; sampling from the drawable while writing to it is not a valid general strategy.
- Apply a separable blur or equivalent downsampled blur within the primitive bounds and content mask.
- Composite the Tauri overlay color after the blur so `bg-black/60`, `bg-black/40`, and `bg-black/80` remain exact.
- Provide a safe no-blur fallback when `render_policy.allow_background_blur` is false.

These call `.track_scroll(handle)` so selection/autoscroll logic can observe the container.

Guidelines:

- Use explicit `ScrollHandle` for any scroll container that participates in text selection or overlay positioning.
- Reset scroll handles when replacing list content or opening a new modal.
- Stop scroll propagation in nested popovers/selects.
- `overflow_y_scrollbar()` is fine for page-like settings surfaces; command palettes and select popups generally need propagation stopped.

## Virtual Lists

For large flat lists, prefer `uniform_list` with `UniformListScrollHandle`:

```rust
let handle = UniformListScrollHandle::new();

uniform_list("items", item_count, move |range, window, cx| {
    range.map(|ix| render_row(ix, window, cx)).collect::<Vec<_>>()
})
.track_scroll(&handle)
```

Use `ListState` / `list(...)` when row heights are dynamic, as in AI chat:

```rust
let state = ListState::new(count, ListAlignment::Top, px(overdraw));
list(state, move |index, window, cx| render_message(index, window, cx))
```

Pitfalls:

- Do not wrap GPUI dynamic lists in large cached roots that also own scroll height.
- Do not rebuild all Markdown/message elements on every terminal tick.
- For chat, isolate message-level render/cache state and keep terminal notifications from invalidating the AI sidebar.

## Deferred And Anchored Overlays

Use `deferred(anchored()...)` for popovers that must float above normal content:

```rust
deferred(
    anchored()
        .anchor(Corner::TopLeft)
        .position(point(px(x), px(y)))
        .position_mode(AnchoredPositionMode::Window)
        .child(popup)
)
.with_priority(300)
```

For select/dropdown overlays inside scroll containers, use an anchor probe. The important trick is measuring in `prepaint`, not one frame later.

Project examples:

- `oxideterm-gpui-ui/src/select.rs`
- `oxideterm-gpui-ui/src/text_input.rs`

Pattern:

```rust
select_anchor_probe(anchor_id, trigger, |anchor, window, cx| {
    // store current window-space bounds immediately
})
```

Then render the popup using those bounds. This avoids the “dropdown jumps after scrolling” bug.

## Modal Pattern

Use the shared modal primitives from `oxideterm-gpui-ui/src/modal.rs`:

```rust
modal_overlay(
    tokens,
    modal_container(tokens)
        .w(px(tokens.metrics.modal_width))
        .max_h(px(modal_max_height))
        .flex()
        .flex_col()
        .child(modal_header(tokens, title, description))
        .child(modal_body(tokens).flex_1().min_h(px(0.0)).overflow_y_scroll())
        .child(modal_footer(tokens).flex_none())
)
```

For Tauri parity:

- Preserve `DialogHeader`, body, footer order.
- Keep body `flex-1 min-h-0 overflow-y-auto`.
- Stop scroll propagation on popups inside modals.
- Do not flatten sections unless Tauri does.

## Text Input

GPUI does not provide browser text input behavior automatically for our custom fields. OxideTerm uses a custom renderer in `oxideterm-gpui-ui/src/text_input.rs`.

`TextInputView` needs:

- `value`
- `placeholder`
- `focused`
- `caret_visible`
- `secret`
- `selected_all`
- `selected_range`
- `marked_text`

Selection ranges use UTF-16 offsets to line up with macOS text input behavior. Keep all selection math saturating; do not subtract unchecked.

Use anchor probes when text input overlays or IME need bounds.

## Selectable Text

Most GPUI labels are not browser-selectable by default. OxideTerm has a custom selectable text layer in `workspace/selectable_text.rs`.

Use helpers for display text that should behave like browser text:

- Single-line values: selectable label/value helpers from the workspace selectable text module.
- Scrollable pages: `.selectable_overflow_y_scroll(&handle)` or `.selectable_overflow_y_scrollbar(&handle)`.

Known limitation to remember: independent labels do not automatically form a DOM-like page-wide selection unless routed through the same selection owner.

## Tooltips

Workspace tooltips use delayed global state in `root/render.rs`:

```rust
this.queue_workspace_tooltip(id, label, x, y, cx);
this.clear_workspace_tooltip(id, cx);
```

Use mouse position plus a small offset:

```rust
.on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
    this.queue_workspace_tooltip(
        "stable-id",
        this.i18n.t("tooltip.key"),
        f32::from(event.position.x) + 12.0,
        f32::from(event.position.y) + 16.0,
        cx,
    );
}))
```

Do not use the visible label as the id; keep ids stable and unique.

## Assets And Icons

Assets are served through `NativeAssets` in `crates/oxideterm-gpui-app/src/assets.rs`.

Use existing `LucideIcon` variants instead of hand-rolled SVGs when an icon exists. Render through the project helper, usually:

```rust
Self::render_lucide_icon(LucideIcon::Info, 14.0, rgb(tokens.ui.warning))
```

## Tests

GPUI tests use `#[gpui::test]` when rendering behavior matters. For pure state/input behavior, normal Rust tests are preferred.

Useful commands:

```bash
cargo check -p oxideterm-gpui-app
cargo test -p oxideterm-gpui-app new_connection::form_state::tests
cargo test -p oxideterm-gpui-app session_manager::tests
```

## OxideTerm-Specific Parity Checklist

Before saying a GPUI feature matches Tauri:

- Compare against the exact Tauri source file and locale keys.
- Preserve structure, not just visible labels.
- Preserve default state and remembered settings.
- Preserve disabled states, warning text, tooltip behavior, and footer actions.
- Verify popup positioning inside scroll containers.
- Verify keyboard order: Escape, Enter, arrows, Tab, IME composition.
- Verify persistence and backend fields if the UI edits data.
- Run at least `cargo check -p oxideterm-gpui-app` plus targeted tests.

## Common Pitfalls

- `AnyElement` does not expose builder methods like `.child(...)`; keep the concrete `Div` until composition is done.
- Plain `overflow_y_scroll()` has no explicit handle; selectable/autoscroll behavior needs a tracked `ScrollHandle`.
- Deferred overlays need current window-space bounds; stale anchors cause one-frame jumps and clipping.
- Calling Tokio APIs from the GPUI main thread can panic with “there is no reactor running”.
- Large dynamic Markdown/chat views should not be rebuilt on every unrelated state tick.
- Custom text input must implement browser-like selection, paste, delete, IME, and shortcut gating explicitly.
