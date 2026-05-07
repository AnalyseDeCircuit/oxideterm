# Native Preview PDFium Distribution

`oxideterm-preview` keeps PDF rendering behind the `pdfium` feature. When the
feature is disabled, `PdfiumPreviewBackend` reports an explicit unavailable
state. When the feature is enabled, the backend currently calls
`Pdfium::bind_to_system_library()`, so a developer machine with PDFium installed
can render PDFs but a user machine without the dynamic library will fail closed.

The native app must not ship with a "works on my machine" PDF path. Release
builds that enable PDF preview need one of these concrete distribution modes:

1. Bundle a platform PDFium dynamic library next to the app binary and add that
   directory to the runtime lookup path before constructing `PdfiumPreviewBackend`.
2. Use a platform package that installs PDFium as an app dependency and verify
   the loader path during startup.
3. Compile without `pdfium` and show the existing explicit unsupported preview
   state until a bundled library is present.

CI should cover both states:

- `cargo test -p oxideterm-preview` verifies the no-PDFium fallback.
- `cargo test -p oxideterm-preview --features pdfium` must run in an image that
  contains the same PDFium library shape used by release packaging.

Packaging acceptance criteria:

- macOS app bundle contains the `.dylib` in the app resources or framework
  location used by the loader.
- Windows installer contains the matching `.dll` next to the executable or in a
  configured application library directory.
- Linux packages either depend on a known compatible PDFium shared object or
  bundle one in the app image/package.
- Startup or first PDF preview reports a clear `BackendUnavailable` error when
  the library is missing; it must not silently render a blank viewer.
