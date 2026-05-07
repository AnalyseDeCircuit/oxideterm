// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::Path;
use std::sync::Arc;

use gpui::RenderImage;
use image::{Frame, RgbaImage};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfDocumentInfo {
    pub page_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfPageBitmap {
    pub page_index: usize,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl PdfPageBitmap {
    pub fn into_render_image(self) -> Option<Arc<RenderImage>> {
        let pixels = gpui_render_image_pixels_from_rgba(self.rgba);
        let image = RgbaImage::from_raw(self.width, self.height, pixels)?;
        Some(Arc::new(RenderImage::new(vec![Frame::new(image)])))
    }
}

fn gpui_render_image_pixels_from_rgba(mut pixels: Vec<u8>) -> Vec<u8> {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    pixels
}

pub trait PdfPreviewBackend: Send + Sync {
    fn document_info(&self, path: &Path) -> Result<PdfDocumentInfo, PdfPreviewError>;

    fn render_page(
        &self,
        path: &Path,
        page_index: usize,
        target_width: u32,
    ) -> Result<PdfPageBitmap, PdfPreviewError>;
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PdfPreviewError {
    #[error("PDFium backend is unavailable: {0}")]
    BackendUnavailable(String),
    #[error("Failed to load PDF: {0}")]
    LoadFailed(String),
    #[error("Failed to render PDF page: {0}")]
    RenderFailed(String),
    #[error("PDF page {0} is out of range")]
    PageOutOfRange(usize),
}

#[derive(Clone, Debug, Default)]
pub struct PdfiumPreviewBackend;

#[cfg(feature = "pdfium")]
impl PdfPreviewBackend for PdfiumPreviewBackend {
    fn document_info(&self, path: &Path) -> Result<PdfDocumentInfo, PdfPreviewError> {
        let pdfium = bind_pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(path, None)
            .map_err(|error| PdfPreviewError::LoadFailed(error.to_string()))?;
        Ok(PdfDocumentInfo {
            page_count: doc.pages().len() as usize,
        })
    }

    fn render_page(
        &self,
        path: &Path,
        page_index: usize,
        target_width: u32,
    ) -> Result<PdfPageBitmap, PdfPreviewError> {
        use pdfium_render::prelude::PdfRenderConfig;

        let pdfium = bind_pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(path, None)
            .map_err(|error| PdfPreviewError::LoadFailed(error.to_string()))?;
        let page = doc
            .pages()
            .get(page_index as u16)
            .map_err(|_| PdfPreviewError::PageOutOfRange(page_index))?;
        let image = page
            .render_with_config(
                &PdfRenderConfig::new()
                    .set_target_width(target_width as i32)
                    .render_form_data(true),
            )
            .map_err(|error| PdfPreviewError::RenderFailed(error.to_string()))?
            .as_image()
            .to_rgba8();
        Ok(PdfPageBitmap {
            page_index,
            width: image.width(),
            height: image.height(),
            rgba: image.into_raw(),
        })
    }
}

#[cfg(feature = "pdfium")]
fn bind_pdfium() -> Result<pdfium_render::prelude::Pdfium, PdfPreviewError> {
    use pdfium_render::prelude::Pdfium;

    let bindings = Pdfium::bind_to_system_library()
        .map_err(|error| PdfPreviewError::BackendUnavailable(error.to_string()))?;
    Ok(Pdfium::new(bindings))
}

#[cfg(not(feature = "pdfium"))]
impl PdfPreviewBackend for PdfiumPreviewBackend {
    fn document_info(&self, _path: &Path) -> Result<PdfDocumentInfo, PdfPreviewError> {
        Err(PdfPreviewError::BackendUnavailable(
            "compiled without the `pdfium` feature".to_string(),
        ))
    }

    fn render_page(
        &self,
        _path: &Path,
        _page_index: usize,
        _target_width: u32,
    ) -> Result<PdfPageBitmap, PdfPreviewError> {
        Err(PdfPreviewError::BackendUnavailable(
            "compiled without the `pdfium` feature".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[derive(Default)]
    struct FakePdfBackend;

    impl PdfPreviewBackend for FakePdfBackend {
        fn document_info(&self, _path: &Path) -> Result<PdfDocumentInfo, PdfPreviewError> {
            Ok(PdfDocumentInfo { page_count: 1 })
        }

        fn render_page(
            &self,
            _path: &Path,
            page_index: usize,
            _target_width: u32,
        ) -> Result<PdfPageBitmap, PdfPreviewError> {
            Ok(PdfPageBitmap {
                page_index,
                width: 1,
                height: 1,
                rgba: vec![255, 255, 255, 255],
            })
        }
    }

    #[test]
    fn pdf_backend_contract_returns_document_info_and_bitmap() {
        let backend = FakePdfBackend;
        assert_eq!(
            backend.document_info(Path::new("fixture.pdf")).unwrap(),
            PdfDocumentInfo { page_count: 1 }
        );
        assert_eq!(
            backend
                .render_page(Path::new("fixture.pdf"), 0, 800)
                .unwrap()
                .rgba,
            vec![255, 255, 255, 255]
        );
    }

    #[test]
    fn pdf_backend_types_are_thread_safe() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PdfiumPreviewBackend>();
        assert_send_sync::<PdfDocumentInfo>();
        assert_send_sync::<PdfPageBitmap>();
    }
}
