# GPUI Maintenance Boundary

`crates/gpui-ce/` contains six local patches to Longbridge's `gpui-pre` 0.3.7 packages, the version used by GPUI Kit v0.7.0. Other gpui-pre packages resolve through crates.io. The six patches retain OxideTerm behavior and community-derived GPUI-CE filters, colors, and window capabilities; this is not an unmodified Zed distribution. The authoritative provenance and local-delta records are [`UPSTREAM_BASELINE.toml`](../../crates/gpui-ce/gpui/UPSTREAM_BASELINE.toml) and [`OXIDETERM_PATCHES.md`](../../crates/gpui-ce/gpui/OXIDETERM_PATCHES.md).

## Decide The Layer First

Change application code when the behavior belongs to OxideTerm navigation, modal ownership, settings, terminal UX, or a product-specific view. Change shared UI code when the primitive is owned by `oxideterm-gpui-ui`.

Consider a local GPUI patch only when all of the following are true:

1. The failure is below the application or shared UI boundary.
2. A minimal reproduction identifies framework or native-platform behavior.
3. The affected platform path is known: Windows Direct3D/Win32, macOS Metal/AppKit, or Linux WGPU/X11/Wayland.
4. The local delta can be recorded in the vendor ledger and verified on the affected platform.

Do not copy unrelated Zed application code into product fixes. A framework refresh imports the audited gpui-pre publication through the procedure below, then reconciles the recorded local and community-derived patches.

## Platform Ownership Map

| Concern | Primary location | Evidence required before changing it |
| --- | --- | --- |
| Shared element, layout, input, and window abstractions | `crates/gpui-ce/gpui` | A minimal framework-level reproduction or a product path that cannot own the behavior |
| Windows windowing, input, cursor, and Direct3D | `gpui_windows` | Windows reproduction, message/input sequence, and native check |
| macOS AppKit, text, windowing, and Metal | `gpui_macos` and `gpui_apple` | macOS reproduction and native check |
| Linux windowing | `gpui_linux` | Explicit Wayland or X11 reproduction |
| WGPU renderer shared by Linux paths | `gpui_wgpu` | Renderer evidence that is not window-system-specific |
| Product components and themes | `oxideterm-gpui-app` and `oxideterm-gpui-ui` | Product-level behavior; do not edit vendor code first |

## Vendor Rules

- Preserve the six local package paths and root `[patch.crates-io]` mappings recorded by the baseline file. Do not treat a downloaded research sample as the complete dependency graph.
- Record every intentional OxideTerm framework delta in `OXIDETERM_PATCHES.md`.
- Refresh from checksum-verified registry archives with the recorded `package.metadata.gpui-pre` Zed revision. Keep the remaining dependencies registry-resolved; do not replay the old 18-directory CE layout or a monolithic diff.
- Keep renderer changes platform-specific: Windows uses Direct3D 11, macOS uses Metal, and Linux uses WGPU with separate Wayland and X11 windowing paths.
- Prefer an upstream equivalent when it satisfies the complete contract. Preserve GPUI-CE provenance and its root license for carried community code; do not silently drop an OxideTerm delta.

Every intentional local framework change needs an English modification notice near the affected source and a semantic entry in `OXIDETERM_PATCHES.md`. The ledger records behavior rather than fragile line numbers so the change survives audited upstream refreshes.

## Change Workflow

1. Reproduce the defect in the product and decide whether an app, shared UI, or framework layer owns it.
2. Inspect `UPSTREAM_BASELINE.toml` and the patch ledger before editing a vendored file; an equivalent local patch may already exist.
3. Make the narrowest framework correction that preserves the existing platform contract.
4. Add or update focused framework coverage only for stable behavior that can be exercised without platform timing assumptions.
5. Record the local delta in the patch ledger and run the native checks plus manual host validation.

For a vendor refresh, first validate the current record:

```sh
python3 scripts/quality/verify_gpui_vendor.py
```

Pass `--registry-archives /path/to/pristine-archives` to verify the six `<package-name>.crate` checksums, embedded manifests, Zed metadata, and Apache licenses. The optional `--gpui-ce-checkout` checks the retained historical CE license source. Cargo.lock pins the actual dependency graph; unrelated lockfile updates do not invalidate the six-package source record. Review each renderer and windowing backend independently.

## Verification

Run the relevant native check locally and use the cross-platform CI matrix before merging:

```sh
cargo check --locked -p gpui-pre-windows
cargo check --locked -p gpui-pre-macos
cargo check --locked -p oxideterm-gpui-app
```

The first two commands require their matching host platform. For input, window, cursor, and renderer defects, manual reproduction on the affected operating system remains required.

Also run `python3 scripts/quality/verify_gpui_vendor.py` when changing baseline metadata, vendor licenses, or the patch ledger.
