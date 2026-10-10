#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::NativeVideoFrame;
#[cfg(target_os = "macos")]
pub(crate) use macos::VideoDecoder;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::NativeVideoFrame;
#[cfg(target_os = "windows")]
pub(crate) use windows::VideoDecoder;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::NativeVideoFrame;
#[cfg(target_os = "linux")]
pub(crate) use linux::VideoDecoder;

/// A native path is requested only after the window's renderer has advertised its import support.
#[derive(Clone)]
pub enum NativeVideoDevice {
    #[cfg(target_os = "macos")]
    Metal,
    #[cfg(target_os = "windows")]
    DirectX(::windows::Win32::Graphics::Direct3D11::ID3D11Device),
    #[cfg(target_os = "linux")]
    DmaBuf,
}

pub(crate) struct VideoFrame {
    // Native decoders deliver BGRA directly; geometric image operations preserve channel order.
    pub image: image::DynamicImage,
    pub timestamp: std::time::Duration,
    pub duration: std::time::Duration,
}
