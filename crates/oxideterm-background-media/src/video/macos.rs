use super::VideoFrame;
use crate::{LoopCount, MediaError, MediaInfo, MemoryBudget, PixelLease, pixels::pixel_bytes};
use objc2::{rc::Retained, runtime::AnyObject};
use objc2_av_foundation::{
    AVAsset, AVAssetReader, AVAssetReaderStatus, AVAssetReaderTrackOutput,
    AVMediaCharacteristicContainsHDRVideo, AVMediaTypeVideo,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::CMFormatDescription;
use objc2_core_video::*;
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use std::{path::Path, time::Duration};

pub struct NativeVideoFrame {
    pub buffer: CFRetained<CVPixelBuffer>,
    pub rotation: u16,
    pub timestamp: Duration,
    pub duration: Duration,
}

// The decoder publishes retained, immutable pixel buffers. Holding the reference prevents
// the native pool from reusing their storage while another thread or the GPU reads it.
unsafe impl Send for NativeVideoFrame {}
unsafe impl Sync for NativeVideoFrame {}

impl NativeVideoFrame {
    pub fn byte_len(&self) -> usize {
        CVPixelBufferGetDataSize(&self.buffer)
    }
}

pub(crate) struct VideoDecoder {
    pub info: MediaInfo,
    reader: Retained<AVAssetReader>,
    output: Retained<AVAssetReaderTrackOutput>,
    rotation: u16,
    frame_duration: Duration,
    first_pts: Option<Duration>,
    _lease: PixelLease,
}

impl VideoDecoder {
    #[allow(deprecated)]
    pub fn open(
        path: &Path,
        budget: &MemoryBudget,
        cancelled: &std::sync::atomic::AtomicBool,
        _software: bool,
        _device: Option<&crate::NativeVideoDevice>,
    ) -> Result<Self, MediaError> {
        objc2::rc::autoreleasepool(|_| unsafe {
            let path = path.to_str().ok_or(MediaError::Unsupported)?;
            let asset = AVAsset::assetWithURL(&NSURL::fileURLWithPath(&NSString::from_str(path)));
            let tracks =
                asset.tracksWithMediaType(AVMediaTypeVideo.ok_or(MediaError::Unsupported)?);
            if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                return Err(MediaError::Cancelled);
            }
            let track = tracks.firstObject().ok_or(MediaError::Unsupported)?;
            if AVMediaCharacteristicContainsHDRVideo
                .is_some_and(|hdr| track.hasMediaCharacteristic(hdr))
            {
                return Err(MediaError::Unsupported);
            }
            let descriptions = track.formatDescriptions();
            let description = descriptions.firstObject().ok_or(MediaError::Unsupported)?;
            // AVAssetTrack documents every entry as a CMFormatDescription, bridged through NSArray.
            let format = &*(Retained::as_ptr(&description).cast::<CMFormatDescription>());
            if format.media_sub_type() != u32::from_be_bytes(*b"avc1") {
                return Err(MediaError::Unsupported);
            }
            let size = track.naturalSize();
            if !size.width.is_finite()
                || !size.height.is_finite()
                || size.width < 1.0
                || size.height < 1.0
            {
                return Err(MediaError::Unsupported);
            }
            let width = size.width.ceil() as u32;
            let height = size.height.ceil() as u32;
            let lease = budget.reserve(
                pixel_bytes(width, height)?
                    .checked_mul(4)
                    .ok_or(MediaError::ResourceExhausted)?,
            )?;
            let transform = track.preferredTransform();
            let rotation = if transform.a.abs() < 0.01 && (transform.b - 1.0).abs() < 0.01 {
                90
            } else if (transform.a + 1.0).abs() < 0.01 && transform.b.abs() < 0.01 {
                180
            } else if transform.a.abs() < 0.01 && (transform.b + 1.0).abs() < 0.01 {
                270
            } else if (transform.a - 1.0).abs() < 0.01 && transform.b.abs() < 0.01 {
                0
            } else {
                return Err(MediaError::Unsupported);
            };
            let reader = AVAssetReader::assetReaderWithAsset_error(&asset)
                .map_err(|_| MediaError::Decode("AVAssetReader could not open video".into()))?;
            let key = NSString::from_str("PixelFormatType");
            let pixel_format = NSNumber::new_u32(kCVPixelFormatType_32BGRA);
            let metal_key = NSString::from_str("MetalCompatibility");
            let surface_key = NSString::from_str("IOSurfaceProperties");
            let metal = NSNumber::new_bool(true);
            let surface = NSDictionary::<NSString, AnyObject>::new();
            let settings = NSDictionary::<NSString, AnyObject>::from_slices(
                &[&*key, &*metal_key, &*surface_key],
                &[&*pixel_format, &*metal, &*surface],
            );
            let output = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(
                &track,
                Some(&settings),
            );
            output.setAlwaysCopiesSampleData(false);
            if !reader.canAddOutput(&output) {
                return Err(MediaError::Unsupported);
            }
            reader.addOutput(&output);
            if !reader.startReading() {
                return Err(MediaError::Decode(
                    "AVAssetReader could not start video".into(),
                ));
            }
            let fps = track.nominalFrameRate();
            if !fps.is_finite() || fps <= 0.0 {
                return Err(MediaError::Unsupported);
            }
            let (width, height) = if rotation == 90 || rotation == 270 {
                (height, width)
            } else {
                (width, height)
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
                output,
                rotation,
                frame_duration: Duration::try_from_secs_f64(1.0 / fps as f64)
                    .map_err(|_| MediaError::Unsupported)?,
                first_pts: None,
                _lease: lease,
            })
        })
    }

    pub fn next_native(
        &mut self,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Option<NativeVideoFrame>, MediaError> {
        objc2::rc::autoreleasepool(|_| unsafe {
            if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                return Err(MediaError::Cancelled);
            }
            let Some(sample) = self.output.copyNextSampleBuffer() else {
                return if self.reader.status() == AVAssetReaderStatus::Completed {
                    Ok(None)
                } else {
                    Err(MediaError::Decode(
                        self.reader
                            .error()
                            .map(|error| error.localizedDescription().to_string())
                            .unwrap_or_else(|| "AVAssetReader failed while reading video".into()),
                    ))
                };
            };
            let buffer = sample
                .image_buffer()
                .ok_or_else(|| MediaError::Decode("video sample contains no pixels".into()))?;
            if CVPixelBufferGetPixelFormatType(&buffer) != kCVPixelFormatType_32BGRA {
                return Err(MediaError::Unsupported);
            }
            let width = CVPixelBufferGetWidth(&buffer) as u32;
            let height = CVPixelBufferGetHeight(&buffer) as u32;
            if pixel_bytes(width, height)? > pixel_bytes(self.info.width, self.info.height)? {
                return Err(MediaError::Unsupported);
            }
            let pts = sample.presentation_time_stamp().seconds();
            if !pts.is_finite() || pts < 0.0 {
                return Err(MediaError::Decode("invalid video timestamp".into()));
            }
            let pts = Duration::from_secs_f64(pts);
            let first = *self.first_pts.get_or_insert(pts);
            let duration = sample.duration().seconds();
            let duration = if duration.is_finite() && duration > 0.0 {
                Duration::from_secs_f64(duration)
            } else {
                self.frame_duration
            };
            Ok(Some(NativeVideoFrame {
                buffer,
                rotation: self.rotation,
                timestamp: pts.saturating_sub(first),
                duration,
            }))
        })
    }

    pub fn next(
        &mut self,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Option<VideoFrame>, MediaError> {
        let Some(frame) = self.next_native(cancelled)? else {
            return Ok(None);
        };
        unsafe {
            let buffer = &frame.buffer;
            let width = CVPixelBufferGetWidth(buffer) as u32;
            let height = CVPixelBufferGetHeight(buffer) as u32;
            if CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags::ReadOnly) != 0 {
                return Err(MediaError::Decode(
                    "video pixels could not be mapped".into(),
                ));
            }
            struct Unlock<'a>(&'a CVPixelBuffer);
            impl Drop for Unlock<'_> {
                fn drop(&mut self) {
                    unsafe {
                        CVPixelBufferUnlockBaseAddress(self.0, CVPixelBufferLockFlags::ReadOnly);
                    }
                }
            }
            let _unlock = Unlock(&buffer);
            let stride = CVPixelBufferGetBytesPerRow(&buffer);
            let data = CVPixelBufferGetBaseAddress(&buffer).cast::<u8>();
            if data.is_null() || stride < width as usize * 4 {
                return Err(MediaError::Decode("invalid video row layout".into()));
            }
            let mut bgra = vec![0; pixel_bytes(width, height)?];
            for y in 0..height as usize {
                let row = std::slice::from_raw_parts(data.add(y * stride), width as usize * 4);
                bgra[y * width as usize * 4..(y + 1) * width as usize * 4].copy_from_slice(row);
            }
            let image =
                image::RgbaImage::from_raw(width, height, bgra).ok_or(MediaError::Unsupported)?;
            let image = match self.rotation {
                90 => image::imageops::rotate90(&image),
                180 => image::imageops::rotate180(&image),
                270 => image::imageops::rotate270(&image),
                _ => image,
            };
            Ok(Some(VideoFrame {
                image: image::DynamicImage::ImageRgba8(image),
                timestamp: frame.timestamp,
                duration: frame.duration,
            }))
        }
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        unsafe {
            self.reader.cancelReading();
        }
    }
}
