use gpui::{ImageSource, RenderImage, Task};
use oxideterm_background_media::{BackgroundFit, OutputParams, PlaybackLimits, decode_poster};
use std::{path::PathBuf, sync::Arc};

struct Poster {
    image: Option<Arc<RenderImage>>,
    error: Option<gpui::ImageCacheError>,
    _task: Task<()>,
    _release: gpui::Subscription,
}

pub fn poster_source(path: PathBuf) -> ImageSource {
    ImageSource::Custom(Arc::new(move |window, cx| {
        let path = path.clone();
        let key = Arc::<std::path::Path>::from(path.clone());
        let poster = window.use_keyed_state(key, cx, |window, cx| {
            let decode = cx.background_executor().spawn(async move {
                decode_poster(
                    &path,
                    OutputParams {
                        width: 256,
                        height: 144,
                        fit: BackgroundFit::Cover,
                        blur: 0.0,
                        limits: PlaybackLimits::default(),
                    },
                )
            });
            let task = cx.spawn(async move |poster, cx| {
                let (_, mut frame) = match decode.await {
                    Ok(result) => result,
                    Err(error) => {
                        let _ = poster.update(cx, |poster: &mut Poster, cx| {
                            poster.error =
                                Some(gpui::ImageCacheError::Other(Arc::new(error.into())));
                            cx.notify();
                        });
                        return;
                    }
                };
                let pixels = std::mem::take(&mut frame.pixels);
                let Some(pixels) = image::RgbaImage::from_raw(frame.width, frame.height, pixels)
                else {
                    return;
                };
                let image =
                    Arc::new(RenderImage::new(vec![image::Frame::new(pixels)]).retaining(frame));
                let _ = poster.update(cx, |poster: &mut Poster, cx| {
                    poster.image = Some(image);
                    cx.notify();
                });
            });
            let release = cx.on_release_in(window, |poster: &mut Poster, window, cx| {
                if let Some(image) = poster.image.take() {
                    cx.drop_image(image, Some(window));
                }
            });
            Poster {
                image: None,
                error: None,
                _task: task,
                _release: release,
            }
        });
        let poster = poster.read(cx);
        poster
            .error
            .clone()
            .map(Err)
            .or_else(|| poster.image.clone().map(Ok))
    }))
}
