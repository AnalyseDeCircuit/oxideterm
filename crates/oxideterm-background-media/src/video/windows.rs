use super::VideoFrame;
use crate::{LoopCount, MediaError, MediaInfo, MemoryBudget, PixelLease, pixels::pixel_bytes};
use std::{os::windows::ffi::OsStrExt, path::Path, sync::Arc, time::Duration};
use windows::{
    Win32::{
        Graphics::{Direct3D11::*, Dxgi::Common::*},
        Media::MediaFoundation::*,
        System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize},
    },
    core::{Interface, PCWSTR},
};

pub struct NativeVideoFrame {
    pub view: ID3D11ShaderResourceView,
    pub rotation: u32,
    pub timestamp: Duration,
    pub duration: Duration,
    bytes: usize,
    // MF must not recycle the decoder surface until the GPU copy and presentation have finished.
    _sample: IMFSample,
    _runtime: Arc<MediaFoundationRuntime>,
}

// MF samples are free-threaded. The frame is immutable after publication, and D3D context
// access is serialized by ID3D11Multithread; retaining the sample prevents decoder reuse.
unsafe impl Send for NativeVideoFrame {}
unsafe impl Sync for NativeVideoFrame {}

impl NativeVideoFrame {
    pub fn byte_len(&self) -> usize {
        self.bytes
    }
}

struct MediaFoundationRuntime;
impl Drop for MediaFoundationRuntime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
        }
    }
}

struct MediaFoundation {
    runtime: Option<Arc<MediaFoundationRuntime>>,
}
impl MediaFoundation {
    fn new() -> Result<Self, MediaError> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(decode_error)?;
            if let Err(error) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                CoUninitialize();
                return Err(decode_error(error));
            }
        }
        Ok(Self {
            runtime: Some(Arc::new(MediaFoundationRuntime)),
        })
    }
}
impl Drop for MediaFoundation {
    fn drop(&mut self) {
        unsafe {
            // COM belongs to this worker; MF remains alive while published samples are retained.
            self.runtime.take();
            CoUninitialize();
        }
    }
}

pub(crate) struct VideoDecoder {
    pub info: MediaInfo,
    reader: IMFSourceReader,
    stride: i32,
    raw_width: u32,
    raw_height: u32,
    rotation: u32,
    frame_duration: Duration,
    first_pts: Option<i64>,
    device: Option<ID3D11Device>,
    _manager: Option<IMFDXGIDeviceManager>,
    _lease: PixelLease,
    // COM and MF are balanced only after every reader and buffer has been released.
    _runtime: MediaFoundation,
}

fn decode_error(error: windows::core::Error) -> MediaError {
    MediaError::Decode(error.to_string())
}

fn default_stride(format: &IMFMediaType, width: u32) -> windows::core::Result<i32> {
    unsafe {
        match format.GetUINT32(&MF_MT_DEFAULT_STRIDE) {
            Ok(stride) => Ok(stride as i32),
            Err(error) if error.code() == MF_E_ATTRIBUTENOTFOUND => {
                // MF may omit the minimum stride after RGB conversion. IMFMediaBuffer::Lock
                // exposes contiguous pixels, so the subtype and width define their stride.
                let subtype = format.GetGUID(&MF_MT_SUBTYPE)?;
                MFGetStrideForBitmapInfoHeader(subtype.data1, width)
            }
            Err(error) => Err(error),
        }
    }
}

impl VideoDecoder {
    pub fn open(
        path: &Path,
        budget: &MemoryBudget,
        cancelled: &std::sync::atomic::AtomicBool,
        software: bool,
        native_device: Option<&crate::NativeVideoDevice>,
    ) -> Result<Self, MediaError> {
        if cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return Err(MediaError::Cancelled);
        }
        let runtime = MediaFoundation::new()?;
        unsafe {
            let device =
                native_device.map(|crate::NativeVideoDevice::DirectX(device)| device.clone());
            let manager = if let Some(device) = &device {
                // MF and GPUI submit to the same immediate context from different threads.
                let _ = device
                    .GetImmediateContext()
                    .map_err(decode_error)?
                    .cast::<ID3D11Multithread>()
                    .map_err(decode_error)?
                    .SetMultithreadProtected(true);
                let mut token = 0;
                let mut manager = None;
                MFCreateDXGIDeviceManager(&mut token, &mut manager).map_err(decode_error)?;
                let manager = manager.ok_or(MediaError::Unsupported)?;
                manager.ResetDevice(device, token).map_err(decode_error)?;
                Some(manager)
            } else {
                None
            };
            let path = path
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>();
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 3).map_err(decode_error)?;
            let attributes = attributes.ok_or(MediaError::Unsupported)?;
            if let Some(manager) = &manager {
                attributes
                    .SetUnknown(&MF_SOURCE_READER_D3D_MANAGER, manager)
                    .map_err(decode_error)?;
            }
            attributes
                .SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)
                .map_err(decode_error)?;
            attributes
                .SetUINT32(
                    &MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,
                    u32::from(!software),
                )
                .map_err(decode_error)?;
            attributes
                .SetUINT32(&MF_SOURCE_READER_DISABLE_DXVA, u32::from(software))
                .map_err(decode_error)?;
            let reader = MFCreateSourceReaderFromURL(PCWSTR(path.as_ptr()), &attributes)
                .map_err(decode_error)?;
            reader
                .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
                .map_err(decode_error)?;
            reader
                .SetStreamSelection(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, true)
                .map_err(decode_error)?;
            let native = reader
                .GetNativeMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, 0)
                .map_err(decode_error)?;
            if native.GetGUID(&MF_MT_SUBTYPE).map_err(decode_error)? != MFVideoFormat_H264 {
                return Err(MediaError::Unsupported);
            }
            if native
                .GetUINT32(&MF_MT_MPEG2_PROFILE)
                .is_ok_and(|profile| profile > 100)
            {
                return Err(MediaError::Unsupported);
            }
            let transfer = native.GetUINT32(&MF_MT_TRANSFER_FUNCTION).unwrap_or(0);
            if transfer == MFVideoTransFunc_2084.0 as u32
                || transfer == MFVideoTransFunc_HLG.0 as u32
            {
                return Err(MediaError::Unsupported);
            }
            let dimensions = native.GetUINT64(&MF_MT_FRAME_SIZE).map_err(decode_error)?;
            let raw_width = (dimensions >> 32) as u32;
            let raw_height = dimensions as u32;
            let lease = budget.reserve(
                pixel_bytes(raw_width, raw_height)?
                    .checked_mul(4)
                    .ok_or(MediaError::ResourceExhausted)?,
            )?;
            let rotation = native.GetUINT32(&MF_MT_VIDEO_ROTATION).unwrap_or(0);
            if ![0, 90, 180, 270].contains(&rotation) {
                return Err(MediaError::Unsupported);
            }
            let format = MFCreateMediaType().map_err(decode_error)?;
            format
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(decode_error)?;
            format
                .SetGUID(
                    &MF_MT_SUBTYPE,
                    &if device.is_some() {
                        MFVideoFormat_ARGB32
                    } else {
                        MFVideoFormat_RGB32
                    },
                )
                .map_err(decode_error)?;
            reader
                .SetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, None, &format)
                .map_err(decode_error)?;
            let format = reader
                .GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)
                .map_err(decode_error)?;
            let size = format.GetUINT64(&MF_MT_FRAME_SIZE).map_err(decode_error)?;
            if size != dimensions {
                return Err(MediaError::Unsupported);
            }
            let stride = if device.is_some() {
                0
            } else {
                default_stride(&format, raw_width).map_err(decode_error)?
            };
            let rate = native.GetUINT64(&MF_MT_FRAME_RATE).map_err(decode_error)?;
            let numerator = (rate >> 32) as u32;
            let denominator = rate as u32;
            if numerator == 0 || denominator == 0 {
                return Err(MediaError::Unsupported);
            }
            let (width, height) = if rotation == 90 || rotation == 270 {
                (raw_height, raw_width)
            } else {
                (raw_width, raw_height)
            };
            Ok(Self {
                info: MediaInfo {
                    width,
                    height,
                    animated: true,
                    frame_count: None,
                    loops: LoopCount::Infinite,
                },
                reader,
                stride,
                raw_width,
                raw_height,
                rotation,
                frame_duration: Duration::from_secs_f64(denominator as f64 / numerator as f64),
                first_pts: None,
                device,
                _manager: manager,
                _lease: lease,
                _runtime: runtime,
            })
        }
    }

    fn refresh_output_format(&mut self) -> Result<(), MediaError> {
        unsafe {
            let format = self
                .reader
                .GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)
                .map_err(decode_error)?;
            let size = format.GetUINT64(&MF_MT_FRAME_SIZE).map_err(decode_error)?;
            let subtype = format.GetGUID(&MF_MT_SUBTYPE).map_err(decode_error)?;
            let expected = if self.device.is_some() {
                MFVideoFormat_ARGB32
            } else {
                MFVideoFormat_RGB32
            };
            // MF can refine stride and color metadata when producing the first sample.
            // Accept that notification only while the negotiated pixel layout stays supported.
            if size != (u64::from(self.raw_width) << 32 | u64::from(self.raw_height))
                || subtype != expected
            {
                return Err(MediaError::Unsupported);
            }
            if self.device.is_none() {
                self.stride = default_stride(&format, self.raw_width).map_err(decode_error)?;
            }
            Ok(())
        }
    }

    pub fn next_native(
        &mut self,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Option<NativeVideoFrame>, MediaError> {
        let device = self.device.clone().ok_or(MediaError::Unsupported)?;
        unsafe {
            loop {
                if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                    return Err(MediaError::Cancelled);
                }
                let mut flags = 0;
                let mut pts = 0;
                let mut sample = None;
                self.reader
                    .ReadSample(
                        MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                        0,
                        None,
                        Some(&mut flags),
                        Some(&mut pts),
                        Some(&mut sample),
                    )
                    .map_err(decode_error)?;
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    return Ok(None);
                }
                if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                    self.refresh_output_format()?;
                }
                let Some(sample) = sample else { continue };
                let buffer = sample.GetBufferByIndex(0).map_err(decode_error)?;
                let buffer: IMFDXGIBuffer = buffer.cast().map_err(decode_error)?;
                let mut resource = std::ptr::null_mut();
                buffer
                    .GetResource(&ID3D11Texture2D::IID, &mut resource)
                    .map_err(decode_error)?;
                let source = ID3D11Texture2D::from_raw(resource);
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                source.GetDesc(&mut desc);
                if desc.Width < self.raw_width
                    || desc.Height < self.raw_height
                    || desc.SampleDesc.Count != 1
                    || ![DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8X8_UNORM]
                        .contains(&desc.Format)
                {
                    return Err(MediaError::Unsupported);
                }
                let subresource = buffer.GetSubresourceIndex().map_err(decode_error)?;
                let texture = if desc.ArraySize == 1
                    && subresource == 0
                    && desc.Width == self.raw_width
                    && desc.Height == self.raw_height
                    && desc.BindFlags & D3D11_BIND_SHADER_RESOURCE.0 as u32 != 0
                {
                    source.clone()
                } else {
                    // Decoder arrays need not permit sampling. Keep this conversion entirely on the GPU.
                    desc.Width = self.raw_width;
                    desc.Height = self.raw_height;
                    desc.ArraySize = 1;
                    desc.MipLevels = 1;
                    desc.BindFlags = D3D11_BIND_SHADER_RESOURCE.0 as u32;
                    desc.Usage = D3D11_USAGE_DEFAULT;
                    desc.CPUAccessFlags = 0;
                    desc.MiscFlags = 0;
                    let mut texture = None;
                    device
                        .CreateTexture2D(&desc, None, Some(&mut texture))
                        .map_err(decode_error)?;
                    let texture = texture.ok_or(MediaError::Unsupported)?;
                    let context = device.GetImmediateContext().map_err(decode_error)?;
                    context.CopySubresourceRegion(
                        &texture,
                        0,
                        0,
                        0,
                        0,
                        &source,
                        subresource,
                        Some(&D3D11_BOX {
                            left: 0,
                            top: 0,
                            front: 0,
                            right: self.raw_width,
                            bottom: self.raw_height,
                            back: 1,
                        }),
                    );
                    texture
                };
                let mut view = None;
                device
                    .CreateShaderResourceView(&texture, None, Some(&mut view))
                    .map_err(decode_error)?;
                let first = *self.first_pts.get_or_insert(pts);
                let duration = sample
                    .GetSampleDuration()
                    .ok()
                    .filter(|duration| *duration > 0)
                    .map(|duration| Duration::from_nanos(duration as u64 * 100))
                    .unwrap_or(self.frame_duration);
                return Ok(Some(NativeVideoFrame {
                    view: view.ok_or(MediaError::Unsupported)?,
                    rotation: self.rotation,
                    timestamp: Duration::from_nanos(pts.saturating_sub(first).max(0) as u64 * 100),
                    duration,
                    bytes: pixel_bytes(desc.Width, desc.Height)?,
                    _sample: sample,
                    _runtime: self
                        ._runtime
                        .runtime
                        .as_ref()
                        .expect("active MF runtime")
                        .clone(),
                }));
            }
        }
    }

    pub fn next(
        &mut self,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Option<VideoFrame>, MediaError> {
        unsafe {
            loop {
                if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                    return Err(MediaError::Cancelled);
                }
                let mut flags = 0;
                let mut pts = 0;
                let mut sample = None;
                self.reader
                    .ReadSample(
                        MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                        0,
                        None,
                        Some(&mut flags),
                        Some(&mut pts),
                        Some(&mut sample),
                    )
                    .map_err(decode_error)?;
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    return Ok(None);
                }
                if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                    self.refresh_output_format()?;
                }
                let Some(sample) = sample else {
                    continue;
                };
                let buffer = sample.ConvertToContiguousBuffer().map_err(decode_error)?;
                let mut data = std::ptr::null_mut();
                let mut length = 0;
                buffer
                    .Lock(&mut data, None, Some(&mut length))
                    .map_err(decode_error)?;
                struct Unlock<'a>(&'a IMFMediaBuffer);
                impl Drop for Unlock<'_> {
                    fn drop(&mut self) {
                        unsafe {
                            let _ = self.0.Unlock();
                        }
                    }
                }
                let _unlock = Unlock(&buffer);
                let stride = self.stride.unsigned_abs() as usize;
                let width = self.raw_width as usize;
                let height = self.raw_height as usize;
                if data.is_null()
                    || stride < width * 4
                    || stride
                        .checked_mul(height)
                        .is_none_or(|bytes| bytes > length as usize)
                {
                    return Err(MediaError::Decode("invalid video buffer layout".into()));
                }
                let mut bgra = vec![0; pixel_bytes(self.raw_width, self.raw_height)?];
                for y in 0..height {
                    let row = if self.stride < 0 { height - 1 - y } else { y };
                    let src = std::slice::from_raw_parts(data.add(row * stride), width * 4);
                    for (src, dst) in src
                        .chunks_exact(4)
                        .zip(bgra[y * width * 4..(y + 1) * width * 4].chunks_exact_mut(4))
                    {
                        dst.copy_from_slice(&[src[0], src[1], src[2], 255]);
                    }
                }
                let image = image::RgbaImage::from_raw(self.raw_width, self.raw_height, bgra)
                    .ok_or(MediaError::Unsupported)?;
                let image = match self.rotation {
                    90 => image::imageops::rotate90(&image),
                    180 => image::imageops::rotate180(&image),
                    270 => image::imageops::rotate270(&image),
                    _ => image,
                };
                let first = *self.first_pts.get_or_insert(pts);
                let timestamp = Duration::from_nanos(pts.saturating_sub(first).max(0) as u64 * 100);
                let duration = sample
                    .GetSampleDuration()
                    .ok()
                    .filter(|duration| *duration > 0)
                    .map(|duration| Duration::from_nanos(duration as u64 * 100))
                    .unwrap_or(self.frame_duration);
                return Ok(Some(VideoFrame {
                    image: image::DynamicImage::ImageRgba8(image),
                    timestamp,
                    duration,
                }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_output_accepts_missing_stride_and_preserves_explicit_orientation() {
        let _runtime = MediaFoundation::new().unwrap();
        unsafe {
            let format = MFCreateMediaType().unwrap();
            format
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                .unwrap();
            assert_eq!(default_stride(&format, 64).unwrap().unsigned_abs(), 256);
            for stride in [256_i32, -256_i32] {
                format
                    .SetUINT32(&MF_MT_DEFAULT_STRIDE, stride as u32)
                    .unwrap();
                assert_eq!(default_stride(&format, 64).unwrap(), stride);
            }
            // A malformed attribute is a real error, not an omitted optional value.
            format
                .SetGUID(&MF_MT_DEFAULT_STRIDE, &MFVideoFormat_RGB32)
                .unwrap();
            assert!(default_stride(&format, 64).is_err());
        }
    }
}
