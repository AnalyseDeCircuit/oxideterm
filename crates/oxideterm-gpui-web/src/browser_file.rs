use js_sys::{Promise, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use zeroize::Zeroizing;
mod import;
pub use import::BrowserImport;

#[wasm_bindgen(module = "/src/upload.mjs")]
extern "C" {
    #[wasm_bindgen(js_name = chooseUploads)]
    fn choose_uploads() -> Promise;
}

pub const MAX_IMPORT_BYTES: u64 = 64 * 1024 * 1024;
pub fn validate_filename(name: &str) -> Result<(), String> {
    (std::path::Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        == Some(name)
        && !name.contains(['\\', '\0']))
    .then_some(())
    .ok_or_else(|| "ide.validation.nameInvalid".to_owned())
}

pub fn pick_uploads() -> JsFuture {
    JsFuture::from(choose_uploads())
}

#[wasm_bindgen(module = "/src/download.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = downloadFile)]
    pub fn download(name: &str, bytes: &[u8]) -> Result<(), JsValue>;
    #[wasm_bindgen(catch, js_name = downloadBlob)]
    pub fn download_blob(name: &str, blob: &web_sys::Blob) -> Result<(), JsValue>;
}

#[wasm_bindgen(inline_js = r#"
export function pickFileBytes(limit, accept) {
    return new Promise((resolve, reject) => {
        const input = document.createElement("input");
        input.type = "file";
        input.accept = accept;
        input.addEventListener("cancel", () => resolve(null), { once: true });
        input.addEventListener("change", async () => {
            const file = input.files[0];
            if (!file) return resolve(null);
            if (file.size > limit) return reject("size");
            try { resolve(new Uint8Array(await file.arrayBuffer())); }
            catch { reject("read"); }
        }, { once: true });
        if (input.showPicker) input.showPicker();
        else input.click();
    });
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = pickFileBytes)]
    fn pick_js(limit: u32, accept: &str) -> Promise;
}

/// Open synchronously inside the user gesture. The future owns just the selected file read.
pub fn pick(
    limit: u32,
    accept: &str,
) -> impl Future<Output = Result<Option<Zeroizing<Vec<u8>>>, &'static str>> + use<> {
    let result = JsFuture::from(pick_js(limit, accept));
    async move {
        let value = result.await.map_err(|e| {
            if e.as_string().as_deref() == Some("size") {
                "settings_view.connections.ssh_config.file_limit"
            } else {
                "sessionManager.batchAdd.file_read_failed"
            }
        })?;
        if value.is_null() {
            return Ok(None);
        }
        let data: Uint8Array = value.unchecked_into();
        let bytes = Zeroizing::new(data.to_vec());
        data.fill(0, 0, data.length());
        Ok(Some(bytes))
    }
}
