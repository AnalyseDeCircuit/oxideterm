use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    collections::VecDeque,
    sync::{Mutex, mpsc},
};

#[derive(Clone)]
pub(crate) enum AnimationInput {
    Image(Arc<Image>),
    Bytes(Arc<[u8]>),
}

impl AsRef<[u8]> for AnimationInput {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Image(image) => &image.bytes,
            Self::Bytes(bytes) => bytes,
        }
    }
}

fn frames(input: AnimationInput, format: ImageFormat) -> Result<image::Frames<'static>> {
    match format {
        ImageFormat::Gif => Ok(GifDecoder::new(Cursor::new(input))?.into_frames()),
        ImageFormat::WebP => {
            let mut decoder = WebPDecoder::new(Cursor::new(input))?;
            decoder.set_background_color(Rgba([0; 4]))?;
            Ok(decoder.into_frames())
        }
        _ => unreachable!("animation format checked by caller"),
    }
}

struct Request {
    index: usize,
    reply: Option<futures::channel::oneshot::Sender<Result<Arc<Vec<u8>>, ImageCacheError>>>,
}

/// The channel owns one decoder thread; dropping the last image closes the channel.
pub(crate) struct AnimationStream {
    sender: mpsc::SyncSender<Request>,
    cached: Arc<Mutex<VecDeque<(usize, Arc<Vec<u8>>)>>>,
    count: usize,
    decoder_bytes: usize,
    cancelled: Arc<AtomicBool>,
}

impl Drop for AnimationStream {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl AnimationStream {
    pub(crate) fn open(
        input: AnimationInput,
        format: ImageFormat,
        native: (u32, u32),
        target: (u32, u32),
        turns: u8,
    ) -> Result<Option<RenderImage>> {
        // Validate/index the same composed frames as the full decoder, including skipped
        // corrupt frames. Large animations discard their pilot payload and decode on demand.
        let mut delays = Vec::new();
        let mut compact = Some(RenderImage::new(SmallVec::<[image::Frame; 1]>::new()));
        let mut resizer = fast_image_resize::Resizer::new();
        for frame in frames(input.clone(), format)? {
            match frame {
                Ok(frame) => {
                    let delay = frame.delay();
                    delays.push(delay);
                    if delays.len() > 1
                        && compact
                            .as_ref()
                            .is_some_and(|image| image.retained_bytes() > 32 * 1024 * 1024)
                    {
                        compact = None;
                    }
                    if let Some(image) = &mut compact {
                        let pixels = super::decoded_size::resize_frame(
                            frame.into_buffer(),
                            target,
                            &mut resizer,
                        )?;
                        let mut pixels = super::decoded_size::rotate(pixels, turns);
                        oxideterm_image_decoder::to_bgra(&mut pixels);
                        image.push_decoded_frame(image::Frame::from_parts(pixels, 0, 0, delay))?;
                        // Cheap animations need no persistent codec canvas or worker thread.
                        if delays.len() > 1 && image.retained_bytes() > 32 * 1024 * 1024 {
                            compact = None;
                        }
                    }
                }
                Err(error) => {
                    log::debug!("Skipping animated image frame due to decode error: {error}")
                }
            }
        }
        anyhow::ensure!(
            !delays.is_empty(),
            "animated image contains no decodable frames"
        );
        if let Some(image) = compact {
            return Ok(Some(image));
        }
        drop(resizer);
        Self::spawn(input, format, native, target, turns, delays).map(Some)
    }

    pub(super) fn spawn(
        input: AnimationInput,
        format: ImageFormat,
        native: (u32, u32),
        target: (u32, u32),
        turns: u8,
        delays: Vec<image::Delay>,
    ) -> Result<RenderImage> {
        let count = delays.len();
        let cached = Arc::new(Mutex::new(VecDeque::new()));
        let (sender, receiver) = mpsc::sync_channel::<Request>(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let stream = Arc::new(Self {
            sender,
            cached: cached.clone(),
            count,
            decoder_bytes: native.0 as usize * native.1 as usize * 4 * 3,
            cancelled: cancelled.clone(),
        });
        std::thread::Builder::new()
            .name("image-animation".into())
            .spawn(move || {
                // image::Frames is not Send; keep the iterator and compositor on this thread.
                let mut decoder = None;
                let mut position = 0usize;
                let mut resizer = fast_image_resize::Resizer::new();
                while let Ok(request) = receiver.recv() {
                    if request
                        .reply
                        .as_ref()
                        .is_some_and(|reply| reply.is_canceled())
                    {
                        continue;
                    }
                    let result = (|| -> Result<Arc<Vec<u8>>, ImageCacheError> {
                        let present = cached
                            .lock()
                            .unwrap()
                            .iter()
                            .any(|(index, _)| *index == request.index);
                        if decoder.is_none() || (!present && position > request.index) {
                            decoder = Some(frames(input.clone(), format)?);
                            position = 0;
                        }
                        let end = (request.index + 1).min(count - 1);
                        while position <= end {
                            if cancelled.load(Ordering::Acquire) {
                                return Err(anyhow::anyhow!("animation decoder stopped").into());
                            }
                            let frame = loop {
                                match decoder.as_mut().unwrap().next() {
                                    Some(Ok(frame)) => break frame,
                                    Some(Err(error)) => log::debug!(
                                        "Skipping animated image frame due to decode error: {error}"
                                    ),
                                    None => {
                                        return Err(anyhow::anyhow!(
                                            "animation ended before its indexed frame"
                                        )
                                        .into());
                                    }
                                }
                            };
                            if position >= request.index {
                                let pixels = super::decoded_size::resize_frame(
                                    frame.into_buffer(),
                                    target,
                                    &mut resizer,
                                )?;
                                let mut pixels =
                                    super::decoded_size::rotate(pixels, turns).into_raw();
                                oxideterm_image_decoder::to_bgra(&mut pixels);
                                let mut ready = cached.lock().unwrap();
                                ready.retain(|(index, _)| *index != position);
                                ready.push_back((position, Arc::new(pixels)));
                                while ready.len() > 3 {
                                    ready.pop_front();
                                }
                            }
                            position += 1;
                        }
                        cached
                            .lock()
                            .unwrap()
                            .iter()
                            .find(|(index, _)| *index == request.index)
                            .map(|(_, pixels)| pixels.clone())
                            .ok_or_else(|| anyhow::anyhow!("animation frame is unavailable").into())
                    })();
                    if let Some(reply) = request.reply {
                        let _ = reply.send(result);
                    } else if let Err(error) = result {
                        log::debug!("Animation prefetch failed: {error}");
                    }
                }
            })?;
        futures::executor::block_on(stream.prepare(0))?;
        let dimensions = if turns % 2 == 1 {
            (target.1, target.0)
        } else {
            target
        };
        Ok(RenderImage::from_animation(
            stream,
            delays,
            crate::size(dimensions.0.into(), dimensions.1.into()),
        ))
    }

    pub(crate) fn ready(&self, index: usize) -> bool {
        self.cached.lock().unwrap().iter().any(|(i, _)| *i == index)
    }

    pub(crate) fn prefetch(&self, index: usize) {
        let _ = self.sender.try_send(Request {
            index: index % self.count,
            reply: None,
        });
    }

    pub(crate) async fn prepare(&self, index: usize) -> Result<(), ImageCacheError> {
        self.request(index).await.map(|_| ())
    }

    async fn request(&self, index: usize) -> Result<Arc<Vec<u8>>, ImageCacheError> {
        if let Some(pixels) = self
            .cached
            .lock()
            .unwrap()
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, p)| p.clone())
        {
            return Ok(pixels);
        }
        let (reply, receive) = futures::channel::oneshot::channel();
        self.sender
            .send(Request {
                index,
                reply: Some(reply),
            })
            .map_err(|_| anyhow::anyhow!("animation decoder stopped"))?;
        receive
            .await
            .map_err(|_| anyhow::anyhow!("animation decoder stopped"))?
    }

    pub(crate) fn pixels(&self, index: usize) -> Result<Vec<u8>> {
        let pixels =
            futures::executor::block_on(self.request(index)).map_err(anyhow::Error::new)?;
        self.prefetch(index + 1);
        Ok((*pixels).clone())
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        self.cached
            .lock()
            .unwrap()
            .iter()
            .map(|(_, pixels)| pixels.capacity())
            .sum::<usize>()
    }

    pub(crate) fn working_bytes(&self) -> usize {
        self.decoder_bytes
    }
}
